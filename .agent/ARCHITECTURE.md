# Architecture

This document is a concise map of the implemented system. `README.md` remains
authoritative for setup, data-flow detail, commands, known risks, and current
feature status.

## Overview

Merchant is an Essentials Plus monorepo with two explicit business systems, a Storefront, and an optional Marketplace Intelligence module:

```text
React admin -> Rust/Axum Core -> Core PostgreSQL + invoice files
                    |   ^
       Core outbox  |   | Vendure payment/order events
                    v   |
             Vendure worker -> Vendure PostgreSQL + assets
                    ^
Next.js Storefront -> Vendure Shop API
                       ^
Marketplace Intelligence -> Amazon SP-API Reports (read-only)
```

There is no shared database and no distributed transaction. The adapter is an
at-least-once projection/import channel with idempotent consumers.

## Components

| Component | Location | Responsibility |
| --- | --- | --- |
| Domain | `backend/crates/domain/` | VAT calculation and invoice lifecycle rules |
| Persistence | `backend/crates/db/` | SQLx migrations, repositories, inventory, invoices, orders, integration records |
| PDF | `backend/crates/pdf/` | Typst/Jinja invoice rendering data and templates |
| API | `backend/crates/server/` | Axum routes, JWT auth, integration auth, migration/bootstrap |
| Admin UI | `frontend/` | React/Vite administration client for Core APIs |
| Marketplace Intelligence | `backend/crates/server/src/marketplace.rs`, `backend/crates/db/src/marketplace.rs` | Optional Amazon Reports v2021-06-30 job, archive, parser, snapshot, and analysis flow |
| Module catalog | `backend/crates/db/src/modules.rs`, `frontend/src/pages/AdminCenter.tsx` | Essentials Plus module visibility, activation, connector configuration health |
| Vendure | `commerce/server/` | Vendure server, worker, Dashboard, migrations, integration plugin |
| Storefront | `commerce/storefront/` | Next.js German example shop using only the Shop API |
| Vertical test | `commerce/test/vertical.mjs` | Full SKU-to-checkout-to-fulfillment acceptance flow |

## Source-of-truth ownership

- Core: articles/SKUs, net price and VAT category, available stock, imported
  sales orders, invoices, company/customer snapshots, and accounting state.
- Vendure: merchandising, facets/categories, cart, checkout, promotions,
  payments, shop channels, and commerce API state.
- Storefront: presentation and server-side proxy/session behavior only; no DB.

The internal `erplite` names are compatibility identifiers, not stale product
ownership. Do not rename them as presentation cleanup.

## Essentials Plus modules

`essentials_modules` is an additive module catalog. Administrators see the whole catalog;
normal users require an enabled module plus a `user_module_permissions` grant. Optional module
handlers and workers check the enabled state, so disabling removes navigation and stops jobs or
webhooks without deleting historical rows. DHL and DPD are separate connector catalog modules with
configuration health records, not Marketplace Intelligence dependencies.

Marketplace Intelligence is currently read-only: it uses LWA OAuth and Reports API endpoints only.
It stores no OAuth tokens. A common persistent state machine covers manual and scheduled requests:
`queued -> requesting -> polling -> downloading -> parsing -> analysing -> succeeded`; terminal
states are `cancelled`, `fatal`, `failed`, and raw-only `archived`. A raw document is immutable
after SHA-256 archive. Snapshot comparisons require identical report type, granularity, and
comparability key.

## Data flow

### Product, price, and stock: Core to Vendure

Article changes and stock movements update Core and enqueue
`vendure.product.project` in the same PostgreSQL transaction. The Vendure worker
claims Core events, upserts the SKU and mapping, and records the applied Core
sequence so a delayed projection cannot overwrite a newer one.

### Paid order: Vendure to Core

Vendure payment state changes enter the Vendure-local integration outbox. The
worker sends a stable event ID and order snapshot to Core. Core's inbox and
unique external-order key make replays safe; order import and stock movements
commit together and stock is booked once.

### Fulfillment: Core to Vendure

Fulfilling an imported order locks and updates the Core order and enqueues a
fulfillment projection in the same transaction. The worker creates or advances
the Vendure fulfillment and applies carrier/tracking without duplicating a
completed fulfillment.

Both outbox implementations use processing leases, exponential retry capped at
one hour, and a dead state after 20 attempts. Recovery-path CI coverage remains
the next task.

## Persistence and deployment

`docker-compose.yml` runs:

- PostgreSQL 16 with `erplite_db_data` for Core
- Rust backend with `erplite_invoices` for generated PDF files
- Nginx-served administration frontend
- separate PostgreSQL 16 with `vendure_db_data`
- Vendure server and worker sharing `vendure_assets`
- Next.js Storefront

The admin UI and Storefront also join external `proxy_net`. Local defaults expose
admin on 8090, Vendure on 3000, and Storefront on 3001. Back up and restore the
two databases, invoices, and assets independently with compatible app versions.

## Authentication and security

- Human Core API access uses JWT authentication bootstrapped from environment
  configuration.
- Core/Vendure adapter routes use a shared `x-shop-suite-integration-key` secret.
- Marketplace connections keep only logical environment secret references; LWA refresh/client/
  access tokens never leave the server or enter logs. The optional AI provider receives allowlisted,
  aggregated metrics only.
- Vendure has its own cookie and Superadmin credentials.
- All credentials come from local `.env`; only placeholders belong in Git.
- Outside local Compose, protect integration traffic with TLS/private networking.

The current shared-secret rotation model is coordinated, not dual-key.

## Accounting and inventory constraints

- Database and Rust domain rules restrict invoice status transitions.
- Issuing an invoice allocates its number atomically and snapshots mutable master data.
- Only drafts can be edited or deleted; sent financial data remains immutable.
- Stock changes are movements applied atomically to article stock.
- External order and event uniqueness prevents duplicate stock booking.
- DATEV EXTF and correction invoices are not implemented.

## Testing and validation

The command source of truth is `README.md`:

- Rust: format, offline Clippy, SQLx integration tests, Typst PDF tests
- Admin: TypeScript/Vite build and Oxlint
- Commerce: TypeScript checks, helper tests, Vendure/Dashboard/Storefront builds
- Compose: build/start all services and run the vertical acceptance test
- CI: `.github/workflows/ci.yml`

Generated Storefront build output, `node_modules`, the full Vendure-generated
migration, and every adapter implementation should not be loaded for routine
tasks. Start from this map and inspect only the affected paths.

## Important constraints

- Additive Core migrations; explicit Vendure migrations; never schema sync.
- Never generate migrations or SQLx metadata against production.
- Do not treat the test payment/manual fulfillment as production integrations.
- Do not claim legal or DATEV compatibility without authoritative reference tests.
- Keep current source-of-truth ownership even when adding providers or channels.
