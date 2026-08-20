# TODO

This file is the authoritative unfinished-work handoff. Do not create a competing root task list.

## Mantle AI marketing mini-tool

- [x] Reuse Marketplace Intelligence rather than creating a third parser/analysis runtime.
- [x] Implement the closed aggregate provider DTO, explicit SHA-256 confirmation, fixed OpenAI
  Responses transport, strict structured result, fail-closed validation, and separate immutable
  idempotent persistence.
- [x] Add the German strategy panel and `/ai-marketing` AI-first route with visible separation from
  facts and deterministic derivations.
- [x] Keep the OpenAI feature disabled by default and the manual report workflow fully usable
  without a key.
- [x] Verify Rust/DB/frontend, fresh Compose/Chromium, privacy/transport contracts, upgrade, and
  backup/empty-target restore with synthetic data only.
- [x] Stage, commit, and push the authorized implementation and recovery fix to
  `pilot/mantle-amazon-analysis-live`; all seven PR #5 jobs passed on deployed head
  `f1ec43c20a809cee3abdc87283812132c62def93`.
- [x] Re-baseline the productive host immediately before deployment and update only Compose project
  `essentials-merchant-amazon` with full Git-SHA backend/frontend images. Do not touch its volumes
  or other projects.
- [x] Add a validated, gracefully reloaded LAN/VPN-only Caddy route for
  `ai-marketing.mantle-climbing.de` to `essentials-merchant-amazon-frontend:80` and an E-Commerce
  tile in the existing Homer config. Do not restart Caddy or Homer wholesale.
- [x] Verify the internal A record `ai-marketing.mantle-climbing.de -> 192.168.178.15` and canonical
  route through normal DNS resolution.
- [x] Run live synthetic acceptance with OpenAI disabled: login, AI-first view, existing analysis,
  disabled external gate, import/comparison/export, unchanged non-target container IDs/restarts,
  logs/secrets check, backup, and empty-target restore.

## Operational retention decision

- [ ] After a human confirms the final backup, decide whether to retain or explicitly remove the two
  stopped restore audit volumes and the segregated, manifest-less first backup attempt. Never use
  the failed partial directory as a restore source and never use `docker compose down -v`.

## External OpenAI gate

- [ ] Create a separately billed, project-scoped OpenAI API project/key. A ChatGPT Pro subscription
  is not sufficient. Provision the key only in the host's mode-0600 environment without printing,
  logging, copying to Git, or placing it in backup metadata.
- [ ] Approve provider data controls and acknowledge that `store: false` disables Responses
  application-state storage but standard abuse-monitoring logs may still be retained for up to 30
  days unless separately approved controls apply.
- [ ] Set `OPENAI_STRATEGY_ENABLED=true`, keep the chosen `OPENAI_STRATEGY_MODEL`, redeploy only the
  backend, and run one short synthetic aggregate assessment. Verify structured output, idempotent
  replay, no secret/payload logs, and the immutable assessment row before any authorized business
  analysis.

## External Amazon gates

- [ ] Obtain explicitly approved LWA credentials, seller/marketplace scope, Brand Analytics role,
  and the one-shot approval artifact without printing secret values.
- [ ] If approved, request exactly one short completed Sales and Traffic report through LWA and
  Reports API, with bounded polling/backoff and no scheduler or PII. Otherwise retain the external
  blocked status.
- [ ] Import an authorized real report only when authorization and local availability are proven;
  never record its path, raw bytes, product identifiers, or business metrics in Git/logs.

## Explicitly deferred

- Ads evidence is the only sensible next analysis extension after the strategy mini-tool has one
  accepted real/synthetic use cycle. Stripe, DHL/DPD, DATEV, automatic Amazon actions,
  multi-tenancy, Kubernetes, and unrelated Marketplace integrations remain outside this milestone.
