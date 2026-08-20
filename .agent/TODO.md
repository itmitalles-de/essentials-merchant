# TODO

This file is the authoritative unfinished-work handoff. Do not create a
competing root task list.

## Current Mantle rollout

- [x] Reuse Marketplace Intelligence and its parser/archive/comparison boundary.
- [x] Add a no-login, 12-hour Amazon-only session with exact route scoping and
  server-side regular-login denial.
- [x] Add encrypted write-only OpenAI/Amazon GUI setup under a host-only master
  key; never return stored values.
- [x] Make the single weekly `Analyse` button create/reuse one seven-day
  Sales-and-Traffic run when Amazon is configured, otherwise use manual imports.
- [x] Preserve the closed aggregate DTO, previous validated handover, fixed
  graphical/section layout, one-success-per-Berlin-week database boundary, and
  zero Amazon/Merchant mutations.
- [x] Exclude provider credential rows from pilot backups and prove zero rows in
  an empty-target restore.
- [x] Complete local Rust/frontend/Chromium/security/SQLx/upgrade/recovery and
  synthetic JSON/CSV/TSV acceptance.
- [x] Commit and push the exact feature head to draft PR #5; require all seven
  CI jobs green.
- [x] Re-baseline production, create a verified pre-deploy backup, deploy only
  Compose project `essentials-merchant-amazon`, and preserve every non-target
  container ID/restart count.
- [x] Verify no-login route, scoped ERP denial, write-only empty status, schema
  19, zero schedules, logs without secrets, synthetic import/comparison/export,
  backup, and empty-target restore on the deployed SHA.
- [x] Record exact live commit, image IDs, backup/restore evidence, and remaining
  external gates in this handoff and `docs/MANTLE_AMAZON_PILOT.md`.

## Public-market and Ads evidence extension

- [x] Add aggregate manual Sponsored Products campaign JSON/CSV/TSV evidence
  without creating an Ads API or retaining identifiers in normalized output.
- [x] Add separate public-only research for competitors, category/market, and
  global events/crises with citation-ordered references and explicitly
  uncertain consumption effects.
- [x] Keep the synthesis fixed-structure, handover-aware, aggregate-only,
  tool-free, weekly, and incapable of Amazon or Merchant mutation.
- [ ] Commit and push the exact extension head, require all seven CI jobs green,
  and deploy only `essentials-merchant-amazon` with Git-SHA image tags.
- [ ] Run live synthetic Ads import/comparison/idempotence/export acceptance,
  verify schema 20/no-login/scoped denials/logs/non-target baseline, and record
  the exact live commit and image IDs here.

## External OpenAI gate

- [ ] In the OpenAI Platform, create a dedicated project with pay-per-use
  billing/budget and a project/service-account API key. ChatGPT Pro is separate.
- [ ] Approve the applicable provider data controls; `store:false` removes
  Responses application-state storage but is not a zero-retention claim.
- [ ] Enter the real key only through the internal write-only GUI. Run one
  authorized aggregate assessment, verify the fixed result/handover and weekly
  lock, and check logs without printing the key or payload.

## External Amazon gate

- [ ] Register and self-authorize a private SP-API app with only the required
  Reports/Brand Analytics access; obtain LWA Client ID/Secret, Refresh Token,
  Seller ID, Marketplace ID, and region.
- [ ] Enter those values only through the internal write-only GUI and confirm
  Mantle authorization/read-only scope. Do not create fake credentials.
- [ ] Run exactly one completed seven-day Sales and Traffic acquisition through
  the weekly button; verify request-ID redaction, polling/backoff, hashes,
  parser, analysis, and absence of Buyer/Order PII.
- [ ] Import an authorized real report only when authorization and local
  availability are proven. Never record its path, raw bytes, product IDs, or
  business metrics in Git/logs.

## Explicitly deferred

- Read-only Amazon Ads API acquisition is the only sensible next analysis
  extension after one accepted real weekly cycle. Stripe, DHL/DPD, DATEV,
  automatic Amazon actions,
  multi-tenancy, Kubernetes, and unrelated marketplace work remain outside this
  milestone.
- Retained stopped restore volumes and historical partial backup directories
  require a separate human retention/deletion decision. Never use
  `docker compose down -v` on production.
