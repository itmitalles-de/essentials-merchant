# Current State

## Active work — Mantle AI marketing mini-tool

- On 2026-08-20 the user explicitly resumed work and requested an internal Amazon strategy
  mini-tool at `ai-marketing.mantle-climbing.de`.
- Active branch: `pilot/mantle-amazon-analysis-live`; published base HEAD remains
  `62dd38eedc0a6c05fa96fe0bdd1c26d65c161ee9`. Draft PR #5 was green at that exact head before this
  uncommitted implementation.
- The worktree contains the complete local implementation and documentation but no new commit or
  push yet. The user explicitly authorized commit, push, deployment, and use of a separately billed
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
- Amazon operation ownership and secret scans pass. No real report, business metric, or provider
  credential was used.

## Live state and gates

- No live change from this implementation has been made. `192.168.178.15` still runs clean detached
  revision `66ce755da8fc1ebed1c4cf2dadd9ec838a4c34c3` in Compose project
  `essentials-merchant-amazon` with PostgreSQL/backend/frontend only.
- Current image IDs remain PostgreSQL
  `sha256:75f5a96988cdf694a215073c3e9c001b706b371e2f94df3967f2efdec2787f6b`, backend
  `sha256:6f0b36ad79b1c54cb9b3f6ae39aeae0f1da99154970d91d443018fd618a323cb`, and frontend
  `sha256:cf2ecd75a4b036e47f87679a2ee41f6efb5d2333aaad519d21f19c0b089b4ca6`.
- Live baseline on 2026-08-20: 69 GiB free, no concurrent deployment process, private environment
  mode `0600`, existing LAN/VPN-only `merchant.mantle-climbing.de` Caddy route, and clean target
  checkout. No `.env` content was read or printed.
- Caddy and the Mantle Homer dashboard can be extended without another service: route the new host
  to `essentials-merchant-amazon-frontend:80` using the existing private-source matcher and add an
  E-Commerce tile. This has not been applied.
- Internal Windows DNS at `192.168.178.12` currently returns no A record for
  `ai-marketing.mantle-climbing.de`; `merchant.mantle-climbing.de` resolves to `192.168.178.15`.
  Credentials/authority for the AD DNS server were not supplied, so the record is an external gate.
- No `OPENAI_API_KEY` is available locally or on the live host. Activation requires a separately
  billed, project-scoped key placed server-side without exposing its value. Deployment may safely
  proceed with the panel disabled, but a real AI run cannot be claimed until that gate is satisfied.
- SP-API and a real Amazon report remain separately blocked; all current acceptance data is visibly
  synthetic.

## Authoritative files

- `backend/crates/server/src/strategy_ai.rs`, migration
  `0017_amazon_ai_strategy_assessments.sql`, Marketplace routes/state/pilot policy
- `frontend/src/pages/MarketplaceIntelligence.tsx`, `frontend/src/App.tsx`, strategy API types
- `docs/STRATEGY_AI_GATE.md`, `docs/MANTLE_AMAZON_PILOT.md`, `docs/DATA_HANDLING.md`,
  `docs/OPERATIONS.md`, `docs/API.md`
- `compose.mantle-amazon.yml`, backup/restore and transport/secret contract scripts
