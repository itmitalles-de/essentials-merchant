# Essentials+ Merchant API boundaries

This is the human-readable contract for new operational APIs. Existing CRUD routes retain their
established shapes. Every human route uses the Core bearer token; administrator-only routes also
verify `role=administrator`. Module-bound routes fail with HTTP 409 and `module_disabled` before
business logic when the corresponding module is off.

When `pilot.amazon_read_only` is enabled, a second server-wide policy applies before route
handlers. Only safe reads plus the explicitly listed Amazon acquisition and deterministic analysis
commands are accepted. Every other `POST`, `PUT`, `PATCH`, or `DELETE` request returns HTTP 409
with `pilot_read_only`; this includes otherwise valid Core, Commerce, payment, shipping, DATEV,
scheduler, and integration mutation routes.

## Modules

`GET /api/modules` returns the complete manifest catalog to administrators. Normal users receive
only enabled modules with an explicit `user_module_permissions` grant.

`PUT /api/modules/{module_id}` accepts `{ "enabled": true|false }`, requires administrator role and
`Idempotency-Key`, validates required modules, dependencies, conflicts, installation state, and
connector health in one transaction, then audits the transition.

`GET /api/modules/{module_id}/health` is administrator-only. Synthetic/manual connectors return
their stored deterministic health. DHL/DPD checks only whether the server-side secret reference is
configured; it does not call or mutate a carrier.

`GET /api/pilot/status` is administrator-only. It returns the exact active pilot-module set,
unexpected active modules, disabled mutating modules, schedule count, and the most recent pilot
backup verification. Redacted connection/report diagnostics come separately from the Marketplace
overview and run-detail reads. Neither surface returns secret references or values, complete seller
IDs, buyer data, or report payloads.

## Integration diagnostics

`GET /api/integration-diagnostics` requires administrator role. It returns Core/Vendure queue
summaries, sanitized event metadata, mappings, remote health/readiness, and recent audit entries.
It deliberately excludes payloads, secrets, tokens, customer data, and full provider responses.

`POST /api/integration-diagnostics/events/{source}/{event_id}/requeue` requires administrator role
and `Idempotency-Key`. `source` is `core` or `vendure`; only a dead event is accepted. Vendure
requeues are delivered as a signed administrative command. Replays return `duplicate: true`.

Core↔Vendure `/api/integrations/vendure/*` routes are not human APIs. Each request is limited to
256 KiB and must carry the HMAC key ID, timestamp, nonce, and signature over the canonical request.

## Correction invoices

`POST /api/invoices/{invoice_id}/corrections` is protected by `accounting.corrections`, requires
`Idempotency-Key`, and accepts `{ "reason": "..." }`. The source must be an issued ordinary invoice
without an existing full correction. Success returns `{ correction, duplicate }` with HTTP 201 for
the first request and HTTP 200 for an idempotent replay.

The correction is created and issued atomically with a reserved correction number and reversed
Decimal lines, tax, and totals. The existing PDF endpoint renders the immutable correction
snapshot and its explicit source reference. Creating a correction never creates a stock movement.

## DATEV export

`POST /api/exports/datev` is protected by the disabled-by-default `export.datev` module and requires
`Idempotency-Key`. The request supplies period/fiscal-year dates, advisor/client number, account
length, accounting framework, currency, customer-account map, revenue-account map by tax rate, and
tax-key map by tax rate.

Success returns `text/csv; charset=utf-8`, attachment name `EXTF_Buchungsstapel.csv`,
`X-Content-Sha256`, and `X-Idempotent-Replay`. Reusing a key with different parameters is HTTP 409.
Invalid periods/mappings or empty entries are HTTP 422. The stored byte payload is immutable.

## Marketplace Intelligence

All routes below are protected by `marketplace.amazon_intelligence`:

- `GET /api/marketplace`: connection status without secrets, registry, schedules, recent runs,
  snapshots/analyses.
- `POST /api/marketplace/demo`: administrator-only synthetic fixture connection.
- `POST /api/marketplace/connections`: administrator-only redacted connection configuration.
- `POST /api/marketplace/connections/{id}/runs`: manual report job with marketplace, report type,
  UTC range, and allowlisted options.
- `PUT /api/marketplace/connections/{id}/schedules`: administrator-only interval schedule using the
  same persistent run path.
- `GET /api/marketplace/runs/{id}`: run, complete state history, document metadata/parser status,
  normalized metrics, snapshot, and analyses.
- `GET /api/marketplace/runs/{id}/raw`: administrator-only unchanged transport document.
- `POST /api/marketplace/connections/{id}/analyses`: deterministic aggregate-period analysis.
- `GET /api/marketplace/analyses/{id}/export`: PII-minimized, allowlisted aggregate JSON.

Unknown `GET_*` report types are accepted only by the fixture connection, archived as raw bytes,
and end as raw-only rather than successfully analysed. Live connections accept only registry types
whose role, region, marketplace, and options validate.

The Amazon pilot narrows this retained API further. Schedules and raw-document downloads are
blocked; connector health probes that persist a result are also blocked. Fixture jobs remain
available, and live jobs require administrator role plus a scoped,
server-side `pilot_seller` secret reference and staging approval matching the seller hash, region,
and marketplace. The only permitted
live report is `GET_SALES_AND_TRAFFIC_REPORT`, requested manually for one completed UTC period of
one to seven days with `DAY`/`CHILD` options. The transport itself is sealed to LWA refresh,
`createReport`, `getReport`, `getReportDocument`, and the validated presigned report download.
Method and path are derived from that operation enum; callers cannot supply arbitrary Amazon URLs.

## Direct connector boundaries

Core fulfillment `POST /api/sales-orders/{id}/fulfill` requires both `core.orders` and
`shipping.manual`. Vendure's synthetic payment handler calls the signed module-status endpoint on
every create/settle/cancel and fails closed unless `payment.test` is explicitly enabled.

Provider-neutral payment/shipping ports and fake callback verification currently have no public
HTTP routes. A production adapter must add module-bound, signature-checked, replay-safe endpoints
without weakening these contracts.
