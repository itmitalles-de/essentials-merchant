# Essentials+ Merchant failure and recovery matrix

This matrix defines the automated reliability acceptance for the existing Core↔Vendure vertical
slice. It uses unique synthetic SKUs, customers, orders, credentials, and tracking numbers in a
disposable Compose project. It does not call a real payment, carrier, Amazon, or shop account.

Production defaults are unchanged. In `APP_ENV=test` only, leases, retry delays, maximum attempts,
and one-shot process failpoints can be shortened. Polling uses 100–200 ms intervals and explicit
deadlines; the test contains no long fixed sleep. Ordinary Compose stop/restart and connection
refusal are deterministic enough for this topology, so Toxiproxy is not required.

## Automated cases

| Failure or ordering case | Injection | Required automated evidence |
| --- | --- | --- |
| Core unavailable before product projection | Stop Core while a committed Core outbox row exists; restart worker then Core | Event remains durable, retry count increases, projection completes before deadline |
| Vendure unavailable before product projection | Stop worker before article/stock commit; later start it | Pending Core row survives and exactly one latest projection is visible |
| Core unavailable during order import | Stop/restart Core and trigger one-shot failures before and after inbox commit | One Core inbox effect, one imported order, one sales stock movement |
| Vendure unavailable during fulfillment update | Stop worker and Vendure DB while Core commits fulfillment | Core event remains pending and one Vendure fulfillment reaches `Shipped` after recovery |
| Vendure worker stopped before claim | Stop worker before creating source event | Event is still pending and processed after start |
| Vendure worker exits after claim | `after_vendure_claim` / `after_core_claim` one-shot failpoints | Processing row and lock time persist; active lease is not immediately reclaimed |
| Vendure worker exits before acknowledge | `before_core_ack` failpoint | Re-delivery is harmless; event is ultimately delivered once |
| Core exits before inbox commit | `before_inbox_commit` failpoint | No partial order/stock commit; retry succeeds |
| Core exits after inbox commit | `after_inbox_commit` failpoint | Retry observes inbox uniqueness; no second order or stock booking |
| Core and Vendure database restart | Restart both PostgreSQL services | Queue, mappings, invoice count, order, payment, and stock invariants persist |
| Worker restart with active lease | DB test plus processing row observed before restart | Claim skips active lease and preserves `locked_at`/attempts |
| Expired lease and reclaim | Short test lease plus worker restart | Expired row returns to pending and is reclaimed; attempts remain persisted |
| Repeated payment delivery | Worker crash/retry and existing vertical duplicate delivery | Exactly one Vendure payment, one Core order, one stock movement, no Core invoice |
| Delayed older product projection | Delay the lower sequence after committing two price updates | Both events deliver; latest price remains authoritative |
| Out-of-order event delivery | Reverse availability of consecutive projections | Product, price, and stock are never reset to older values |
| Backoff and retry | Connection refusal and unsupported synthetic event | `available_at`, attempt counter, capped exponential delay, and eventual outcome persist |
| Dead state | Repeatedly deliver unsupported event to configured attempt limit | Event becomes `dead` with sanitized `last_error` visible in diagnostics |
| Controlled manual requeue | Administrator submits the same idempotency key twice | One requeue mutation, duplicate response on replay, one immutable audit row |
| Complete stack restart | Restart all Compose services after recovery | Readiness returns and all exact-once/staleness invariants still hold |
| Temporary network failure | Stop target service/DB to produce connection refusal, then restart | Eventual consistency within 90 seconds after targets are ready; no manual DB repair |
| Disabled module direct access | Disable Marketplace, DATEV, Payment, and manual Shipping modules | Direct Core/GraphQL write is rejected; re-enable is audited and restores the test flow |
| Request authentication | Old/current HMAC keys, invalid signature, reused nonce, expired timestamp | Rotation keys accepted as configured; invalid/replayed/expired calls rejected |

The DB-level tests in `backend/crates/db/src/commerce.rs` separately assert active versus expired
leases, retry timestamps, maximum attempts, redacted diagnostics, and requeue transactionality.
`commerce/test/recovery.mjs` proves these invariants through real processes and both databases.
Application effects have 90-second deadlines. Compose infrastructure start/restart commands have a
separate 180-second deadline so slow disposable-volume synchronization is not mistaken for a
delivery failure; no fixed sleep is used for readiness.

## Reproducible disposable run

Create the external Compose network once, then run only with synthetic values and a unique project
name. Do not point these commands at a retained developer or production stack.

```bash
docker network inspect proxy_net >/dev/null 2>&1 || docker network create proxy_net

POSTGRES_PASSWORD=synthetic-core-db-only \
JWT_SECRET=synthetic-jwt-secret-at-least-thirty-two-bytes \
ADMIN_USERNAME=synthetic-admin \
ADMIN_PASSWORD=synthetic-admin-password \
INTEGRATION_SECRET=synthetic-current-hmac-key-at-least-32-bytes \
INTEGRATION_KEY_ID=current \
INTEGRATION_PREVIOUS_KEY_ID=previous \
INTEGRATION_PREVIOUS_SECRET=synthetic-previous-hmac-key-at-least-32-bytes \
VENDURE_DB_PASSWORD=synthetic-vendure-db-only \
VENDURE_COOKIE_SECRET=synthetic-cookie-secret-at-least-thirty-two-bytes \
VENDURE_SUPERADMIN_USERNAME=synthetic-superadmin \
VENDURE_SUPERADMIN_PASSWORD=synthetic-superadmin-password \
APP_ENV=test \
INTEGRATION_LEASE_SECONDS=2 \
INTEGRATION_RETRY_BASE_SECONDS=1 \
INTEGRATION_RETRY_MAX_SECONDS=2 \
INTEGRATION_LEASE_MS=2000 \
INTEGRATION_RETRY_BASE_MS=100 \
INTEGRATION_RETRY_MAX_MS=500 \
INTEGRATION_MAX_ATTEMPTS=5 \
CORE_INTEGRATION_TEST_FAILPOINTS=before_inbox_commit,after_inbox_commit \
VENDURE_INTEGRATION_TEST_FAILPOINTS=after_vendure_claim,after_core_claim,before_core_ack \
COMPOSE_PROJECT_NAME=merchant-recovery-test \
docker compose --env-file /dev/null up -d --build --wait

POSTGRES_PASSWORD=synthetic-core-db-only \
JWT_SECRET=synthetic-jwt-secret-at-least-thirty-two-bytes \
ADMIN_PASSWORD=synthetic-admin-password \
INTEGRATION_SECRET=synthetic-current-hmac-key-at-least-32-bytes \
INTEGRATION_PREVIOUS_KEY_ID=previous \
INTEGRATION_PREVIOUS_SECRET=synthetic-previous-hmac-key-at-least-32-bytes \
VENDURE_DB_PASSWORD=synthetic-vendure-db-only \
VENDURE_COOKIE_SECRET=synthetic-cookie-secret-at-least-thirty-two-bytes \
VENDURE_SUPERADMIN_USERNAME=synthetic-superadmin \
VENDURE_SUPERADMIN_PASSWORD=synthetic-superadmin-password \
APP_ENV=test \
INTEGRATION_LEASE_SECONDS=2 \
INTEGRATION_RETRY_BASE_SECONDS=1 \
INTEGRATION_RETRY_MAX_SECONDS=2 \
INTEGRATION_LEASE_MS=2000 \
INTEGRATION_RETRY_BASE_MS=100 \
INTEGRATION_RETRY_MAX_MS=500 \
INTEGRATION_MAX_ATTEMPTS=5 \
CORE_INTEGRATION_TEST_FAILPOINTS=before_inbox_commit,after_inbox_commit \
VENDURE_INTEGRATION_TEST_FAILPOINTS=after_vendure_claim,after_core_claim,before_core_ack \
COMPOSE_PROJECT_NAME=merchant-recovery-test \
CORE_API_URL=http://127.0.0.1:8090/api \
STOREFRONT_API_URL=http://127.0.0.1:3001/api/shop \
CORE_ADMIN_USERNAME=synthetic-admin \
CORE_ADMIN_PASSWORD=synthetic-admin-password \
npm --prefix commerce run test:recovery

COMPOSE_PROJECT_NAME=merchant-recovery-test \
POSTGRES_PASSWORD=synthetic-core-db-only \
JWT_SECRET=synthetic-jwt-secret-at-least-thirty-two-bytes \
ADMIN_PASSWORD=synthetic-admin-password \
INTEGRATION_SECRET=synthetic-current-hmac-key-at-least-32-bytes \
VENDURE_DB_PASSWORD=synthetic-vendure-db-only \
VENDURE_COOKIE_SECRET=synthetic-cookie-secret-at-least-thirty-two-bytes \
VENDURE_SUPERADMIN_USERNAME=synthetic-superadmin \
VENDURE_SUPERADMIN_PASSWORD=synthetic-superadmin-password \
docker compose --env-file /dev/null down --volumes --remove-orphans
```

CI runs the same recovery script after the ordinary clean vertical flow. Service logs are emitted
only on failure, and the stack is always removed with its volumes.
