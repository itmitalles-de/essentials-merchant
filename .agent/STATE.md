# Current State

## Scope

- Active branch: `pilot/mantle-amazon-analysis-live`; draft PR #5 targets
  `main`. Broader Merchant work remains paused at the user's request.
- The independently reviewed foundation from PR #4 is on `main` as squash
  commit `6a5bb899939ee2f04764898938a5404893ebc058`.
- Mantle's service stays inside existing Marketplace Intelligence. No wiki
  parser/runtime or third analysis system was copied.

## Mantle mini-tool implemented locally

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
- OpenAI receives only the existing bounded aggregate-history DTO plus the last
  validated handover. The fixed Responses request uses `store:false`, strict
  structured output, no tools/files/conversation/background mode, and no
  mutation capability. A successful row remains unique per Europe/Berlin week;
  provider failures do not consume the week.
- Pilot backup contains schema, raw evidence, deterministic analyses, and
  validated AI output, but explicitly excludes provider-secret rows even as
  ciphertext. Empty-target restore requires zero provider-secret rows.

## Local verification on 2026-08-20

- Rust format/check/Clippy with warnings denied: passed.
- Rust workspace: 19 DB + 13 domain + 8 PDF + 55 server tests = 95 passed.
- Migrations 1–19, SQLx prepare/check, and v10-to-v19 upgrade rehearsal: passed.
- Frontend build/lint: passed; only three pre-existing Fast Refresh warnings.
- Chromium/axe: both no-login/write-only/full synthetic pilot flows passed with
  no serious or critical accessibility finding.
- Synthetic CLI acceptance via the scoped session: JSON/CSV/TSV, idempotent
  retry, two-period comparison, JSON/Markdown/CSV export, raw-download and
  business-mutation denial passed. Reports existed only in memory.
- Amazon operation allowlist, repository secret scan, dependency gate, Cargo
  audit, shell syntax, and SQLx offline contract passed.
- Backup/restore rehearsal passed with a synthetic encrypted provider row in
  the source and zero provider rows in the empty-target restore.
- No real report, Amazon credential, OpenAI key, provider call, or business
  metric was used.

## Live state before this rollout

- Host `192.168.178.15`, Compose project `essentials-merchant-amazon`, exactly
  PostgreSQL/backend/frontend, currently runs revision
  `61e7b3855afaa6a378edffc39b352afd875feebe`.
- Existing internal Caddy/Homer routing for `ai-marketing.mantle-climbing.de`
  is already LAN/VPN-only and healthy. It must not be reloaded unless the
  existing route is found invalid.
- No live OpenAI or Amazon credential exists. Manual analysis remains usable;
  the external provider gates stay blocked until an operator enters real,
  authorized values through the deployed write-only GUI.
- Before deployment: commit/push the implementation, require all seven CI jobs
  green on the exact head, re-baseline the host, create a verified backup, add a
  non-printing random `PILOT_SECRETS_KEY`, and change only the target project.

## Authoritative files

- `backend/crates/server/src/provider_secrets.rs`, `auth.rs`, `pilot.rs`,
  `marketplace.rs`, `strategy_ai.rs`, and Marketplace/auth routes
- `backend/crates/db/migrations/0019_pilot_provider_secrets.sql` and
  `backend/crates/db/src/provider_secrets.rs`
- `frontend/src/pilot.ts`, `ProviderSettingsPanel.tsx`,
  `MarketplaceIntelligence.tsx`, `AuthContext.tsx`, and `App.tsx`
- `compose.mantle-amazon.yml`, pilot backup/restore scripts, and the five pilot
  documents plus `STRATEGY_AI_GATE.md`
