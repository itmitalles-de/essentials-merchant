# Merchant · Essentials Plus

Merchant is an Essentials Plus commerce and ERP core for German small businesses. The existing Rust
application remains the system of record for SKU, ERP master data, available stock, imported
orders, invoices, and accounting. Vendure is a deliberately separate commerce subsystem for
merchandising, cart, checkout, promotions, payments, and the Shop API.

Status: active development. Authentication, company settings, customers, VAT, invoices and PDF
generation, articles and inventory, sales orders, and the first Vendure commerce adapter are
implemented. DATEV EXTF, correction invoices, production payment/shipping providers, labels,
marketplaces, and B2B price lists are not implemented yet.

## Architecture

| Component | Responsibility | Database |
| --- | --- | --- |
| Merchant Core (`backend`, `frontend`) | SKU, ERP data, available stock, imported orders, invoices, accounting | Existing `erplite` PostgreSQL database |
| Vendure 3.7.2 server and worker (`commerce/server`) | Products exposed to the shop, categories/facets, cart, checkout, test payment, Shop/Admin APIs | Separate `vendure` PostgreSQL database |
| Next.js storefront (`commerce/storefront`) | German example shop and test checkout | No database; Vendure Shop API only |

The existing `erplite` database, volume, Rust crate, and migration names are intentionally
unchanged. Renaming them would be a separate data migration and would break existing deployments.

## Essentials Plus Admin-Center and modules

The React administration is the Essentials Plus Admin-Center. Its catalog is grouped by product
area. Administrators can see every module; normal users receive only explicitly granted, enabled
modules. Disabling a module removes its navigation and stops its jobs and webhooks without deleting
historical data.

- `core_operations` is the enabled Merchant Core module.
- `marketplace_intelligence` is an optional, read-only Amazon Reports module.
- `connector_dhl` and `connector_dpd` are separate connector catalog entries. Their configuration
  check reports whether the required secret reference exists; it deliberately does not create a
  shipping order or perform a carrier-side write.

### Marketplace Intelligence

Marketplace Intelligence uses Amazon Selling Partner API Reports API `v2021-06-30` with Login
with Amazon OAuth. The live client obtains an access token from a refresh token through LWA; it
does not implement obsolete IAM/SigV4 signing. A connection stores only a logical `secret_ref`,
seller ID, region, granted roles, and marketplace IDs. The backing secret is read at runtime from
the environment variable derived as `AMAZON_SECRET_<SECRET_REF>` and must be JSON with
`refresh_token`, `client_id`, and `client_secret`. No token or secret is persisted, logged, or sent
to the frontend.

The persistent job flow is shared by manual and scheduled runs: request, poll with exponential
backoff, document URL lookup/download/decompression, immutable raw archive with SHA-256, parsing,
normalized snapshot, then deterministic analysis. A run is idempotent while it is in flight, has
lease-reclaim recovery, and stores its complete status history. HTTP 429 and expired pre-signed URLs
are retried; `CANCELLED` and `FATAL` are terminal. The MVP only calls Reports endpoints and is
read-only towards Amazon.

The registry currently includes:

| Report type | Format | Role | MVP handling |
| --- | --- | --- | --- |
| `GET_SALES_AND_TRAFFIC_REPORT` | JSON | Brand Analytics | Sales, units, traffic, conversion; requestable and schedulable |
| `GET_FBA_INVENTORY_PLANNING_DATA` | TSV | Amazon Fulfillment | Inventory, 30-day shipments, stock cover; requestable |
| `GET_FBA_FULFILLMENT_CUSTOMER_RETURNS_DATA` | TSV | Pricing or Amazon Fulfillment | Raw archive only; possible PII fields are not analysed or sent to AI |
| `GET_V2_SETTLEMENT_REPORT_DATA_FLAT_FILE_V2` | TSV | Finance and Accounting | Raw archive only; replaces old settlement types scheduled for removal |

The parser accepts unknown fields, reports missing required fields, uses `Decimal` for money and
metrics, and persists parser version and import failures. The synthetic fixture client provides a
Sales & Traffic JSON report and an Inventory Planning TSV report, including an unknown TSV column,
so the UI can be demonstrated without Amazon credentials.

Deterministic analysis always runs first. It compares only identical report type, granularity, and
period length; incompatible periods remain explicitly unanalysed. An optional OpenAI-compatible
provider can be enabled with `AI_PROVIDER_ENDPOINT`, `AI_PROVIDER_MODEL`, and optionally
`AI_PROVIDER_API_KEY`. It receives only an allowlist of aggregated metrics. Invalid or failed
provider responses cannot overwrite the deterministic result. Every result keeps the strategy,
model (if any), prompt version, payload hash, and timestamp.

The adapter uses two durable outboxes rather than pretending to provide a distributed
transaction:

1. A Core article insert/update or stock movement writes a `vendure.product.project` event in the
   same PostgreSQL transaction. The Vendure worker applies SKU, net price, VAT category, and
   available stock, then records the Core UUID ↔ Vendure ID mapping.
2. A Vendure Authorized/Settled payment event is written to Vendure's own outbox. The worker sends
   it to the Core with a stable event key. Core inbox and external-order uniqueness make duplicate
   and late payment events safe and book stock once.
3. Fulfilling an imported order in Core writes a `vendure.fulfillment.project` event in the same
   transaction. The worker creates a Vendure fulfillment and moves it through `Pending` to
   `Shipped` with carrier and tracking number.

Claims use row locks, a five-minute processing lease, exponential retry (up to one hour), and a
dead state after 20 attempts. Product projections carry a monotonic sequence, so a delayed event
cannot overwrite a newer price or stock value. This is at-least-once delivery with idempotent
consumers, not globally exactly-once messaging.

## Docker Compose

Copy the example environment and fill every empty secret or credential. In particular, use
independently generated values for the Core JWT secret, integration secret, Vendure cookie secret,
database password, and Superadmin credentials. The repository has no working default Superadmin
login.

```bash
cp .env.example .env
docker network inspect proxy_net >/dev/null 2>&1 || docker network create proxy_net
docker compose up -d --build
docker compose ps
```

Default local endpoints:

- Merchant administration: `http://localhost:8090`
- Vendure Shop API: `http://localhost:3000/shop-api`
- Vendure Dashboard: `http://localhost:3000/dashboard`
- Storefront: `http://localhost:3001`

The first Vendure server start runs committed TypeORM migrations and populates only infrastructure
data: Germany, 19/7/0 percent tax rates, standard shipping, and an automatically settled dummy
payment named `Testzahlung`. Products still originate exclusively in Core and are projected by the
worker. The dummy payment must never be used as a production payment method.

Backup and restore the Core and Vendure databases independently. Keep their matching application
versions and the `vendure_assets` volume with a backup. Run upgrades in a staging copy first;
never point migration generation or SQLx preparation at production.

## Local checks

### Core

```bash
cd backend
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true cargo test
```

For `cargo test`, `DATABASE_URL` must point to a disposable PostgreSQL database whose user can
create temporary databases; `#[sqlx::test]` creates and migrates an isolated database per
integration test. Offline mode controls compile-time query metadata, not those runtime tests.

### Administration frontend and commerce

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

### Reproducible vertical test

Start the complete Compose stack, then pass the local Core administrator credentials only through
the process environment:

```bash
cd commerce
CORE_API_URL=http://localhost:8090/api \
STOREFRONT_API_URL=http://localhost:3001/api/shop \
CORE_ADMIN_USERNAME="$ADMIN_USERNAME" \
CORE_ADMIN_PASSWORD="$ADMIN_PASSWORD" \
npm run test:vertical
```

The test creates a unique Core SKU, receives ten units, waits for projection to the Storefront,
checks out two units with Vendure's test payment, proves one Core order and one sales stock
movement, and verifies the returned `Shipped` tracking status. Test data is deliberately retained
for diagnosis; use only a disposable installation or staging copy.

## SQLx offline cache

CI and the backend image compile against the committed `backend/.sqlx` cache, so compilation does
not depend on an already migrated CI database. After every Core migration or checked-query change,
refresh and commit the cache against a disposable, fully migrated database:

```bash
cd backend
cargo install sqlx-cli --no-default-features --features rustls,postgres
cargo sqlx migrate run --source crates/db/migrations
cargo sqlx prepare --workspace -- --all-targets
cargo sqlx prepare --workspace --check -- --all-targets
```

`DATABASE_URL` must point to that disposable database. The explicit migration path matters because
the migrations belong to the `db` crate, not the Cargo workspace root.

## Vendure schema changes

Vendure migrations live in `commerce/server/src/migrations`. With a disposable Vendure database
and all required environment variables set:

```bash
cd commerce/server
npx vendure migrate --generate DescriptiveName \
  --output-dir "$PWD/src/migrations" \
  --config "$PWD/src/vendure-config.ts"
npx vendure migrate --run --config "$PWD/src/vendure-config.ts"
```

Review generated SQL, run it against a copy of existing data, then commit it. Runtime uses
`synchronize: false`; schema changes must be explicit migrations.

## Known limits and risks

- The Core migration adds columns and integration tables without renaming or deleting existing
  data. It also enqueues a projection for every existing article; plan worker capacity before a
  large upgrade. Rollback after live commerce traffic requires retaining mappings, inbox/outbox,
  and imported order snapshots.
- The adapter supports one default Vendure channel and integer saleable stock. Fractional Core
  quantities are rounded down for Vendure availability.
- Integration authorization is a shared secret over HTTP headers. Use TLS and private service
  networking outside local Compose; secret rotation is currently coordinated rather than dual-key.
- Vendure 3.7.2 currently brings upstream production dependency advisories reported by
  `npm audit` (Apollo Server, file/image parsing, lodash, sharp, uuid, and ws). npm's proposed
  automatic fixes downgrade Vendure and are not safe. Asset uploads should remain restricted and
  the pinned Vendure patch line should be updated as soon as an upstream compatible release fixes
  them.
- There are no claims of legal or DATEV compatibility. DATEV EXTF remains unimplemented and must
  be validated against an authoritative format reference before production use.
- Marketplace Intelligence has no live Amazon-account acceptance in this repository. Configure
  approved LWA credentials and perform a manual seller/role/marketplace gate in a staging account
  before relying on the live transport. Never use restricted report types unless their selected
  registry entry explicitly requires an RDT; the current MVP registry does not request RDTs.

## Next three steps

1. Extend the CI vertical test with deliberate Core/Vendure outages and worker restarts, and assert
   recovery from each persisted processing lease.
2. Replace the dummy payment and manual fulfillment handler with one production payment provider
   and one shipping provider, including signed webhooks and reconciliation.
3. Implement cancellation/correction invoices and only then build and reference-test DATEV EXTF
   from immutable accounting entries.
