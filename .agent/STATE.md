# Current State

## Project goal

Provide a compact ERP/inventory and commerce suite for German small businesses,
with Merchant Core (Essentials Plus) remaining authoritative for operational and accounting data
while Vendure supplies a separately owned commerce experience.

## Current status

- Default branch: `main`; Marketplace Intelligence is locally complete and awaits its
  focused commit/push at this handoff.
- The first Core-to-Vendure vertical commerce slice is implemented and merged.
- GitHub Actions run `31671308497` for `7d243b6` completed successfully on
  2026-08-13. The earlier SQLx/Typst CI failures described in the historical prompt are
  resolved on current `main`; there is no current CI failure to reproduce.
- No open GitHub issues or pull requests were present when this handoff was written.
- The project is active development, not a claim of production, legal, or DATEV
  compatibility.

## Working

- Rust/Axum Core with PostgreSQL migrations for authentication, company settings,
  customers, VAT, invoices/PDFs, articles/inventory, and sales orders/fulfillment.
- React/Vite administration frontend for the implemented Core workflows.
- Vendure 3.7.2 server, worker, current Dashboard, separate PostgreSQL database,
  and Next.js Storefront.
- Product/net-price/VAT/stock projection from Core to Vendure.
- Idempotent paid-order import into Core with one stock booking.
- Fulfillment/carrier/tracking projection from Core back to Vendure.
- Durable mapping, inbox, and outbox records with leases, backoff, dead state,
  stale-projection protection, and explicit TypeORM/SQLx migrations.
- CI covers Core, frontend, commerce, image builds, a healthy clean Compose stack,
  and the vertical SKU-to-fulfillment flow.
- Additive migration `0009_marketplace_intelligence.sql` introduces the Essentials
  Plus module catalog/permissions, connector health records, and a persistent,
  read-only Amazon Reports job/archive/snapshot/analysis model.
- Marketplace Intelligence is disabled by default. It has a fixture client and
  parsers for Sales & Traffic JSON plus Inventory Planning TSV; Returns and
  Settlement V2 are raw-archive-only registry entries. It uses LWA OAuth in the
  live client, not IAM/SigV4, and the UI offers a synthetic demo connection.
- Deterministic analyses work without an AI provider. An optional OpenAI-compatible
  endpoint receives only allowlisted aggregate metrics and cannot overwrite the
  deterministic result if its JSON is invalid or the provider fails.

## Active work

Marketplace Intelligence has passed its local database, SQLx, frontend, commerce,
and isolated Compose acceptance checks. Commit and push the focused change next;
then the pre-existing focus remains failure-recovery coverage for Core/Vendure
outages and worker restarts. Do not repeat completed Vendure work.

## Recently completed

- Consolidated visible branding as Merchant / Essentials Plus while preserving internal
  compatibility names.
- Fixed SQLx offline CI and pinned Typst 0.12.0 in CI for PDF tests.
- Added the Vendure vertical slice and made the full main-branch CI pass.
- Replaced the generic root handoff with the persistent `.agent/` workflow.

## Known issues

- The vertical test covers duplicate payment delivery and the happy-path flow,
  but not deliberate Core/Vendure outages, expired leases, or worker restarts.
- Vendure 3.7.2 carries upstream production dependency advisories recorded in
  `README.md`. npm's proposed forced downgrade is incompatible and must not be
  applied; update only to a compatible patch and rerun the whole vertical test.
- The integration uses one shared HTTP-header secret. Outside local Compose it
  requires TLS/private networking; rotation is coordinated rather than dual-key.
- Only one default Vendure channel and integer saleable stock are supported;
  fractional Core quantities are rounded down for shop availability.
- Test payment and manual fulfillment are not production providers.
- Correction invoices and reference-tested DATEV EXTF are not implemented.
- A live Amazon seller/role/marketplace/RDT gate has not been run. The current
  registry intentionally avoids RDT-required types in the MVP.
- Full DB-backed Marketplace tests require a disposable PostgreSQL database; do
  not use a developer's existing Compose database or inspect local secrets.

## Next recommended tasks

1. Extend vertical CI with deliberate target outages, worker restarts, lease
   expiry/reclaim, replay, and persisted recovery assertions.
2. After that reliability work, integrate one production payment provider and
   one shipping provider with signed webhooks and reconciliation.
3. Add correction invoices before building DATEV EXTF from immutable entries.

The authoritative prioritized task list is `.agent/TODO.md`.

## Relevant files

- `README.md`: current architecture, setup, validation, and known risks
- `.github/workflows/ci.yml`: authoritative automated checks
- `docker-compose.yml`: full local Core/Vendure/Storefront topology
- `backend/crates/db/src/commerce.rs`: Core integration and idempotent import
- `backend/crates/db/migrations/0008_commerce_integration.sql`: Core adapter schema
- `backend/crates/db/src/invoices.rs`: invoice numbering, snapshots, and lifecycle
- `commerce/server/src/plugins/shop-suite-integration/`: Vendure outbox/worker
- `commerce/storefront/`: Shop API-only Storefront
- `commerce/test/vertical.mjs`: end-to-end vertical acceptance flow
- `backend/crates/server/src/marketplace.rs`: Amazon Reports client, fixture,
  parsers, worker, deterministic analysis, optional provider seam
- `backend/crates/db/migrations/0009_marketplace_intelligence.sql`: module and
  Marketplace Intelligence persistence

## Validation

- Current main-branch CI success was verified through GitHub Actions before the
  Marketplace work began. On a disposable PostgreSQL 16 database, migrations
  `0001` through `0009`, `cargo test` (35 tests), `cargo fmt --check`, offline
  Clippy, `cargo sqlx prepare`, and `cargo sqlx prepare --check` pass.
- Frontend build/lint and commerce lint, tests, and build pass. Frontend lint
  retains its three pre-existing Fast Refresh warnings and returns success.
- An isolated placeholder-backed Compose stack built and became healthy. Its
  existing vertical commerce acceptance test passed; after enabling the optional
  module, a synthetic Sales & Traffic request also reached `succeeded` with one
  stored deterministic analysis.
- No live Amazon credentials, seller account, live carrier request, or live AI
  provider has been used. Use the exact scoped and full-flow commands in
  `README.md` for future changes.

## Last handoff

2026-08-13: completed and locally validated the Merchant / Essentials Plus Marketplace
Intelligence slice; commit and push the focused change, then perform the documented
live Amazon staging gate when an approved seller context is available.
