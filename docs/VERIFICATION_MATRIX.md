# Essentials+ Merchant verification matrix

Status recorded on 2026-08-13 on branch `agent/essentials-merchant-autonomous`. Every fixture,
credential, product, customer, order, report, payment, and shipment used here was synthetic. No
real `.env`, shop, provider, Amazon seller, carrier, payment account, or production database was
accessed.

| Capability | Local unit/static | Integrated | Complete Compose | Fake provider | Real sandbox | Production | Remaining risk |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Rust Core/domain/PDF | 55 tests; fmt; Clippy `-D warnings` | SQLx tests on PostgreSQL 16 and offline prepare/check | Built and healthchecked | n/a | no | no | Deployment sizing and production data remain untested |
| React Admin-Center | TypeScript/Vite build and lint pass | Module/API types compile | Nginx image healthy in all rehearsals | synthetic API data | no | no | Three pre-existing non-failing Fast Refresh warnings; no manual UI claim |
| Vendure/Storefront | Typecheck, 10 server + 2 Storefront tests | Server, Dashboard, Next.js builds | SKU-to-fulfillment passed before and after restore | synthetic payment/manual shipping | no | no | 12 transitive npm findings; one channel/integer stock only |
| Core↔Vendure recovery | DB lease/retry/requeue tests | HMAC, nonce, old/new key, invalid/replay/expiry tests | Failure matrix passed twice with different IDs | synthetic orders/payment/shipment | no | no | Production TLS/network/clock behavior and sustained load remain untested |
| Module contract | DB visibility/state/config/dependency tests | Disabled direct Marketplace, DATEV, payment, shipping requests rejected | Module-aware workers and navigation exercised | synthetic health | no | no | Real connector configuration probes await adapters |
| Corrections | Decimal, immutability, idempotency, PDF tests | Accounting entry triggers and API compile | Core image/DB migration exercised | synthetic invoices | no | no | No legal/tax conformity claim |
| DATEV export | BOM/CRLF/fields/limits/mappings/determinism tests | Immutable stored export batch test | Migration and backup/restore exercised | synthetic accounting entries | no | no | DATEV checker and test-client import not run; module disabled |
| Marketplace Intelligence | Parser/job/archive/delta/PII tests | Local fake SP-API exercises OAuth, poll, 429, terminal states, GZIP, partial URL/download | Core image, schema and backup stores exercised | yes, fully synthetic | no | no | Seller roles, marketplace availability, live throttling/RDT not accepted |
| Payment/shipping ports | Contract, signature/replay, idempotency, status, retry/reconcile tests | Module-aware synthetic payment and manual fulfillment | Recovery flow verifies one payment and tracking | yes | no | no | Stripe/DHL real adapters and account contracts not implemented |
| Backup/restore | Manifest/checksum/redaction scripts | Two logical dumps plus two file stores compared | Empty-project restore and vertical flow before/after pass | synthetic full stack | no | no | External encryption, retention, production volume, RPO/RTO unproven |
| Upgrade rehearsal | Migration assertions | Synthetic schema v10 → v14 passes losslessly | PostgreSQL container rehearsal | synthetic invoice/report | no | no | No production or future Vendure upgrade performed |

## Exact automated evidence

- `cargo fmt --all --check`
- `SQLX_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings`
- `DATABASE_URL=<disposable> cargo test --workspace`: 12 DB, 13 domain, 8 PDF, 22 server tests
- `cargo sqlx prepare --workspace -- --all-targets` and `--check`; offline all-target check
- frontend `npm run build` and `npm run lint`
- commerce `npm run lint`, `npm test`, and `npm run build`
- `ops/test-upgrade-rehearsal.sh`: migration 10 synthetic data through migration 14
- `npm run test:recovery`: passed twice consecutively after a clean Compose build
- `ops/test-backup-restore.sh`: six checksums, both databases, both file stores, vertical flow twice
- `npm audit --omit=dev`: intentionally non-green, 6 moderate and 6 high transitive findings; no
  forced/incompatible fix applied

## Interpretation

“Fake provider” means only that local behavior matches the repository-owned contract. It is not
evidence of a provider's real authentication, rate limiting, webhook delivery, reconciliation,
data semantics, commercial entitlement, or uptime. “Real sandbox” and “Production” remain `no` for
Amazon, Stripe, DHL, DPD, DATEV, and the complete Merchant stack.
