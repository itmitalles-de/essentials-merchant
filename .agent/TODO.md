# TODO

This file is the authoritative repository task handoff. Do not recreate a
competing root task list. GitHub issues may track execution, but unfinished work
must be summarized here for session continuation.

## Now

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
