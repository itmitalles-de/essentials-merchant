# Current State

## Scope

- Active branch: `pilot/mantle-amazon-analysis-live`; draft PR #5 targets
  `main`. Broader Merchant work remains paused at the user's request.
- The independently reviewed PR #4 foundation is on `main` as squash commit
  `6a5bb899939ee2f04764898938a5404893ebc058`.
- Mantle's service extends Marketplace Intelligence. No wiki parser/runtime or
  third analysis system was copied.

## Mantle AI marketing tool

- Canonical internal route: `https://ai-marketing.mantle-climbing.de`.
- The dedicated no-login profile issues only a 12-hour
  `mantle-amazon-read-only` session. Its exact method/path allowlist covers
  pilot status, aggregate import/view/export, weekly strategy, curated context,
  and write-only provider setup. Regular login, ERP/settings APIs, raw report
  downloads, schedules, and every Merchant/Amazon mutation remain denied.
- Caddy admits only `192.168.178.0/24`, `10.0.0.0/8`, and `100.64.0.0/10` for
  this hostname. There is no public registration or frontend host bind.
- OpenAI and Amazon secrets can be set or replaced on the gear-linked settings
  page but are never returned. AES-256-GCM ciphertext is protected by the
  host-only `PILOT_SECRETS_KEY`; backups exclude the secret table.
- Supported evidence is official Sales and Traffic JSON/CSV/TSV and
  identifier-free aggregate Sponsored Products campaign JSON/CSV/TSV. Live
  SP-API acquisition uses only LWA and Reports API for one bounded Sales and
  Traffic report; there is no Ads API or mutating Amazon client.
- The single `Analyse` action can succeed only once per Europe/Berlin calendar
  week. It refreshes eligible aggregate Amazon evidence, performs a bounded
  public-only market/competitor/global-event research request, then runs a
  tool-free structured synthesis. Raw reports, product/customer/campaign
  identifiers, and internal metrics never enter the public-search request.
- The synthesis receives a closed aggregate DTO, the immutable curated
  Mantle/Sphagnum baseline, and the previous validated handover. Migration 21
  permits exactly one immutable baseline; the live baseline contains 13 source
  manifests and 30 reviewed statements. Raw Markdown, Notes, PII, and secrets
  were not imported.
- Prompt v4 separates facts, supported derivations, hypotheses, actions,
  uncertainty, missing evidence, sources, open questions, and the next-run
  handover. Dynamic evidence enums prevent invented evidence references.
- The action-first UI has a stable graphical result layout, a light/dark
  switch, rising-market icon/favicon, and a persistent terminal-style activity
  log. The log contains only observable sanitized stages and metadata. Hidden
  model reasoning, request bodies, credentials, signed URLs, and raw reports
  are never exposed; the visible rationale is validated structured output.

## Verified live state on 2026-08-20

- Host `192.168.178.15`, Compose project `essentials-merchant-amazon`, exactly
  PostgreSQL/backend/frontend. The host checkout and backend are
  `2812c0d85c864bdb58fe88dfcf2453989b4a8ce0`; the unchanged frontend remains
  `77a2608f222bdc099d696518784cc95052fc9b33`.
- Live image IDs:
  - PostgreSQL:
    `sha256:75f5a96988cdf694a215073c3e9c001b706b371e2f94df3967f2efdec2787f6b`
  - backend:
    `sha256:fd81da095cf87b80c73644a7c91bf89a78cb3e083f1257e464a49be676b7c7e7`
  - frontend:
    `sha256:10871780498e12fb16f90e8359a22439d07112a6c6040b60eff688a1955074c2`
- The route returns HTTPS 200. PostgreSQL is healthy at schema 21. Live counts:
  one immutable knowledge row, one successful weekly assessment, two encrypted
  provider rows, zero automatic schedules, and seven archived report
  documents.
- One authorized real Sales and Traffic acquisition completed report creation,
  polling, download, immutable archive, parsing, deterministic analysis, public
  research, and paid AI synthesis without logging business metrics or raw
  paths. The assessment was created at `2026-08-20T15:32:08.849186Z` for week
  `2026-08-17`, model `gpt-5.6`, prompt
  `mantle-amazon-weekly-strategy-v4`, with a validated handover. A repeat POST
  returned the cached row and made no second provider call. The next eligible
  action is Monday, 2026-08-24 at 00:00 Europe/Berlin.
- A one-time real 91-day baseline from 2026-05-21 through 2026-08-19 now
  contains 13 distinct, contiguous seven-day periods and one persisted
  13-snapshot aggregate. The current period was re-anchored after the
  historical backfill, so the bounded eight-result weekly context is in exact
  newest-first chronological order. No automatic schedule or second paid AI
  call was created; the existing weekly lock and handover remain intact.
- Amazon returned one legitimate edge case in which a child ASIN had two
  different parent relationships within the same report period. Commit
  `2812c0d` keeps exact duplicates fail-closed while disambiguating only those
  parent-partitioned rows. The retry succeeded. Raw values and identifiers were
  not logged or committed, and the original failed archive remains immutable.
- The successful first result used 15 bounded public sources and retained the
  fixed competitor, category, global-event/crisis, risks, opportunities,
  actions, uncertainties, and handover sections. `previous_run_context` is
  correctly false for this first successful week; the stored handover becomes
  context on the next successful week.
- The original operator-visible failures were independently resolved:
  1. exact weekly Nginx route timeout (`fff8ede`),
  2. bounded OpenAI provider timeout and stage-specific errors (`c35b5e6`),
  3. truncated/invalid strict output via dynamic evidence enums and a larger
     bounded output allowance (`0480883`).
- Frontend revision `77a2608` restores safe run metadata after reload. Live
  Chromium verified no login form, exactly one weekly action, locked state
  after success, fixed result sections, global context, visible rationale and
  handover, no settings forms on the analysis page, and no secret-shaped text.
- Relevant validation passed: Rust check and Clippy with warnings denied,
  strategy tests 8/8, isolated DB suite 65/65, frontend build/lint, focused
  Chromium/axe E2E 3/3, Nginx config test, Amazon operation allowlist, secret
  scan, synthetic JSON/CSV/TSV import/comparison/export, and idempotence.
- The baseline parser hotfix additionally passed the focused regression test,
  all 21 Marketplace tests against an isolated disposable PostgreSQL instance,
  and Clippy with warnings denied. Production log scanning after deployment
  found zero secret markers. Only the backend container changed identity;
  PostgreSQL, frontend, Caddy, and every unrelated container retained theirs.
- Verified final live backup:
  `/opt/essentials-merchant-amazon-backups/live-ai-context-77a2608-20260820T1540Z`,
  mode `0700`, manifest SHA-256
  `f1a06fa3cb5d6f070ce0ca90f0a2c2457962c4267b5333014ec0f3c7d3c15e4d`.
  The exact schema-21 allowlist passed an isolated empty-target restore with the
  knowledge row and AI assessment intact, zero provider secrets, and zero
  schedules. Production itself was not used as a restore target.
- Before/after comparison found the same 26 non-target running containers with
  exact identity, image, restart count, and start time; baseline SHA-256
  `fe6c775817727763a60b1b3a6608adc2f17c2ea910abd10176873b0cac6391a9`.
  PostgreSQL and Caddy retained their IDs and restart counts. Final backend and
  frontend log scans found zero sensitive markers.

## External gates

- Draft PR #5 is mergeable, but recent PR runs, including `32388594705`, could
  not start any of their seven jobs because the GitHub account has failed
  recent payments or needs a higher Actions spending limit. Local/live
  validation is green; keep the PR draft until that external billing gate is
  cleared and the final exact head reruns.
- The paid OpenAI call is technically proven. The operator must still confirm
  the dedicated OpenAI project budget and applicable provider data controls;
  `store:false` prevents Responses application-state storage but is not itself
  a zero-retention claim.
- There is no remaining Amazon gate for the current read-only Sales and Traffic
  path. Read-only Amazon Ads API acquisition is explicitly deferred and is the
  only sensible next analysis extension after observing the next weekly cycle.

## Authoritative files

- `backend/crates/server/src/provider_secrets.rs`, `auth.rs`, `pilot.rs`,
  `marketplace.rs`, and `strategy_ai.rs`
- `backend/crates/db/migrations/0019_pilot_provider_secrets.sql`,
  `0020_manual_amazon_ads_evidence.sql`,
  `0021_mantle_business_knowledge.sql`, and marketplace DB code
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
