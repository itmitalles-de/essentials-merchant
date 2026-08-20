# Current State

## Product and branch

- Visible product and repository: **Essentials+ Merchant**, `itmitalles-de/essentials-merchant`.
- Active work: `pilot/merchant-amazon-read-only`, based on `main` commit
  `f4ad7813512e7e845418579343c9cca395e81156`.
- Historical `erplite`, crate, database, volume, migration, token-storage, mapping, and
  `shop-suite-*` values remain deliberate compatibility contracts.
- The latest pre-branch CI run checked during discovery, `31711079422`, completed successfully.
- This is an internal pilot, not production, Amazon-account, legal, tax, DATEV, payment, or carrier
  certification.

## Active milestone

The only active external integration is read-only Amazon Marketplace Intelligence. The reproducible
`amazon-read-only` module profile enables exactly the required Core modules,
`marketplace.amazon_intelligence`, `intelligence.rules`, and `pilot.amazon_read_only`. It disables
Vendure Commerce, Storefront, every payment/shipping module, DATEV, custom mutations, and all Amazon
schedules. `compose.amazon-pilot.yml` starts only database, backend, and admin frontend.

The backend additionally enforces a global fail-closed mutation policy. Core, Commerce,
integration, payment, shipping, fulfillment, scheduler, and DATEV writes return HTTP 409 while the
pilot is active. The exact Amazon transport allowlist is LWA refresh, `createReport`, `getReport`,
`getReportDocument`, and validated presigned document download; method/path cannot be supplied by a
caller. No Amazon business-mutation client or external LLM provider exists.

The admin UI identifies the pilot as read-only and shows exact module state, redacted Amazon
connection/report/transport/archive/parser/snapshot diagnostics, missing data, deterministic
analysis, and backup verification. It excludes credentials, buyer data, and raw payloads.

## Retained but outside the pilot

The existing Core↔Vendure vertical, Storefront, immutable invoices/accounting/DATEV renderer,
provider-neutral payment/shipping ports, fakes, recovery tests, and compatibility data remain in the
repository. They are not deleted or started by the pilot. Stripe/webhooks, DHL, DPD, carrier
labels, DATEV activation, other marketplaces, external AI, multi-tenancy, and Kubernetes are
frozen until a later explicitly approved milestone after Amazon success.

## Verification status — 2026-08-19

- Current branch passed Rust fmt, SQLx-offline Clippy with warnings denied, migrations 1–15,
  SQLx prepare/check and 60 tests against disposable PostgreSQL 16. Frontend clean install,
  build/lint and the Chromium/axe pilot flow pass from an empty three-service pilot project.
- The exact local Pilot Compose graph (`db`, `backend`, `frontend`), seven-module persisted profile,
  zero schedules, eight HTTP 409 mutation/archive/connector-health probes, operation allowlist, secret scan,
  dependency gate, syntax checks and workflow parsing pass.
- Retained Commerce clean install/lint, 10 server tests, 2 Storefront tests, Vendure/Dashboard/Next
  builds, clean vertical, recovery matrix, general backup/empty restore, pilot backup/empty restore
  with a >2 MB raw archive, and the migration-10-to-15 upgrade rehearsal pass.
- Current audits: frontend production dependencies 0 findings; retained Commerce 12 production
  package findings (six high, six moderate, zero critical), representing 11 distinct GHSAs through
  Vendure 3.7.2. They are triaged individually and remain open; the incompatible npm force-fix was
  not applied.
- Pilot and retained-Commerce CycloneDX SBOMs plus a redacted dependency report are present.
  `cargo-audit`, Syft and Trivy were unavailable locally, so no Rust/container advisory-free claim
  is made. Exact evidence and its limits are in `docs/VERIFICATION_MATRIX.md`.

## External gates

- **BLOCKED:** no approved Amazon SP-API credential, seller hash, Brand Analytics role, confirmed
  marketplace participation, or encrypted raw-archive attestation was supplied. No real request
  was made and no fixture/local result is described as live.
- The first permitted real request is one manual `GET_SALES_AND_TRAFFIC_REPORT` for one confirmed
  marketplace and a completed one-to-seven-day period, after validation of the ignored approval
  and secret files. No scheduler and no RDT are allowed.
- A second snapshot is allowed only after the first real request succeeds, using the same report,
  marketplace dimension, granularity, period length, and parser version.
- Production-sized storage, external encryption/retention, measured RPO/RTO, and real provider
  behavior remain unverified.

## Authoritative files

- `README.md`, `.agent/ARCHITECTURE.md`, `.agent/DECISIONS.md`, `.agent/TODO.md`
- `docs/PILOT_SCOPE.md`, `docs/COMPATIBILITY_IDENTIFIERS.md`,
  `docs/operations/AMAZON_STAGING_GATE.md`, `docs/DEFERRED_EXTERNAL_GATES.md`
- `docs/FAILURE_MATRIX.md`, `docs/VERIFICATION_MATRIX.md`, `docs/OPERATIONS.md`, `docs/API.md`
- `docs/security/VENDURE_ADVISORIES.md`, `docs/security/dependency-audit-2026-08-19.json`, SBOMs
- migration `0015_amazon_read_only_pilot.sql`, `compose.amazon-pilot.yml`, `scripts/`, and `ops/`
