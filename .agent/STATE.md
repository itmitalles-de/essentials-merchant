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
  product-mapping revisions, and write-only provider setup. Regular login,
  ERP/settings APIs, raw report downloads, schedules, and every
  Merchant/Amazon mutation remain denied.
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
- The synthesis receives a closed aggregate DTO, up to 13 newest-first weekly
  analysis documents, identifier-free aggregates for reviewed product
  mappings, the immutable curated Mantle/Sphagnum baseline, and the previous
  validated handover. Migration 21 permits exactly one immutable baseline; the
  live baseline contains 13 source manifests and 30 reviewed statements.
  Migration 22 stores append-only mapping revisions. Raw Markdown, Notes, PII,
  secrets, Child ASINs, SKUs, connection IDs, and marketplace IDs do not cross
  the AI boundary.
- Prompt v5 separates facts, supported derivations, hypotheses, actions,
  uncertainty, missing evidence, sources, open questions, and the next-run
  handover. Dynamic evidence enums prevent invented evidence references.
- The action-first UI has a stable graphical result layout, a light/dark
  switch, rising-market icon/favicon, and a persistent terminal-style activity
  log. The log contains only observable sanitized stages and metadata. Hidden
  model reasoning, request bodies, credentials, signed URLs, and raw reports
  are never exposed; the visible rationale is validated structured output.

## Verified live state on 2026-08-20

- Host `192.168.178.15`, Compose project `essentials-merchant-amazon`, exactly
  PostgreSQL/backend/frontend. The deployed backend and frontend revision is
  `ef1c63bc9e4fff2c86d3f482a2aa83411e5ac32b`.
- Live image IDs:
  - PostgreSQL:
    `sha256:75f5a96988cdf694a215073c3e9c001b706b371e2f94df3967f2efdec2787f6b`
  - backend:
    `sha256:b67a2f9a7651f211ade769e90150fd3c12ddc5fb5ff48838148a3d13cfa1ee96`
  - frontend:
    `sha256:5df9fb4dd4dbea568cee617071d26605bb747fc37c399ab34e78abbba9f1350e`
- The route returns HTTPS 200. PostgreSQL is healthy at schema 22. Live counts:
  one immutable knowledge row, one successful weekly assessment, two encrypted
  provider rows, zero automatic schedules, 22 archived report documents, 20
  snapshots, 21 deterministic analyses, and six enabled current product
  mappings covering six of 178 observed live products.
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
  historical backfill, so the bounded 13-result weekly context is in exact
  newest-first chronological order. No automatic schedule or second paid AI
  call was created; the existing weekly lock and handover remain intact.
- Six reviewed Sphagnum product mappings were entered through the new internal
  append-only mapping API. An immediate identical retry returned `unchanged`
  for all six revisions. The next eligible weekly run is prepared with 13
  analysis documents, six identifier-free product aggregates, prompt
  `mantle-amazon-weekly-strategy-v5`, and the previous validated handover. No
  extra paid call was made while the weekly lock is active.
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
- Live Chromium verified no login form, exactly one weekly action, locked state
  after success, fixed result sections, global context, visible rationale and
  handover, no settings forms on the analysis page, a working gear-linked
  product-mapping workflow with 178 observed-product options and six current
  rows, and no secret-shaped text.
- Relevant validation passed: Rust format and Clippy with warnings denied,
  isolated database tests 23/23, isolated server tests 68/68, frontend
  build/lint, focused
  Chromium/axe E2E 3/3, Nginx config test, Amazon operation allowlist, secret
  scan, synthetic JSON/CSV/TSV import/comparison/export, and idempotence.
- The baseline parser hotfix additionally passed the focused regression test,
  all 21 Marketplace tests against an isolated disposable PostgreSQL instance,
  and Clippy with warnings denied. The final product-context deployment
  recreated only this project's backend and frontend; PostgreSQL, Caddy, and
  every unrelated container retained their identities. Production log scanning
  after deployment found zero secret markers.
- Verified final live backup:
  `/opt/essentials-merchant-amazon-backups/product-context-ef1c63b-20260820T1758Z`,
  mode `0700`, manifest SHA-256
  `5d0c6698cffba428631046222aac18447df798109895cd2e685d5b9aa4135fd2`.
  The exact schema-22 allowlist passed an isolated empty-target restore with
  the knowledge row, AI assessment, and six mapping revisions intact, zero
  provider secrets, and zero schedules. Production itself was not used as a
  restore target.
- Before/after comparison found the same 26 non-target running containers with
  exact identity, image, restart count, and start time; baseline SHA-256
  `fe6c775817727763a60b1b3a6608adc2f17c2ea910abd10176873b0cac6391a9`.
  PostgreSQL and Caddy retained their IDs and restart counts. Final backend and
  frontend log scans found zero sensitive markers.

## External gates

- Draft PR #5 is mergeable and remains intentionally open for the deferred
  Merchant continuation. GitHub Actions began executing all seven jobs again on
  2026-08-20. Run `32401025466` exposed a stale schema-20 assertion in the
  upgrade rehearsal after migrations 21 and 22 had applied successfully; the
  rehearsal now verifies schema 22 plus the new append-only stores and trigger.
  Never merge a later PR #5 head without checking that exact head.
- The paid OpenAI call is technically proven. The operator must still confirm
  the dedicated OpenAI project budget and applicable provider data controls;
  `store:false` prevents Responses application-state storage but is not itself
  a zero-retention claim.
- There is no remaining Amazon gate for the current read-only Sales and Traffic
  path. The immediate evidence-quality step is to review the highest-impact
  unmapped products through the GUI. Read-only Amazon Ads API acquisition stays
  the only deferred data-source extension after observing the next weekly
  cycle.

## Authoritative files

- `backend/crates/server/src/provider_secrets.rs`, `auth.rs`, `pilot.rs`,
  `marketplace.rs`, and `strategy_ai.rs`
- `backend/crates/db/migrations/0019_pilot_provider_secrets.sql`,
  `0020_manual_amazon_ads_evidence.sql`,
  `0021_mantle_business_knowledge.sql`,
  `0022_amazon_product_mapping.sql`, and marketplace DB code
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
