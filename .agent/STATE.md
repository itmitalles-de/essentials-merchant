# Current State

## Active work — Mantle AI marketing mini-tool

- On 2026-08-20 the user explicitly resumed work and requested an internal Amazon strategy
  mini-tool at `ai-marketing.mantle-climbing.de`.
- Active branch: `pilot/mantle-amazon-analysis-live`. The deployed application revision is
  `61e7b3855afaa6a378edffc39b352afd875feebe`; all seven PR #5 jobs passed on that exact head in CI
  run `32341733986`.
- Draft PR #5 remains open and intentionally unmerged because broader Merchant work is paused. The
  user explicitly authorized this Mantle deployment and future use of a separately billed
  pay-per-use OpenAI API key on 2026-08-20.

## Broader Merchant handover

- The independently reviewed Amazon read-only foundation from PR #4 is already on `main` as squash
  commit `6a5bb899939ee2f04764898938a5404893ebc058`.
- Broader Merchant product work is deliberately paused. Commerce, Storefront, payment, shipping,
  DATEV and automatic operational workflows are not part of this mini-tool milestone and remain
  disabled/absent from the Mantle pilot runtime.
- Draft PR #5 and this branch contain the existing manual Amazon analysis/live-pilot work plus the
  new optional AI interpretation layer. Resume from `.agent/TODO.md`; do not recreate the parser,
  archive, comparison engine, or a separate Mantle analytics service.

## Implemented boundary

- The mini-tool is part of existing Marketplace Intelligence, not another parser, data store, or
  Compose service. `/ai-marketing` opens the AI-first variant; the canonical host is
  `ai-marketing.mantle-climbing.de`.
- Operators still import official Sales and Traffic JSON/CSV/TSV into the existing immutable
  archive and deterministic comparison pipeline. Facts and supported derivations remain canonical.
- Marketplace Intelligence has exactly one administrator button named `Analyse`. A successful
  request is allowed once per Monday-based calendar week in `Europe/Berlin`; the server and a
  partial database unique index enforce the limit. Provider failures create no weekly row and do
  not consume the week.
- `strategy_ai.rs` creates one bounded, newest-first history DTO from distinct allowlisted catalog
  aggregates plus deterministic period deltas, anomaly classes, uncertainty, and semantic evidence
  references. The previous validated assessment and handover are included as untrusted continuity
  context. Raw bytes/rows, paths, archive hashes, database evidence UUIDs, ASIN/SKU, seller secrets,
  and buyer/customer/order PII are excluded.
- The fixed OpenAI Responses API request uses `store: false`, no tools/files/conversations/background
  execution, redirects disabled, bounded input/output, medium reasoning, and strict Structured
  Outputs. The model is configurable through `OPENAI_STRATEGY_MODEL`, default `gpt-5.6`.
- `OPENAI_STRATEGY_ENABLED` defaults false. Missing/disabled configuration leaves the deterministic
  workflow fully available and presents a visible disabled gate. A ChatGPT subscription is not an
  API credential or API billing entitlement.
- Output validation rejects unknown fields, bounds violations, incomplete/refused responses, and
  invented evidence references. The fixed output ends with a structured handover. Only validated
  output and redacted metadata are stored immutably in `amazon_ai_strategy_assessments`; prompts and
  raw provider responses are not stored or logged. `week_start` and `previous_assessment_id` form
  the durable weekly continuity chain.
- The read-only middleware permits only exact `POST /api/marketplace/strategy/weekly`. The
  five-operation Amazon transport enum and every Commerce/payment/shipping/DATEV mutation boundary
  are unchanged.

## Verified locally

- Rust format and workspace Clippy with warnings denied pass.
- PostgreSQL migrations 1–18 and SQLx prepare/check pass.
- 19 DB + 13 domain + 8 PDF + 48 server tests pass (**88 total**), including weekly history/privacy,
  provider request, disabled/missing-key gates, evidence validation, immutable persistence, and
  idempotence.
- Frontend TypeScript/Vite build and lint pass (only the three pre-existing Fast Refresh warnings).
- A fresh three-service Compose/Chromium flow passes: manual synthetic import, comparison, exports,
  disabled OpenAI gate, weekly button and fixed KPI/handover structure, hash confirmation against a
  local fake provider response, facts/hypotheses separation, mutation probes, and axe with no
  serious/critical violations.
- Synthetic pilot backup and empty-target restore preserve two linked weekly AI assessments while
  excluding API key, prompt, and raw provider response. The v10-to-v18 upgrade rehearsal passes.
- The Amazon backup/restore rehearsal also passes with `MERCHANT_NODE_RUNTIME=container`; the
  production host no longer needs a system-wide Node installation for manifest generation or
  verification.
- Amazon operation ownership and secret scans pass. No real report, business metric, or provider
  credential was used.

## Live state and gates

- `192.168.178.15` runs clean detached revision
  `61e7b3855afaa6a378edffc39b352afd875feebe` in Compose project
  `essentials-merchant-amazon`, with exactly PostgreSQL/backend/frontend. Image IDs are PostgreSQL
  `sha256:75f5a96988cdf694a215073c3e9c001b706b371e2f94df3967f2efdec2787f6b`, backend
  `sha256:44e1fc334437608e1cc29a1039de4d8b96b035bc55aef4af45d1941429d473e0`, and frontend
  `sha256:8427903ec0a2eb8da7562804cc81eb67e8b0a698c1cd99b91e1ecff48b39858c`.
- `https://ai-marketing.mantle-climbing.de` resolves internally to `192.168.178.15`, returns HTTP
  200, and uses the existing Caddy private-source matcher. Homer serves an `AI Amazon Marketing`
  tile pointing to the canonical route. Caddy was validated and gracefully reloaded; neither Caddy
  nor Homer restarted.
- Live synthetic acceptance imported two JSON periods plus CSV/TSV, proved retry idempotence,
  produced a deterministic comparison and JSON/Markdown/CSV exports, and confirmed blocked raw
  download and business mutations. Schema 18, the seven-module allowlist, zero automatic schedules,
  raw hashes, and target logs were rechecked; no non-target container restarted.
- Weekly live acceptance collected four existing synthetic aggregate analyses into one stable input
  hash. The deployed UI contains the fixed KPI and handover structure. GET returned the Berlin week
  starting `2026-08-17`; the missing provider gate returned `openai_not_configured`, created no row,
  and left the hash stable. No successful provider call or real business analysis ran.
- Verified backups are retained at
  `/opt/essentials-merchant-amazon-backups/pre-5542769-20260820T050718Z`,
  `/opt/essentials-merchant-amazon-backups/live-synthetic-5542769-20260820T052225Z`, and
  `/opt/essentials-merchant-amazon-backups/live-final-f1ec43c-20260820T054653Z`. The weekly rollout
  adds verified pre/post backups
  `/opt/essentials-merchant-amazon-backups/pre-weekly-61e7b38-20260820T071000Z` and
  `/opt/essentials-merchant-amazon-backups/live-weekly-61e7b38-20260820T072000Z`. The final backup
  ran through the committed pinned-Node fallback. An empty-target restore matched live raw/archive,
  metric, analysis, schema-18, and HTTP fingerprints. Its containers/network were removed, while
  `essentials-merchant-amazon-restore-weekly-61e7b38_erplite_db_data` and
  `essentials-merchant-amazon-restore-weekly-61e7b38_erplite_invoices` were retained for audit.
- During credential-file format inspection, the former operator password appeared once in tool
  output. It was immediately treated as compromised: a new server-side credential was generated,
  the old database login was disabled, old/new login behavior returned 401/200 respectively, and
  both the private environment and credential file remain mode `0600`. No replacement value was
  printed or committed.
- No `OPENAI_API_KEY` is available locally or on the live host. Activation requires a separately
  billed, project-scoped key placed server-side without exposing its value. The live status is
  `externally_blocked_missing_pay_per_use_api_key`; a confirmed weekly request fails closed with
  `openai_not_configured`, while manual analysis remains fully usable.
- SP-API and a real Amazon report remain separately blocked; all current acceptance data is visibly
  synthetic. Root filesystem free space after acceptance is 67 GiB.

## Authoritative files

- `backend/crates/server/src/strategy_ai.rs`, migration
  `0017_amazon_ai_strategy_assessments.sql`, migration `0018_weekly_amazon_ai_strategy.sql`,
  Marketplace routes/state/pilot policy
- `frontend/src/pages/MarketplaceIntelligence.tsx`, `frontend/src/App.tsx`, strategy API types
- `docs/STRATEGY_AI_GATE.md`, `docs/MANTLE_AMAZON_PILOT.md`, `docs/DATA_HANDLING.md`,
  `docs/OPERATIONS.md`, `docs/API.md`
- `compose.mantle-amazon.yml`, `ops/run-node-tool.sh`, backup/restore and transport/secret contract
  scripts
