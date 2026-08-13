# Essentials+ Merchant

Essentials+ Merchant is a compact ERP and commerce system for German small businesses. The Rust
Core remains authoritative for SKUs, ERP master data, available stock, imported orders, invoices,
and immutable accounting entries. Vendure is a separate commerce subsystem for merchandising,
cart, checkout, promotions, payments, and Shop/Admin APIs.

Status: active development. The repository has automated synthetic coverage for the Core↔Vendure
vertical flow, restart and failure recovery, correction invoices, deterministic read-only Amazon
report intelligence, and backup/restore. It is not a claim of production readiness, legal or tax
compliance, DATEV import compatibility, or live provider verification.

The repository slug and existing internal names remain `erplite`. Crates, migrations, PostgreSQL
databases, Docker volumes, token storage, and mapping tables are compatibility contracts and are
not presentation branding.

## Architecture

| Component | Responsibility | Persistence |
| --- | --- | --- |
| Rust/Axum Core and React admin (`backend`, `frontend`) | ERP, inventory, imported orders, immutable invoices/accounting, modules, diagnostics, Marketplace Intelligence | `erplite` PostgreSQL and `erplite_invoices` |
| Vendure 3.7.2 server and worker (`commerce/server`) | Commerce catalog projection, cart, checkout, synthetic test payment, Shop/Admin APIs | Separate `vendure` PostgreSQL and `vendure_assets` |
| Next.js Storefront (`commerce/storefront`) | German synthetic storefront | Vendure Shop API only |

Core and Vendure never share a database or distributed transaction. Durable outboxes, an inbox,
stable idempotency keys, leases, retries, and monotonic product sequences provide at-least-once
delivery with replay-safe consumers. See [.agent/ARCHITECTURE.md](.agent/ARCHITECTURE.md).

## Essentials+ module contract

The React administration is the thematically grouped Essentials+ Merchant Admin-Center.
Administrators see the complete catalog; normal users see only enabled modules for which they have
an explicit grant. Canonical module states are `not_installed`, `needs_configuration`, `disabled`,
`enabled`, and `degraded`.

Core modules are required. Optional modules and connectors declare version, group, type,
dependencies, conflicts, compatibility, configuration and secret requirements, API/navigation
boundaries, jobs, webhooks, healthcheck, data ownership, and backup/restore behavior. State changes
are transactional, idempotent, and audited. Disabling a module blocks its API and navigation and
stops its worker claims, jobs, or webhooks without deleting configuration, mappings, reports, or
history.

Implemented catalog IDs include:

- `core.catalog`, `core.inventory`, `core.orders`
- `commerce.vendure`, `commerce.storefront`
- `payment.test`, `shipping.manual`, plus separately disabled `shipping.dhl` and `shipping.dpd`
- `accounting.invoices`, `accounting.corrections`, `export.datev`
- `marketplace.amazon_intelligence`, `intelligence.rules`, and `custom.catalog`

Connector activation requires server-side configuration health. The synthetic payment and manual
shipping modules are for local tests only. DHL and DPD are separate catalog modules and never part
of Marketplace Intelligence.

## Integration reliability and diagnostics

Core↔Vendure service requests use HMAC-SHA-256 over method, path, timestamp, nonce, and body hash.
Core persists used nonces, rejects replayed or expired requests, accepts a current and an optional
previous key during coordinated rotation, and limits adapter request bodies to 256 KiB. Tokens and
payloads are absent from diagnostics.

The administrator-only integration view shows Core and Vendure queue counts, oldest open event,
last success, sanitized error, lease state, mappings, health/readiness, and an audit trail. Manual
requeue accepts only dead events, requires an idempotency key, and never exposes full payloads.

Production defaults remain a five-minute lease, exponential retries capped at one hour, and dead
state after 20 attempts. Automated recovery tests override these values only in `APP_ENV=test`.
The tested outage and restart cases are listed in [docs/FAILURE_MATRIX.md](docs/FAILURE_MATRIX.md).

## Correction invoices and accounting export

An issued invoice is an immutable snapshot. A full correction:

- receives a unique `KR-YYYY-NNNN` number and an explicit reference to the original;
- copies lines, tax amounts, customer/company snapshots, and totals with reversed decimal signs;
- is idempotent and limited to one full correction per original invoice;
- creates no stock movement and never mutates the original;
- renders a PDF with the correction reference and records immutable audit history.

Migration `0014_accounting_export_model.sql` derives immutable accounting entries from issued
invoices and corrections. The guarded `POST /api/exports/datev` endpoint creates deterministic,
byte-identical UTF-8-with-BOM EXTF Buchungsstapel files with CRLF, semicolon fields, header 700,
format version 13, decimal commas, explicit S/H signs, account/tax mappings, and correction
references. Export batches, parameters, entry IDs, hashes, and creator are immutable.

The renderer follows the public DATEV Developer Portal format description and local tests, but its
output has not been accepted by the DATEV checking program or imported into a real DATEV sandbox.
`export.datev` therefore stays disabled and carries an explicit external-validation gate. No legal,
tax, or DATEV-compatibility claim is made.

Authoritative format references: [DATEV format structure](https://developer.datev.de/de/file-format/details/datev-format/getting-started),
[booking batch fields](https://developer.datev.de/de/file-format/details/datev-format/format-description/booking-batch),
[character sets](https://developer.datev.de/de/file-format/details/datev-format/character-set), and
[DATEV validation tools](https://developer.datev.de/de/file-format/details/datev-format/tools).

## Marketplace Intelligence

`marketplace.amazon_intelligence` is disabled by default and read-only toward Amazon. The live
transport uses Reports API `v2021-06-30` with Login with Amazon OAuth. AWS IAM/SigV4 is not
implemented. A connection persists seller ID, region (`na`, `eu`, `fe`), marketplace IDs, granted
roles, mode, and a logical secret reference only. Refresh token, client secret, and access token
never enter the database, logs, or frontend. The current report registry requests no RDT.

Manual and scheduled retrieval use the same persistent job path: `createReport`, exponential
`getReport` polling, terminal `DONE`/`CANCELLED`/`FATAL`, immediate `getReportDocument`, pre-signed
download, optional GZIP decompression, transport and decoded SHA-256, immutable raw archive,
versioned parsing, normalized metrics, compatible snapshot, and deterministic analysis. Claims,
leases, retries, expired URLs, partial downloads, and uniqueness constraints make the flow safe
across duplicate triggers and worker restarts.

| Report type | Official shape and role | Essentials+ Merchant handling |
| --- | --- | --- |
| `GET_SALES_AND_TRAFFIC_REPORT` | JSON; Brand Analytics; requestable/schedulable | JSON parser v2, explicit date/ASIN granularity, sales/units/traffic/conversion, analysis |
| `GET_FBA_INVENTORY_PLANNING_DATA` | Tab-delimited; Amazon Fulfillment; request-only | TSV parser v1, inventory, 30-day shipments, stock cover, analysis; marketplace availability must be checked |
| `GET_FBA_FULFILLMENT_CUSTOMER_RETURNS_DATA` | Tab-delimited; Pricing or Amazon Fulfillment; request-only | Immutable raw archive only because order IDs/comments can contain sensitive data |
| `GET_V2_SETTLEMENT_REPORT_DATA_FLAT_FILE_V2` | Tab-delimited settlement report; Finance and Accounting | Immutable raw archive only; no deprecated predecessor is implemented |

The parsers tolerate unknown fields and column order, reject duplicate dimensions and malformed
encoding, report missing required fields, and use `Decimal` rather than binary floats. Snapshot
comparison requires identical report type, parser version, marketplace, date/ASIN granularity, and
period length. Incompatible snapshots are reported as missing comparison data.

Analysis is deterministic and rule-based. Results contain facts, delta, overall trend, anomalies,
hypotheses, two-to-five possible actions, expected impact, effort, risk, evidence references,
uncertainty, and missing data. Exports are allowlist-based aggregates and strip buyer/customer,
address, email, order-ID, comment, and phone fields. No external LLM provider is implemented, and
the software never changes Amazon prices, ads, listings, inventory, or orders.

The local fixture client and fake SP-API server use fully synthetic documents and exercise OAuth,
Reports endpoints, polling, rate limits, terminal states, compressed and partial documents,
parser errors, retries, restart safety, raw-only types, and PII filtering. They are not a production
Amazon acceptance.

Authoritative references: [Reports API v2021-06-30](https://developer-docs.amazon.com/sp-api/docs/reports-api),
[official Sales & Traffic schema](https://github.com/amzn/selling-partner-api-models/blob/main/schemas/reports/sellerSalesAndTrafficReport.json),
[FBA report types](https://developer-docs.amazon.com/sp-api/docs/report-type-values-fba),
[LWA authorization](https://developer-docs.amazon.com/sp-api/docs/authorizing-selling-partner-api-applications),
[removal of the SigV4 requirement](https://developer-docs.amazon.com/sp-api/changelog/sp-api-will-no-longer-require-aws-iam-or-aws-signature-version-4),
and [Settlement predecessor removal](https://developer-docs.amazon.com/sp-api/changelog/update-removal-of-xml-settlement-report-and-flat-file-settlement-report-date-changed-to-november-11-2026).

## Payment and shipping provider boundary

Provider-neutral TypeScript ports define payment authorization/capture/failure/refund and shipping
creation/in-transit/delivery/failure states, idempotency, amount/currency/order checks,
reconciliation, audit, timeout, retryable errors, carrier, and tracking. A complete in-memory fake
provider and signed callback verifier cover these contracts without credentials.

Stripe Payment Intents and DHL Parcel Germany are the documented production candidates. No live
adapter is enabled: account-specific contracts, secrets, webhook setup, sandbox credentials, and
provider acceptance are external gates. `payment.test` fails closed when its Core module is disabled
or unavailable; `shipping.manual` is protected at the Core fulfillment API.

Candidate evaluation uses Stripe's official [idempotent request](https://docs.stripe.com/api/idempotent_requests)
and [webhook](https://docs.stripe.com/webhooks) contracts plus DHL's official
[Post & Parcel Germany authentication API](https://developer.dhl.com/api-reference/authentication-api-post-paket-deutschland?lang=de).

## Docker Compose

Create a local environment from `.env.example`; never reuse production credentials or data.

```bash
cp .env.example .env
docker network inspect proxy_net >/dev/null 2>&1 || docker network create proxy_net
docker compose up -d --build --wait
docker compose ps
```

Default endpoints are the Essentials+ Merchant admin on `http://localhost:8090`, Vendure Shop API on
`http://localhost:3000/shop-api`, Vendure Dashboard on `http://localhost:3000/dashboard`, and the
Storefront on `http://localhost:3001`.

The first Vendure start runs committed TypeORM migrations and populates only synthetic
infrastructure data. Products continue to originate in Core. The test payment must not be exposed
as a production method.

## Automated checks

Core checks require a disposable PostgreSQL URL whose user may create temporary databases:

```bash
cd backend
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings
DATABASE_URL=postgres://USER:PASSWORD@HOST/DISPOSABLE_DB SQLX_OFFLINE=true cargo test --workspace
cargo sqlx prepare --workspace --check -- --all-targets
```

Frontend and commerce checks:

```bash
cd frontend
npm ci
npm run build
npm run lint

cd ../commerce
npm ci
npm run lint
npm test
npm run build
```

The clean Compose vertical test is `npm run test:vertical`. The deliberate outage matrix is
`npm run test:recovery` and requires the synthetic test-only variables documented in
[docs/FAILURE_MATRIX.md](docs/FAILURE_MATRIX.md). Both tests retain data for diagnosis and must run
only against a disposable Compose project.

Backup/restore and upgrade rehearsals create and delete isolated, randomly named synthetic stacks:

```bash
ops/test-backup-restore.sh
ops/test-upgrade-rehearsal.sh
```

The verification status of every layer is recorded in
[docs/VERIFICATION_MATRIX.md](docs/VERIFICATION_MATRIX.md).

## Backup and restore

`ops/backup.sh` quiesces both application writers, creates separate logical PostgreSQL dumps,
archives Core documents and Vendure assets, exports module configuration without secrets, and
writes a manifest with SHA-256 checksums, UTC timestamp, Git revision, app/schema versions, every
store, and redacted Compose metadata. The stack is resumed even if the backup fails.

`ops/restore.sh` verifies every checksum and refuses a project that already has containers or any
declared volume. It restores only into a completely empty, explicitly named Compose project. The
rehearsal then reruns the complete SKU-to-fulfillment test. See
[docs/OPERATIONS.md](docs/OPERATIONS.md).

## SQLx and Vendure migrations

Core migrations live in `backend/crates/db/migrations`; Vendure migrations live in
`commerce/server/src/migrations`. Runtime schema synchronization is disabled. Generate migrations
and SQLx offline metadata only against disposable databases, review the result, and run
`ops/test-upgrade-rehearsal.sh` before deployment. Never generate or test migrations against
production.

## Current external risks

- Vendure packages are already pinned together at 3.7.2, the current 3.7.x patch verified on
  2026-08-13. The [official 3.7.2 release](https://github.com/vendurehq/vendure/releases/tag/v3.7.2)
  fixes four Vendure advisories and needs no migration. `npm audit
  --omit=dev` still reports 12 transitive production advisories (six high, six moderate); its
  proposed automatic remedy is an incompatible downgrade, so no forced audit fix is applied.
- No Amazon seller/role/marketplace/RDT acceptance, Stripe sandbox, DHL sandbox, DATEV checker, or
  production integration was used. Local fakes prove contracts and recovery, not provider behavior.
- One default Vendure channel and integer saleable stock are supported. Fractional Core quantities
  are rounded down for shop availability.
- HMAC protects message authenticity/replay, but production traffic still requires TLS and private
  service networking.
- Shipping labels, multi-warehouse, B2B channels, other providers/marketplaces, external AI,
  automation, multi-tenancy, and Kubernetes are deliberately deferred in
  [docs/NICE_TO_HAVE.md](docs/NICE_TO_HAVE.md).

## Next three steps

1. Run the documented Amazon staging gate with an approved synthetic-safe seller context and one
   non-restricted report, then record marketplace/role/rate-limit evidence without credentials.
2. Complete Stripe and DHL account onboarding, implement the real adapters behind the tested
   ports, and run their official sandbox and webhook/reconciliation acceptance suites.
3. Validate generated EXTF fixtures with the DATEV checking program and an approved empty test
   client before enabling `export.datev` anywhere outside development.
