# TODO

This file is the authoritative unfinished-work handoff. Do not create a competing root task list.

## Mantle live completion

- [x] Independently review, fix, green, and squash-merge PR #4; verify `main` CI.
- [x] Review the complete Mantle wiki Amazon Marketing toolchain without copying real data or
  creating another analysis runtime.
- [x] Implement and locally verify the manual Sales and Traffic import, two-period comparison,
  separated evidence classes, UI workflow, aggregate exports, backup/restore, and exact live
  three-service topology.
- [ ] Commit and push `pilot/mantle-amazon-analysis-live`; require every CI job on the exact head.
- [ ] Baseline `192.168.178.15`, deploy only Compose project `essentials-merchant-amazon`, add a
  private Caddy route by validated graceful reload, and prove non-target containers unchanged.
- [ ] Run live in-memory JSON/CSV/TSV acceptance, backup, empty-target restore, secret/log/raw-Git
  checks, and record the deployed SHA/image IDs and internal route.

## External Amazon gate — blocked

- [ ] Obtain explicitly approved LWA credentials, seller/marketplace scope, Brand Analytics role,
  and the one-shot approval artifact without printing secret values.
- [ ] If approved, request exactly one short completed Sales and Traffic report through LWA and
  Reports API, with bounded polling/backoff and no scheduler or PII. Otherwise retain
  `externally_blocked_missing_approved_credentials`.
- [ ] Import an authorized real report only when its authorization and local availability are
  proven; never record its path, raw bytes, product identifiers, or business metrics in Git/logs.

## Next analysis milestone after live acceptance

- [ ] Add the smallest compatible Ads evidence adapter for period attribution, reusing the same
  archive/snapshot/comparison/export boundary and keeping all Ads operations read-only. Do not
  start this before the manual pilot is accepted.

## Explicitly deferred

- Stripe/production payments, DHL/DPD, DATEV activation, external AI, automatic Amazon actions,
  multi-tenancy, Kubernetes, and unrelated Marketplace integrations remain outside this milestone.
