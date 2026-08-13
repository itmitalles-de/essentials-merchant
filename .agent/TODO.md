# TODO

This file is the authoritative repository task handoff. Do not recreate a
competing root task list. GitHub issues may track execution, but unfinished work
must be summarized here for session continuation.

## Now

- [ ] Perform the documented manual Amazon staging gate with an approved seller,
  assigned Report roles, marketplace IDs, LWA secret reference, rate-limit
  handling, and a real non-restricted report. Do not log credentials or claim
  live validation before this gate.
- [ ] Extend `commerce/test/vertical.mjs` and CI with deliberate Core and Vendure
  outages plus Vendure-worker restart cases.
- [ ] Assert persisted lease expiry/reclaim, replay safety, retry/backoff behavior,
  and eventual recovery without duplicate order or stock booking.
- [ ] Review available compatible Vendure 3.7.x patches for the advisories in
  `README.md`; do not accept npm's incompatible forced downgrade, and rerun the
  complete vertical test for any upgrade.

## Next

- [ ] Select and implement one production payment provider with signed webhooks,
  idempotency, reconciliation, and failure tests.
- [ ] Select and implement one production shipping provider with fulfillment,
  tracking, signed callbacks where applicable, and reconciliation.
- [ ] Implement cancellation/correction invoices while preserving issued invoice
  snapshots, numbering, and immutable accounting history.
- [ ] Implement DHL and DPD only as independent connector modules with real
  configuration validation, health checks, webhook-disable behaviour, and their
  own idempotency/recovery tests; do not couple them to Marketplace Intelligence.

## Later

- [ ] Implement DATEV EXTF from immutable accounting entries and validate it
  against an authoritative format reference; make no compatibility claim before that.
- [ ] Consider shipping labels, marketplace adapters, and B2B price lists/channels
  only after the reliability, provider, and correction-invoice work above.

## Blocked

- [ ] Removing the recorded Vendure dependency advisories is blocked on a
  compatible upstream patch or a separately reviewed safe remediation.

## Recently completed

- [x] Fixed SQLx offline and Typst CI prerequisites; current `main` CI is green.
- [x] Added separate Vendure server/worker/database, Dashboard, and Storefront.
- [x] Added product/price/stock projection, idempotent paid-order import, and
  fulfillment/tracking projection with a passing vertical CI flow.
- [x] Replaced the generic root handoff with this persistent task source.
- [x] Added the optional read-only Marketplace Intelligence module with persistent
  report runs, raw archives, JSON/TSV fixtures/parsers, snapshots, deterministic
  analysis, an optional provider seam, and an Essentials Plus Admin-Center.
- [x] Validated the Marketplace slice with a disposable PostgreSQL database,
  refreshed SQLx offline metadata, frontend/commerce builds, and an isolated
  healthy Compose stack including the synthetic report flow.
