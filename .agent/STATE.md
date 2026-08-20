# Current State

## Scope

- Active branch: `pilot/mantle-amazon-analysis-live`; draft PR #5 targets
  `main`. Broader Merchant work remains paused at the user's request.
- The independently reviewed foundation from PR #4 is on `main` as squash
  commit `6a5bb899939ee2f04764898938a5404893ebc058`.
- Mantle's service stays inside existing Marketplace Intelligence. No wiki
  parser/runtime or third analysis system was copied.

## Mantle mini-tool

- Canonical route: `https://ai-marketing.mantle-climbing.de`.
- `MANTLE_PILOT_NO_LOGIN=true` removes the login form. The dedicated frontend
  proxy issues a 12-hour `mantle-amazon-read-only` JWT; the browser always
  discards an older token first and the normal login endpoint returns 403.
- The scoped token has an exact method/path allowlist for pilot status,
  aggregate Amazon views/import/export, weekly strategy, and write-only
  provider setup. ERP, settings, raw report, module health, schedules, and all
  business mutations remain denied.
- OpenAI and Amazon credentials can be set/replaced in the GUI but never read
  back. The backend validates and encrypts values with AES-256-GCM under the
  host-only `PILOT_SECRETS_KEY`; status exposes only booleans, field names,
  approval state, and timestamps. Migration 19 adds the opaque store.
- Amazon setup atomically creates one enabled `pilot_seller` connection with
  exactly one marketplace and `Brand Analytics`. Credential rotation is
  blocked while a live report run is active.
- The single `Analyse` button is the only AI-first acquisition action. With an
  approved live connection it creates/reuses one Sales and Traffic run for the
  last seven completed UTC days, uses bounded polling/backoff, then submits the
  refreshed aggregate hash. Without Amazon credentials it uses manual imports.
- Manual evidence supports Sales and Traffic plus identifier-free aggregate
  Sponsored Products campaign KPIs; there is no Amazon Ads API client.
- One public-only Responses request may use at most three `web_search` calls.
  A separate tool-free synthesis receives its bounded cited result, the closed
  aggregate-history DTO, and the last validated handover. Both use `store:false`
  and have no files/conversation/background or mutation capability. A
  successful row remains unique per Europe/Berlin week; provider failures do
  not consume the week.
- Pilot backup contains schema, raw evidence, deterministic analyses, and
  validated AI output, but explicitly excludes provider-secret rows even as
  ciphertext. Empty-target restore requires zero provider-secret rows.

## Verification on 2026-08-20

- Migrations 1–20, SQLx prepare/check, v10-to-v20 upgrade, all 103 Rust tests,
  Clippy with warnings denied, frontend build/lint, Chromium/axe, the Amazon
  operation boundary, dependency/advisory/secret gates, and synthetic
  import/comparison/export passed.
- Exact runtime parent `9c6f8ba30c829c42255f33311fcd838e9f761049`
  passed all seven jobs in GitHub Actions run `32363658273`.
- Live revision `608691bc5b3f490568d2d3f05561e007eb0977df`
  changes only the backup report allowlist, its Sales+Ads recovery fixture, a
  static security assertion, and operations documentation. The exact local
  empty-target backup/restore passed with two raw hashes/two parser versions,
  zero provider secrets, and zero schedules.
- GitHub run `32365245827` did not start any job. Every job was rejected before
  checkout because the account has failed recent payments or needs a higher
  Actions spending limit. Treat this as an external CI-account gate and keep PR
  #5 draft until it can be rerun successfully.
- No real report, Amazon credential, OpenAI key, provider call, or business
  metric was used.

## Accepted live state

- Host `192.168.178.15`, Compose project `essentials-merchant-amazon`, exactly
  PostgreSQL/backend/frontend, runs revision
  `608691bc5b3f490568d2d3f05561e007eb0977df`, schema 20, the seven-module
  allowlist, and zero automatic schedules.
- Image IDs: PostgreSQL
  `sha256:75f5a96988cdf694a215073c3e9c001b706b371e2f94df3967f2efdec2787f6b`,
  backend `sha256:fe11bc5f0a45fcc6131319d62b6924d54121b02cc16a6fd92896d239c49c86e9`,
  frontend `sha256:831b5063853f5e25587687b65473552e28cf57aa8616d08b2e8ea8bef660ab8c`.
- `https://ai-marketing.mantle-climbing.de` redirects inside the SPA to
  `/ai-marketing`. Live Chromium found zero login headings/buttons, exactly one
  `Analyse` button, the rising-market icon/favicon, and the Ads import.
- The single weekly action uses aggregate manual evidence until credentials
  exist. Prompt v3 first performs public-only competitor/category/global-crisis
  research, then a separate tool-free synthesis with aggregate history and the
  last validated handover. Facts, supported derivations, hypotheses, actions,
  uncertainty, missing evidence, sources, and open questions stay separated.
- The existing Caddy LAN/VPN route was unchanged and Caddy was not reloaded.
  All 26 non-target running containers retained their exact ID, image, restart
  count, and start time (baseline SHA-256
  `becf4957d74030debf19035fce8bf2489a102d9c1f6b9eba8cabb1eb83b85212`).
- Live synthetic acceptance passed Sales and Traffic JSON/CSV/TSV plus two
  aggregate Sponsored Products campaign periods, raw-hash idempotence,
  comparisons, JSON/Markdown/CSV export, identifier/search-term rejection,
  blocked raw download, and blocked business mutation. No report bytes were
  written to a host file.
- Frontend access logging omits all query parameters. Final scanning found zero
  upload filenames, sentinel values, query strings, secrets, or fatal markers.
- Verified live backup:
  `/opt/essentials-merchant-amazon-backups/live-market-context-608691b-20260820T1146Z`,
  mode `0700`, manifest SHA-256
  `58eb2cd3375624159e717a08ca7fecb68b6a5a46e488d0d2ead0a99caa716bc0`.
  The empty-target recovery proof was local/disposable; no additional
  production restore was run.
- No live OpenAI or Amazon credential exists. The write-only store is available;
  manual analysis remains fully usable while both provider gates are external.

## Authoritative files

- `backend/crates/server/src/provider_secrets.rs`, `auth.rs`, `pilot.rs`,
  `marketplace.rs`, `strategy_ai.rs`, and Marketplace/auth routes
- `backend/crates/db/migrations/0019_pilot_provider_secrets.sql`,
  `0020_manual_amazon_ads_evidence.sql`, and provider/marketplace DB modules
- `frontend/src/pilot.ts`, `ProviderSettingsPanel.tsx`,
  `MarketplaceIntelligence.tsx`, `AuthContext.tsx`, `App.tsx`, and `nginx.conf`
- `compose.mantle-amazon.yml`, pilot backup/restore scripts, and the five pilot
  documents plus `STRATEGY_AI_GATE.md`
