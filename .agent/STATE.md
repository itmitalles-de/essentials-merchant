# Current State

## Product and branch

- Visible product name: **Essentials+ Merchant**.
- Repository and compatibility identifiers remain `erplite`; crate, database, volume, migration,
  token-storage, mapping, and `shop-suite-*` identifiers were not renamed.
- Active branch: `agent/essentials-merchant-autonomous`, based on `c7563f4`.
- This is active development, not a production, legal, tax, DATEV, Amazon, payment, or carrier
  certification.

## Implemented and working

- The existing Rust Core↔Vendure vertical remains the ownership boundary: Core owns ERP,
  inventory, imported orders, immutable invoices/accounting, modules, diagnostics, and Marketplace
  Intelligence; Vendure owns commerce and has a separate PostgreSQL database.
- Core and Vendure delivery now has signed HMAC requests, persistent nonces/replay protection,
  current/previous key rotation, request limits, sanitized errors, configurable leases/backoff, and
  deterministic test-only process failpoints.
- Administrator-only integration diagnostics expose queue counts, oldest open events, last
  success/error, leases, mappings, readiness, and audit without payloads, credentials, or buyer
  data. Dead-event requeue is protected, idempotent, and audited.
- The Essentials+ module contract is persisted and enforced server-side. Required Core modules,
  dependencies/conflicts, connector configuration health, user grants, thematic navigation, jobs,
  webhooks, and data-preserving disable behavior are represented. Direct disabled Marketplace,
  DATEV, payment, and shipping calls are tested.
- Issued invoices are immutable. Full correction invoices have separate numbering, source
  reference, reversed Decimal lines/taxes/totals, immutable history/PDF, one-per-source and request
  idempotency, and no inventory side effect.
- Migration `0014_accounting_export_model.sql` provides immutable accounting entries and stored
  export batches. The deterministic EXTF-v13 renderer stays behind the disabled `export.datev`
  external-validation gate.
- `marketplace.amazon_intelligence` is disabled by default and Amazon-read-only. LWA/Reports
  `v2021-06-30`, fixture and local fake transports, persistent/restart-safe jobs, exact raw archive,
  versioned Sales & Traffic JSON v2 and Inventory Planning TSV parsers, compatible snapshots,
  deterministic analysis, scheduler, UI, and PII-minimized export are implemented. Returns and
  Settlement V2 remain raw-only.
- Provider-neutral payment/shipping ports, complete synthetic providers, signed callback replay
  protection, reconciliation/status/idempotency contracts, and module-aware test payment/manual
  shipping are implemented. Stripe Payment Intents and DHL Parcel Germany are candidates only;
  real adapters remain externally gated.
- Coordinated backup, checksum verification, empty-project restore, and v10-to-v14 upgrade
  rehearsal cover both databases, Core documents, Vendure assets, module configuration without
  secrets, integration state, and Marketplace data.

## Verified locally on 2026-08-13

- Rust: formatting, offline Clippy with `-D warnings`, SQLx prepare/check, offline build, and 55
  tests pass against disposable PostgreSQL 16.
- Frontend: build and lint pass; three pre-existing Fast Refresh warnings remain non-failing.
- Commerce: lint/typecheck, 10 server tests, 2 Storefront tests, Vendure Dashboard/server build,
  and Next.js build pass.
- Upgrade rehearsal: synthetic schema v10 data migrates losslessly through migration 14.
- Recovery Compose matrix: passed twice consecutively with different synthetic IDs, including
  service/DB/full-stack restarts, active/expired leases, failpoints, backoff/dead/requeue, stale
  events, exactly one order/stock/payment, no invoice, and HMAC rotation/replay checks.
- Backup/restore rehearsal: six checksums, both databases and both document stores verified; the
  full SKU-to-fulfillment flow passed before and after restore.
- `npm audit --omit=dev` still reports 12 transitive production findings (6 moderate, 6 high). Its
  proposed force fix downgrades Vendure incompatibly and was not applied.

See `docs/VERIFICATION_MATRIX.md` for evidence layers and limits.

## External gates and known risks

- No real Amazon seller/role/marketplace/RDT request was made. Marketplace availability and roles
  must be verified per selected seller and marketplace before enabling live mode.
- No Stripe or DHL sandbox/account contract was configured; only local ports and fake providers
  are verified. DPD is cataloged as a separate disabled connector, not implemented as a live
  adapter.
- No DATEV checking-program or test-client import was performed, so `export.datev` remains disabled
  and no compatibility claim is made.
- No production-sized backup, external encrypted retention, RPO/RTO, live upgrade, or production
  verification was performed.
- Vendure is pinned consistently at 3.7.2. Current transitive npm advisories require upstream or a
  separately reviewed compatible remediation.

## Next three steps

1. Run the documented Amazon staging gate for one approved non-restricted report and record
   marketplace/role/rate-limit evidence without credentials or buyer PII.
2. Complete Stripe and DHL onboarding, implement real adapters behind the tested ports, and pass
   their official sandbox webhook/reconciliation contracts.
3. Validate EXTF output with the DATEV checking program and an approved empty test client before
   enabling `export.datev` outside development.

## Authoritative files

- `README.md`, `.agent/ARCHITECTURE.md`, `.agent/DECISIONS.md`, `.agent/TODO.md`
- `docs/FAILURE_MATRIX.md`, `docs/VERIFICATION_MATRIX.md`, `docs/OPERATIONS.md`, `docs/API.md`
- migrations `0010` through `0014`
- `commerce/test/recovery.mjs`, `commerce/test/vertical.mjs`, and `ops/`

## Last handoff

2026-08-13: implementation and local synthetic verification complete. Publication details and
remote CI state are recorded in Git history and the branch pull request rather than duplicated
here.
