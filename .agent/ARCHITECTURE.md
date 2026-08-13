# Architecture

This is the concise map of the implemented Essentials+ Merchant system. `README.md` is the
operational command source of truth. Internal `erplite`, `shop-suite-*`, database, volume, crate,
migration, and mapping identifiers are compatibility names and must not be renamed as branding.

## System topology

```text
React Admin -> Rust/Axum Core -> Core PostgreSQL + immutable document volume
                    |   ^
       Core outbox  |   | signed Vendure payment/order events
                    v   |
             Vendure worker -> separate Vendure PostgreSQL + asset volume
                    ^
Next.js Storefront -> Vendure Shop API

Core Marketplace module -> Amazon Reports API v2021-06-30 (read-only)
```

There is no shared database, distributed transaction, multi-tenancy, Kubernetes control plane, or
cross-product runtime library.

## Ownership

- Core owns SKU/master data, net prices/VAT category, available inventory, imported sales orders,
  stock movements, issued/correction invoice snapshots, immutable accounting entries, modules,
  integration audit, and Marketplace Intelligence.
- Vendure owns commerce merchandising, cart/checkout, promotions, payment and fulfillment runtime,
  Shop/Admin APIs, and commerce-side outbox.
- Storefront owns presentation and session proxy behavior only.
- Provider modules own their mappings/audit but never move Core accounting or inventory authority.

## Components

| Component | Location | Responsibility |
| --- | --- | --- |
| Domain | `backend/crates/domain` | Decimal VAT and invoice lifecycle rules |
| Core persistence | `backend/crates/db` | SQLx migrations/repositories, module/integration/accounting/Marketplace data |
| PDF | `backend/crates/pdf` | Immutable ordinary/correction invoice render data and templates |
| Core API | `backend/crates/server` | JWT/module auth, HMAC integration routes, workers, deterministic analysis/export |
| React admin | `frontend` | Module-aware themed workflows, diagnostics, invoices/corrections, Marketplace UI |
| Vendure | `commerce/server` | Vendure 3.7.2 server/worker, explicit TypeORM migrations, integration plugin |
| Storefront | `commerce/storefront` | Shop-API-only synthetic storefront |
| Provider contracts | `commerce/server/src/providers` | Payment/shipping ports, fake adapters, webhook HMAC/replay tests |
| Reliability | `commerce/test/recovery.mjs` | Destructive disposable failure/restart matrix |
| Operations | `ops` | Coordinated backup, empty restore, verification, upgrade rehearsal |

## Essentials+ module contract

`essentials_modules` keeps a stable `module_key` compatibility alias and exposes canonical
`module_id`. Each manifest includes version, thematic group, core/optional/connector type,
`required`, dependencies/conflicts, compatibility, configuration/secret requirements,
API/navigation boundaries, jobs, webhooks, health, ownership, and backup/restore behavior.

States are `not_installed`, `needs_configuration`, `disabled`, `enabled`, and `degraded`.
Administrators see the full catalog; normal users require both enabled state and an explicit grant.
Transitions lock and validate dependencies/conflicts/configuration, mutate atomically, and write an
immutable audit record keyed by idempotency. Required Core modules cannot be disabled. Disabling
preserves all data but causes API guards, navigation, worker claims, scheduled jobs, webhooks, and
synthetic payment/shipping writes to fail closed.

DHL, DPD, Stripe-candidate payment, manual shipping, and Marketplace Intelligence are independent
modules. No connector is implicitly enabled by another.

## Core↔Vendure delivery

Article/stock transactions enqueue a monotonic `vendure.product.project`. The Vendure worker claims
with `FOR UPDATE SKIP LOCKED`, writes product/variant/stock and mappings, and acknowledges Core.
Applied sequence on the variant prevents delayed product/price/stock rollback.

Vendure Authorized/Settled payment events enter its local outbox. The worker sends a stable event
and synthetic order snapshot to Core. Core inbox uniqueness and external-order uniqueness protect
the transaction that creates one order and one stock booking.

Core fulfillment updates order state and `vendure.fulfillment.project` in one transaction. Vendure
creates/advances one fulfillment with carrier/tracking.

Both sides use leases, persisted attempts, capped exponential backoff, dead state, and controlled
audited requeue. Default lease/retry values are production values; `APP_ENV=test` failpoints and
short timing overrides power deterministic restart tests.

## Integration authentication and diagnostics

Every Core adapter route authenticates HMAC-SHA-256 over uppercase method, exact path, Unix
timestamp, nonce, and SHA-256 body hash. Nonces persist in Core, timestamp age is bounded, bodies
are limited to 256 KiB, and current/previous keys support overlap rotation. Vendure signs current
only. Production still requires TLS/private networking.

The administrator diagnostics endpoint aggregates Core queues plus Vendure's signed, sanitized
remote observations. It returns counts, oldest open time, last success/error, event ID/type/state,
attempts, lease timestamps, mappings, health/readiness, and audit—never payloads or customer data.
Vendure requeue uses a signed command queue because Core does not write Vendure's DB.

## Accounting

Issuing allocates an ordinary number and snapshots mutable customer/company/line/tax data. DB
triggers prevent later mutation. A full correction is a separate draft document with its own
number, source reference, reason, reversed Decimal lines/totals, one-per-source uniqueness,
idempotency, PDF reference, and immutable audit. It never creates inventory movement.

Migration 0014 backfills and triggers immutable accounting entries for issued ordinary and
correction invoices. The DATEV renderer reads only those entries, orders deterministically, and
stores an immutable export batch with parameter/payload hashes and exact bytes. `export.datev`
remains disabled until external DATEV checking-program/test-client validation; no compatibility or
tax/legal claim is made.

## Marketplace Intelligence

Connections persist seller, region, marketplace IDs, roles, mode, and logical secret reference,
never tokens. The live transport uses LWA OAuth and `v2021-06-30`; no SigV4 or Amazon write API is
compiled. Manual and scheduled triggers share `amazon_report_runs`, state history, unique in-flight
identity, lease/retry/backoff, raw transport document, decoded document, hashes, parser version,
normalized metrics, compatible snapshots, analysis jobs, and deterministic results.

Sales & Traffic JSON parser v2 follows `reportSpecification` plus separate official ASIN rows and
records date/ASIN granularity in the comparability key. Inventory Planning TSV parser v1 tolerates
unknown/optional/reordered columns. Returns and Settlement V2 are raw-only. Parser failures retain
the immutable raw archive; unknown fixture types never become successfully analysed.

Analysis compares only same report, marketplace, parser, granularity, and period length. It stores
facts/deltas/trend/anomalies/hypotheses/options/evidence/uncertainty/missing data. Export filters to
aggregate allowlisted metrics and recursively removes buyer/customer/address/email/order/comment/
phone fields. There is no external LLM provider and no automatic Amazon mutation.

## Backup/restore boundary

A coordinated backup quiesces both application writers and captures separate logical DB dumps,
Core documents, Vendure assets, module configuration without secrets, redacted Compose metadata,
checksums, repository revision, app/schema versions, timestamp, and explicit store list. Marketplace
raw/normalized data and integration mappings/inbox/outbox are in the Core dump.

Restore verifies every file and refuses any target Compose project with existing containers or
declared volumes. The automated rehearsal restores into a random empty project, compares database
and file invariants, and reruns the vertical flow. This is an implementation proof, not an RPO/RTO
or external storage guarantee.

## Deployment and testing constraints

- Additive Core migrations; explicit Vendure migrations; `synchronize: false`.
- SQLx offline cache is committed and refreshed only against disposable PostgreSQL.
- Core and Vendure schema/app versions travel together in backup metadata.
- Synthetic payment/manual shipping and provider fakes are never production claims.
- Current vendure packages are pinned together at 3.7.2; incompatible forced audit fixes are
  forbidden.
- Full coverage layers are recorded in `docs/VERIFICATION_MATRIX.md`.
