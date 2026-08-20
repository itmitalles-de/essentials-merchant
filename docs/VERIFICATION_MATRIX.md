# Essentials+ Merchant verification matrix

Status recorded on 2026-08-19 on branch `pilot/merchant-amazon-read-only`. Every local login,
connection, seller, marketplace report, order, payment, shipment, archive, and restore fixture used
for this evidence was synthetic. No repository test accessed an Amazon seller, provider account,
real `.env`, production database, or buyer dataset.

## Evidence by execution level

| Capability | Unit/static | Integrated / local Compose | Synthetic provider | Amazon staging | Real seller / production | Remaining risk |
| --- | --- | --- | --- | --- | --- | --- |
| Rust Core/domain/PDF | fmt; Clippy `-D warnings`; 60 tests | PostgreSQL 16 migrations 1–15; SQLx prepare/check | fixture data | no | no | Deployment sizing and production data remain untested |
| Amazon transport boundary | Exact five-operation enum/allowlist and repository ownership scan pass | Fake LWA, Reports polling, 429/retry, gzip and document-download contract | yes | blocked | no | Real roles, throttling, availability, response variation and marketplace participation remain external |
| Pilot module boundary | Profile/idempotence/future-required-module tests pass | Exact seven active modules, zero schedules and only `db`, `backend`, `frontend` running | yes | no | no | Operator environment and encrypted-storage attestation remain external |
| Server read-only boundary | Route-policy test covers Core, Commerce, payment, shipping, scheduler, connector health, DATEV and raw archive | Browser probes eight disallowed routes and receives HTTP 409 | yes | no | no | Only the documented manual report acquisition and deterministic analysis POSTs are permitted |
| React Admin / accessibility | TypeScript/Vite build and lint pass | Chromium flow covers login, banner, module state, diagnostics, report, polling, snapshot, analysis and export; axe has no serious/critical finding | fixture SP-API | no | no | Three retained non-failing Fast Refresh lint warnings; no manual production UX claim |
| Raw evidence and analysis | Parser/archive/hash/comparison/PII tests pass | Immutable raw archive, versioned snapshot and deterministic analysis exercised | Sales & Traffic fixture | no | no | Real schema drift and Amazon data semantics are unverified |
| Retained Vendure/Storefront | Typecheck, 10 server tests and 2 Storefront tests | Vendure server/Dashboard and Next.js builds pass; retained vertical/recovery flows pass | synthetic payment/manual shipping | n/a | no | 12 open production-package findings through Vendure 3.7.2; these services are absent from the pilot |
| Core↔Vendure recovery | Lease/retry/requeue/HMAC/idempotence tests | Clean vertical followed by restart/failure matrix passes | synthetic orders/payment/shipment | n/a | no | Production TLS/network/clock/load behavior remains untested |
| Full-stack backup/restore | Manifest/checksum/redaction checks | Empty-project restore compares both databases and both document stores; vertical flow passes before and after | synthetic full stack | n/a | no | External encryption, retention and measured production RPO/RTO unproven |
| Amazon pilot backup/restore | Exact allowlisted manifest and secret-exclusion checks | Empty-project restore verifies a >2 MB raw archive, hashes, snapshots, parser, analysis, modules, audit and documents | synthetic report | no | no | Approved production-like storage acceptance remains open |
| Upgrade rehearsal | Migration assertions | Synthetic v10 invoice/report data migrates losslessly through migration 15 | synthetic | n/a | no | No production or Vendure major-version upgrade performed |
| Dependency and supply chain | Secret, dependency, SHA/tag and SBOM gates pass | Digest-pinned pilot images build | n/a | n/a | no | Vendure findings remain open; cargo-audit/Syft/Trivy were unavailable locally |

## Exact current-branch evidence

- Backend: `cargo fmt --all -- --check`; SQLx-offline Clippy for all workspace targets with warnings
  denied; migrations 1–15 and `cargo sqlx prepare --workspace --check` against disposable
  PostgreSQL 16; 14 DB + 13 domain + 8 PDF + 25 server tests = **60 passed**.
- Frontend: clean npm install, production build and lint pass. The one Playwright Chromium pilot
  flow passes from an empty pilot database, including the fixture report lifecycle, aggregate
  export, eight HTTP 409 boundary probes and zero serious/critical axe violations.
- Pilot Compose: configuration check and fresh start pass with only `db`, `backend`, and `frontend`;
  persisted status reports the exact seven-module allowlist and zero automatic schedules. The
  disposable project and its volumes were removed after the run.
- Retained Commerce: clean npm install, TypeScript checks, 10 Vendure/provider tests, 2 Storefront
  tests, Vendure server/Dashboard build and Next.js build pass. Commerce services are not part of
  the pilot Compose graph.
- Retained recovery: the clean vertical flow and the restart/failure recovery matrix pass with
  restart-safe leases, exactly-once import, stale-event protection, dead-letter audit, HMAC
  rotation and persisted recovery.
- Recovery storage: the general backup/empty-project restore passes for both databases, both file
  stores, manifest hashes and the vertical flow before/after restore. The Amazon-only recovery
  passes for a synthetic raw archive over 2 MB plus report inventory, transport/decoded hashes,
  snapshot, parser, analysis, exact modules, audit, documents and disabled schedules.
- Upgrade: migration-10 synthetic invoice/report data reaches migration 15 losslessly.
- Security: Amazon operation contract, repository secret scan (244 files), dependency gate and
  both lockfile-derived CycloneDX inventories pass. Frontend production audit is 0. Retained
  Commerce audit is intentionally non-green: 12 affected package nodes (6 moderate, 6 high,
  0 critical), representing 11 individually triaged GHSAs. No forced fix was applied.
- Hygiene: shell/Node syntax checks, workflow YAML parse and `git diff --check` pass. GitHub Actions
  and release-download pins are reviewed separately from application dependency findings.

## Interpretation and external boundary

“Synthetic” proves only repository-owned behavior. “Local Compose” proves process, database and
browser integration on this workstation. Neither is evidence of Amazon authentication, seller
authorization, real rate limits, provider uptime, production encryption, legal/tax acceptance or
operational RPO/RTO.

Amazon staging is **blocked** because no approved seller, SP-API credential reference, Brand
Analytics role, confirmed marketplace, or encrypted archive attestation was available. No real
request was sent. The first safe external action remains the separately reviewed one-shot gate in
[`operations/AMAZON_STAGING_GATE.md`](operations/AMAZON_STAGING_GATE.md); real-seller production
operation is outside this milestone.
