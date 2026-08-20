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
- The AI-first route hides retained `SYNTHETIC-` acceptance cards, places the
  weekly action before long result output, provides an explicit light/dark
  switch, and renders five truthful activity phases: Amazon report, validation
  and KPIs, market and competition, global crises, and strategy/handover. It
  does not invent percentages or progress that the backend cannot observe.
- Current UI revision `0703a88666abe20229244043d93039b93fa0f8b8` removes
  provider/configuration forms from the analysis route. A gear in the top
  toolbar opens `/ai-marketing/settings`; the analysis button now precedes the
  animation, and hashes/model/storage metadata are collapsed under `Technische
  Laufdetails`.
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
- Revision `7c7f7dafc69be789ed7cfb95e8f30207ca410015` adds the
  fail-closed database boundary that excludes the reserved `SYNTHETIC-`
  acceptance namespace from weekly AI context. Its full DB suite passed 21/21,
  DB Clippy passed with warnings denied, and the workspace all-target check
  passed. The earlier `608691b` backup extension passed its exact local
  Sales+Ads empty-target restore with two raw hashes/two parser versions, zero
  provider secrets, and zero schedules.
- Frontend revision `998d4864a75dd4349496e3ded5ad757886479f0c` introduced the
  light/dark switch, synthetic-card suppression, action-first layout, and the
  five-phase live activity display. Build and lint passed; the new focused
  Chromium test passed 1/1. A broader reused E2E run had one unrelated Ads
  request failure against an older local backend image and was not treated as a
  clean full-suite result.
- Revision `0703a88666abe20229244043d93039b93fa0f8b8` completes the uncluttered
  layout and separate settings route. Frontend build/lint and both focused
  no-login/write-only plus pipeline Chromium tests passed 2/2; live Chromium
  verified the button above the fold and empty secret inputs on settings.
- Current backend revision `f984cd3e5bb2268d5c7523dbe497e60c70b32e06`
  accepts Amazon's official dotted `amzn1.spdoc...` document identifiers while
  retaining the closed character/length boundary and rejecting path syntax.
  Both focused unit/transport tests and server Clippy with warnings denied pass.
- GitHub run `32365245827` did not start any job. Every job was rejected before
  checkout because the account has failed recent payments or needs a higher
  Actions spending limit. Treat this as an external CI-account gate and keep PR
  #5 draft until it can be rerun successfully.
- On 2026-08-20 the operator entered both provider credential sets through the
  write-only GUI. Live status verifies OpenAI configured with one expected
  field and Amazon configured with all six expected fields, read-only approval,
  region `eu`, one marketplace, one encrypted row per provider, and zero
  schedules; secret values were neither read nor logged.
- The first real one-shot Amazon attempt refreshed LWA, created the requested
  Sales and Traffic report, polled it to `DONE`, and received a document ID.
  It then failed before `getReportDocument` because the local resource-ID
  boundary had omitted Amazon's official dot character. No report bytes,
  business metrics, or OpenAI request were produced. The fix is deployed in
  `f984cd3`; a new click remains an explicit operator action.

## Accepted live state

- Host `192.168.178.15`, Compose project `essentials-merchant-amazon`, exactly
  PostgreSQL/backend/frontend. The checked-out revision is
  `0703a88666abe20229244043d93039b93fa0f8b8`; the unchanged running backend was
  built from `f984cd3e5bb2268d5c7523dbe497e60c70b32e06`, and the frontend was built
  from `0703a88666abe20229244043d93039b93fa0f8b8`.
  Schema 20, the seven-module allowlist, and zero automatic schedules remain.
- Image IDs: PostgreSQL
  `sha256:75f5a96988cdf694a215073c3e9c001b706b371e2f94df3967f2efdec2787f6b`,
  backend `sha256:3491f32cd921a3f97aa8e6417998815f7d99cd9e2a4479a994807e0d0a0bd99c`,
  frontend `sha256:b0ac13937b72772b994bd18ac4346703322340ca741f16bb75905a42390bdc5d`.
- `https://ai-marketing.mantle-climbing.de` redirects inside the SPA to
  `/ai-marketing`. Live Chromium found the no-login AI route, exactly one
  enabled `Analyse` button, two configured-provider indicators, zero disclosed
  password values, zero synthetic result cards, the rising-market icon/favicon,
  the five pipeline phases, a working light/dark switch, no configuration form
  on the analysis route, and the separate gear-linked settings page.
- A post-deploy 403 was traced to the shared Caddy snippet allowing only seven
  fixed device IPs. Only the `ai-marketing.mantle-climbing.de` site now has a
  dedicated source matcher for `192.168.178.0/24`, `10.0.0.0/8`, and
  `100.64.0.0/10`; all other Mantle routes retain the narrower device list.
  HTTPS, pilot-session POST, and a no-login Chromium load return 200. Caddy was
  reloaded in place with zero restart; config SHA-256 is
  `ab998fcd420a47d1a14c1956884aecec78e629cb6641bb92645e56b2d527f2f9`
  and the previous file is retained as
  `/opt/caddy/Caddyfile.before-ai-marketing-lan-20260820T1323Z`.
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
  written to a host file. All six retained analyses are synthetic; the live
  weekly endpoint now reports `source_analysis_count: 0` and
  `no_analysis_data`, proving they are ineligible for a provider request.
- Frontend access logging omits all query parameters. Final scanning found zero
  upload filenames, sentinel values, query strings, secrets, or fatal markers.
- Verified live backup:
  `/opt/essentials-merchant-amazon-backups/live-market-context-608691b-20260820T1146Z`,
  mode `0700`, manifest SHA-256
  `58eb2cd3375624159e717a08ca7fecb68b6a5a46e488d0d2ead0a99caa716bc0`.
  The empty-target recovery proof was local/disposable; no additional
  production restore was run.
- Live OpenAI and Amazon credential records exist only in the encrypted
  write-only store and are excluded from backups. The Amazon LWA and Reports
  credentials are now proven through report completion, but no real document
  has yet been downloaded and the OpenAI key has not yet been called.
- The UI deployment recreated only this project's backend/frontend. The
  follow-up dotted-document-ID fix recreated only its backend. PostgreSQL and
  Caddy retained their container IDs and restart counts. The exact identity,
  restart-count, and start-time hash for every running container other than the
  replaced backend remained
  `7163964b9ede17532627b1e09a74a1b317b4949014932a33c632dacec3b4c434`;
  the post-deploy backend log scan found zero credential-shaped values.
- The `0703a88` uncluttered-layout deployment recreated only the frontend. The
  backend, PostgreSQL, Caddy, and every other running container retained the
  exact pre-deploy identity/restart/start-time baseline
  `a70248f9ac5148f96e9b0cc70320ac4b3b9489de96bd77d87f605701e28172b3`.

## Authoritative files

- `backend/crates/server/src/provider_secrets.rs`, `auth.rs`, `pilot.rs`,
  `marketplace.rs`, `strategy_ai.rs`, and Marketplace/auth routes
- `backend/crates/db/migrations/0019_pilot_provider_secrets.sql`,
  `0020_manual_amazon_ads_evidence.sql`, and provider/marketplace DB modules
- `frontend/src/pilot.ts`, `ProviderSettingsPanel.tsx`,
  `PilotProviderSettings.tsx`, `MarketplaceIntelligence.tsx`, `AuthContext.tsx`,
  `App.tsx`, and `nginx.conf`
- `compose.mantle-amazon.yml`, pilot backup/restore scripts, and the five pilot
  documents plus `STRATEGY_AI_GATE.md`

## Simple Business design-system contract

- `.simple-business-design-system.json` pins the central UI source to commit
  `e508cc2` and package version `0.1.0`; no rules are copied into this product.
- Existing product-owned UI remains legacy and upstream Vendure UI is excluded.
  Package/CI activation waits for the central GitHub Actions billing blocker.
