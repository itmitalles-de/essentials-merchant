# Shop Suite

Shop Suite is a compact commerce and ERP core for German small businesses. The existing Rust
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
| Shop Suite Core (`backend`, `frontend`) | SKU, ERP data, available stock, imported orders, invoices, accounting | Existing `erplite` PostgreSQL database |
| Vendure 3.7.2 server and worker (`commerce/server`) | Products exposed to the shop, categories/facets, cart, checkout, test payment, Shop/Admin APIs | Separate `vendure` PostgreSQL database |
| Next.js storefront (`commerce/storefront`) | German example shop and test checkout | No database; Vendure Shop API only |

The existing `erplite` database, volume, Rust crate, and migration names are intentionally
unchanged. Renaming them would be a separate data migration and would break existing deployments.

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

- Shop Suite administration: `http://localhost:8090`
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

## Next three steps

1. Extend the CI vertical test with deliberate Core/Vendure outages and worker restarts, and assert
   recovery from each persisted processing lease.
2. Replace the dummy payment and manual fulfillment handler with one production payment provider
   and one shipping provider, including signed webhooks and reconciliation.
3. Implement cancellation/correction invoices and only then build and reference-test DATEV EXTF
   from immutable accounting entries.
