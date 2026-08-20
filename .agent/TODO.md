# TODO

This file is the authoritative unfinished-work handoff. Do not create a competing root task list.

## Current pilot completion

- [x] Complete the current-branch local verification matrix: disposable PostgreSQL Rust tests and
  SQLx, frontend and retained Commerce checks, pilot Playwright/axe flow, clean/recovery vertical
  tests, pilot/full backup restore, upgrade rehearsal, security gates, and diff hygiene.
- [x] Regenerate final SBOM and dependency metadata after the final dependency state.
- [ ] Create reviewed commits, push `pilot/merchant-amazon-read-only`, and open a draft PR without
  merging it.
- [ ] Review the draft CI result. Preserve exact module/transport allowlists and do not weaken the
  existing failure matrix to make CI pass.

## External Amazon staging gate — blocked

- [ ] Obtain an explicitly approved seller, server-side SP-API secret, Brand Analytics role,
  correct region, one confirmed marketplace, and encrypted archive destination attestation.
- [ ] Run `scripts/request-amazon-staging-report.sh` in validation mode without printing secret
  values. Only after it passes, authorize one manual `GET_SALES_AND_TRAFFIC_REPORT` for a completed
  one-to-seven-day UTC period with `DAY`/`CHILD`; do not enable a scheduler or RDT.
- [ ] Record only redacted UTC/request/rate-limit/duration/size/hash/parser/count/freshness/missing-
  field evidence in the ignored staging result. Never commit the raw report, seller/ASIN/revenue
  data, or credentials.
- [ ] After the first real job succeeds, acquire one compatible second period and verify the
  deterministic facts/delta/trend/anomaly/hypothesis/action/uncertainty/evidence output. Actions
  remain suggestions only.

## Explicitly deferred — no work in this milestone

- [ ] Stripe adapter or production payment webhooks.
- [ ] DHL/DPD adapters or carrier-label generation.
- [ ] DATEV activation/checker import.
- [ ] Other marketplaces, external AI, automatic procurement or marketplace mutation.
- [ ] Multi-tenancy or Kubernetes.

Existing ports, fakes, tests, and retained implementation remain. Reconsider these only after a
successful Amazon pilot and explicit scope approval; details are in
`docs/DEFERRED_EXTERNAL_GATES.md` and `docs/NICE_TO_HAVE.md`.

## Security and later operational acceptance

- [ ] Monitor the 11 current Vendure-path GHSAs and adopt only a compatible reviewed upstream
  remediation. Never apply npm's incompatible forced downgrade automatically.
- [ ] Run pilot backup/restore with approved production-like volume in non-production encrypted
  storage and measure RPO/RTO; repository rehearsal is not that acceptance.
