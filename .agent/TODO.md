# TODO

This file is the authoritative unfinished-work handoff. Do not create a competing root task list.

## Mantle live completion

- [x] Independently review, fix, green, and squash-merge PR #4; verify `main` CI.
- [x] Review the complete Mantle wiki Amazon Marketing toolchain without copying real data or
  creating another analysis runtime.
- [x] Implement and locally verify the manual Sales and Traffic import, two-period comparison,
  separated evidence classes, UI workflow, aggregate exports, backup/restore, and exact live
  three-service topology.
- [x] Commit and push `pilot/mantle-amazon-analysis-live`; require every CI job on the exact head.
- [x] Baseline `192.168.178.15`, deploy only Compose project `essentials-merchant-amazon`, add a
  private Caddy route by validated graceful reload, prove all 21 non-Caddy baseline containers
  unchanged, and separately record the concurrent Office deployment's Caddy replacement.
- [x] Run live in-memory JSON/CSV/TSV acceptance, backup, empty-target restore, secret/log/raw-Git
  checks, and record the deployed SHA/image IDs and internal route.

## External Amazon gate — blocked

- [ ] Obtain explicitly approved LWA credentials, seller/marketplace scope, Brand Analytics role,
  and the one-shot approval artifact without printing secret values.
- [ ] If approved, request exactly one short completed Sales and Traffic report through LWA and
  Reports API, with bounded polling/backoff and no scheduler or PII. Otherwise retain
  `externally_blocked_missing_approved_credentials`.
- [ ] Import an authorized real report only when its authorization and local availability are
  proven; never record its path, raw bytes, product identifiers, or business metrics in Git/logs.

## Next Mantle analysis milestone — external AI gate

- [ ] Create and fund a dedicated OpenAI API project and provide a project-scoped server-side key
  through the approved host secret path; do not reuse or export a ChatGPT browser session.
- [ ] Approve the minimized aggregate field allowlist and provider retention/data-control settings.
- [ ] Add one explicit operator-triggered strategy synthesis adapter over the existing safe summary
  export. Preserve deterministic facts as canonical, label model output as hypotheses/questions,
  request no provider-side storage, and expose no raw report or mutation capability.

## Explicitly deferred

- Ads evidence, Stripe/production payments, DHL/DPD, DATEV activation, automatic Amazon actions,
  multi-tenancy, Kubernetes, and unrelated Marketplace integrations remain outside this milestone.
