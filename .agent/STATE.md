# Current State

## Active work — Mantle AI marketing mini-tool

- On 2026-08-20 the user explicitly resumed work and requested an internal Amazon strategy
  mini-tool at `ai-marketing.mantle-climbing.de`.
- Active branch: `pilot/mantle-amazon-analysis-live`. The deployed application/operations revision
  is `f1ec43c20a809cee3abdc87283812132c62def93`; all seven PR #5 jobs passed on that exact head in CI
  run `32336051994`.
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
- An administrator can review a closed aggregate-input SHA-256 and explicitly confirm one OpenAI
  request. The browser sends only analysis ID, current hash, and the confirmation flag; any hash
  refresh revokes the checkbox confirmation.
- `strategy_ai.rs` creates a second closed DTO containing only allowlisted catalog aggregates,
  deterministic period deltas, bounded reporting context, anomaly classes, uncertainty, and
  semantic evidence references. Raw bytes/rows, paths, archive hashes, database evidence UUIDs,
  ASIN/SKU, seller secrets, and buyer/customer/order PII are excluded.
- The fixed OpenAI Responses API request uses `store: false`, no tools/files/conversations/background
  execution, redirects disabled, bounded input/output, medium reasoning, and strict Structured
  Outputs. The model is configurable through `OPENAI_STRATEGY_MODEL`, default `gpt-5.6`.
- `OPENAI_STRATEGY_ENABLED` defaults false. Missing/disabled configuration leaves the deterministic
  workflow fully available and presents a visible disabled gate. A ChatGPT subscription is not an
  API credential or API billing entitlement.
- Output validation rejects unknown fields, bounds violations, incomplete/refused responses, and
  invented evidence references. Only validated structured output and redacted metadata are stored
  immutably in `amazon_ai_strategy_assessments`; prompts and raw provider responses are not stored
  or logged. The unique analysis/hash/model/prompt key makes repeat requests idempotent.
- The read-only middleware permits only the exact new strategy POST. The five-operation Amazon
  transport enum and every Commerce/payment/shipping/DATEV mutation boundary are unchanged.

## Verified locally

- Rust format and workspace Clippy with warnings denied pass.
- PostgreSQL migrations 1–17 and SQLx prepare/check pass.
- 19 DB + 13 domain + 8 PDF + 47 server tests pass (**87 total**), including aggregate privacy,
  provider request, disabled/missing-key gates, evidence validation, immutable persistence, and
  idempotence.
- Frontend TypeScript/Vite build and lint pass (only the three pre-existing Fast Refresh warnings).
- A fresh three-service Compose/Chromium flow passes: manual synthetic import, comparison, exports,
  disabled OpenAI gate, explicit hash confirmation against a local fake provider response,
  facts/hypotheses separation, `/ai-marketing`, mutation probes, and axe with no serious/critical
  violations.
- Synthetic pilot backup and empty-target restore preserve one validated AI assessment while
  excluding API key, prompt, and raw provider response. The v10-to-v17 upgrade rehearsal passes.
- The Amazon backup/restore rehearsal also passes with `MERCHANT_NODE_RUNTIME=container`; the
  production host no longer needs a system-wide Node installation for manifest generation or
  verification.
- Amazon operation ownership and secret scans pass. No real report, business metric, or provider
  credential was used.

## Live state and gates

- `192.168.178.15` runs clean detached revision
  `f1ec43c20a809cee3abdc87283812132c62def93` in Compose project
  `essentials-merchant-amazon`, with exactly PostgreSQL/backend/frontend. Image IDs are PostgreSQL
  `sha256:75f5a96988cdf694a215073c3e9c001b706b371e2f94df3967f2efdec2787f6b`, backend
  `sha256:325d2937867426faa13257017debc3da11ab99d9e028ecec99f41736283caf22`, and frontend
  `sha256:56bbcc509dcb8aa88d81fda7aa9da502da9a671a151692ff5b3daf5fc2597427`.
- `https://ai-marketing.mantle-climbing.de` resolves internally to `192.168.178.15`, returns HTTP
  200, and uses the existing Caddy private-source matcher. Homer serves an `AI Amazon Marketing`
  tile pointing to the canonical route. Caddy was validated and gracefully reloaded; neither Caddy
  nor Homer restarted.
- Live synthetic acceptance imported two JSON periods plus CSV/TSV, proved retry idempotence,
  produced a deterministic comparison and JSON/Markdown/CSV exports, and confirmed blocked raw
  download and business mutations. Schema 17, the seven-module allowlist, zero automatic schedules,
  raw hashes, and target logs were rechecked; no non-target container restarted.
- Verified backups are retained at
  `/opt/essentials-merchant-amazon-backups/pre-5542769-20260820T050718Z`,
  `/opt/essentials-merchant-amazon-backups/live-synthetic-5542769-20260820T052225Z`, and
  `/opt/essentials-merchant-amazon-backups/live-final-f1ec43c-20260820T054653Z`. The final backup
  ran through the committed pinned-Node fallback. An empty-target restore matched live raw/archive,
  metric, and analysis fingerprints and HTTP readiness; its containers/network were removed without
  `-v`, while its two volumes remain for audit.
- During credential-file format inspection, the former operator password appeared once in tool
  output. It was immediately treated as compromised: a new server-side credential was generated,
  the old database login was disabled, old/new login behavior returned 401/200 respectively, and
  both the private environment and credential file remain mode `0600`. No replacement value was
  printed or committed.
- No `OPENAI_API_KEY` is available locally or on the live host. Activation requires a separately
  billed, project-scoped key placed server-side without exposing its value. The live status is
  `externally_blocked_missing_pay_per_use_api_key`; a confirmed request fails closed with
  `openai_not_configured`, while manual analysis remains fully usable.
- SP-API and a real Amazon report remain separately blocked; all current acceptance data is visibly
  synthetic. Root filesystem free space after acceptance is 68 GiB.

## Authoritative files

- `backend/crates/server/src/strategy_ai.rs`, migration
  `0017_amazon_ai_strategy_assessments.sql`, Marketplace routes/state/pilot policy
- `frontend/src/pages/MarketplaceIntelligence.tsx`, `frontend/src/App.tsx`, strategy API types
- `docs/STRATEGY_AI_GATE.md`, `docs/MANTLE_AMAZON_PILOT.md`, `docs/DATA_HANDLING.md`,
  `docs/OPERATIONS.md`, `docs/API.md`
- `compose.mantle-amazon.yml`, `ops/run-node-tool.sh`, backup/restore and transport/secret contract
  scripts
