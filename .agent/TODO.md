# TODO

This file is the authoritative unfinished-work handoff. Do not create a competing root task list.

## Paused handoff — 2026-08-13

- [ ] Check the final result of post-merge CI run `31710613289`; backend, frontend, and Commerce
  were green when work paused, while Docker/Compose was still running.
- [ ] Confirm `main` remains clean and synchronized after the documentation checkpoint is pushed.
- [ ] Delete the merged remote/local feature branch only if explicitly requested.

## Now — external acceptance gates

- [ ] Run one approved live Amazon staging request for a non-restricted report with the correct
  seller, region, marketplace participation, Report role, and server-side LWA secret reference.
  Record rate-limit/freshness evidence without tokens, raw buyer data, or credentials.
- [ ] Validate generated EXTF-v13 fixtures with the DATEV checking program and an approved empty
  test client. Keep `export.datev` disabled and make no compatibility claim until this passes.
- [ ] Obtain Stripe and DHL sandbox/account contracts, implement their real adapters behind the
  tested provider ports, and run official signed-webhook/reconciliation acceptance tests.
- [ ] Monitor the 12 current transitive Commerce advisories and adopt only a compatible reviewed
  Vendure 3.7.x/upstream remediation; never apply npm's proposed forced 2.0.10 downgrade.

## Next — operational acceptance

- [ ] Run backup/restore with production-like data volume in an approved non-production
  environment; define external encrypted retention plus measured RPO/RTO.
- [ ] Exercise coordinated HMAC rotation and rollback in staging behind TLS/private networking.
- [ ] Validate the Admin-Center accessibility and browser matrix with automated browser tests if a
  project-standard browser harness is introduced; do not substitute manual UI acceptance.

## Recently completed

- [x] Added and twice repeated the automated Core/Vendure outage, lease, replay, stale-event,
  dead-letter/requeue, database, and complete-stack recovery matrix.
- [x] Added protected redacted integration diagnostics and audited idempotent requeue.
- [x] Implemented the Essentials+ module contract and themed Admin-Center with server-side guards.
- [x] Implemented immutable correction invoices and accounting entries without stock side effects.
- [x] Implemented deterministic read-only Marketplace Intelligence with JSON/TSV parsers, exact raw
  archive, scheduler, snapshot comparison, fake SP-API, PII filter, and no external LLM.
- [x] Implemented payment/shipping ports, fake providers, signed callbacks, reconciliation tests,
  and module-aware synthetic payment/manual shipping.
- [x] Implemented deterministic gated EXTF rendering from immutable entries.
- [x] Implemented and passed empty-environment backup/restore plus schema v10-to-v14 rehearsal.
- [x] Updated visible branding to `Essentials+ Merchant` while preserving compatibility names.
- [x] Documented all deliberately deferred functionality in `docs/NICE_TO_HAVE.md` without stubs.
