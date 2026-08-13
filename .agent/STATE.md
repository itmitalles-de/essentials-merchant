# Current State

## Project goal

Provide a compact ERP/inventory and commerce suite for German small businesses,
with Shop Suite Core remaining authoritative for operational and accounting data
while Vendure supplies a separately owned commerce experience.

## Current status

- Default branch: `main` at `76e0fcc` before this handoff migration.
- The first Core-to-Vendure vertical commerce slice is implemented and merged.
- GitHub Actions run `31666197155` for `76e0fcc` completed successfully on
  2026-08-13. The earlier SQLx/Typst CI failures described in the old prompt are
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

## Active work

No branch, pull request, issue, or uncommitted workstream is active. The next
documented focus is failure-recovery coverage for Core/Vendure outages and worker
restarts, not another provider integration.

## Recently completed

- Consolidated visible branding as Shop Suite while preserving internal
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

## Validation

- Current main-branch CI success was verified through GitHub Actions.
- Documentation migration references and paths were checked locally.
- `cargo fmt --check`, placeholder-backed `docker compose config -q`, frontend
  lint, and commerce lint plus five helper tests passed locally. Frontend lint
  retained three existing Fast Refresh warnings and returned success.
- No full Rust integration suite or live vertical deployment test was rerun
  solely for these docs.
- Use the exact scoped and full-flow commands in `README.md` for future changes.

## Last handoff

2026-08-13: introduced the persistent `.agent/` workflow, migrated the real
tasks from the old root `TODO.md`, and removed its stale CI-rerun statement.
