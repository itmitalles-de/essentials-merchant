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
- OpenAI receives only the existing bounded aggregate-history DTO plus the last
  validated handover. The fixed Responses request uses `store:false`, strict
  structured output, no tools/files/conversation/background mode, and no
  mutation capability. A successful row remains unique per Europe/Berlin week;
  provider failures do not consume the week.
- Pilot backup contains schema, raw evidence, deterministic analyses, and
  validated AI output, but explicitly excludes provider-secret rows even as
  ciphertext. Empty-target restore requires zero provider-secret rows.

## Verification on 2026-08-20

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
- Exact runtime head `9b8edc6e6099e9d85c44a2b6d797f00f5c88ffe8` passed all seven jobs in
  GitHub Actions run `32349661359`: frontend, backend, Amazon pilot, Commerce,
  recovery, Docker, and security.

## Current extension candidate

- The same mini-tool now has a manual aggregate Sponsored Products campaign
  import for JSON/CSV/TSV. It requires campaign-level report shape, sums only
  impressions/clicks/spend and optional attributed outcomes, discards campaign
  identifiers before normalization, and preserves attribution-window semantics.
- Weekly AI prompt v3 performs a separate public-only web-research request for
  competitor signals, category/market trends, and global events or crises. The
  synthesis receives canonical `public:*` sources and citation excerpts in
  provider citation order; internal Amazon aggregates never enter web search.
- Local candidate verification passed migration 1–20, SQLx, v10-to-v20 upgrade,
  all 103 Rust tests after final citation/report-shape hardening, Clippy with
  warnings denied, frontend
  build/lint, Chromium/axe, operation allowlist, dependency/audit/secret gates,
  synthetic import/comparison/export, and empty-target backup/restore.
- Production still runs the accepted revision and schema 19 recorded below.
  The extension is not live until its exact commit passes all CI jobs and the
  target-only rollout/acceptance updates this handoff.

## Accepted live state

- Host `192.168.178.15`, Compose project `essentials-merchant-amazon`, exactly
  PostgreSQL/backend/frontend, runs revision
  `9b8edc6e6099e9d85c44a2b6d797f00f5c88ffe8`, schema 19, the seven-module
  allowlist, and zero automatic schedules.
- Image IDs: PostgreSQL
  `sha256:75f5a96988cdf694a215073c3e9c001b706b371e2f94df3967f2efdec2787f6b`,
  backend `sha256:dd1619471558012bc4d724e85dfc417161239dfb3f8eecc27504892158f89e51`,
  frontend `sha256:cbb7f2a45b73a785fd0738b265f11e7815a23be6d11d43fa9782b444c5f94025`.
- `https://ai-marketing.mantle-climbing.de` redirects inside the SPA to
  `/ai-marketing`; live Chromium found zero login inputs, the weekly analysis
  heading, and six write-only credential inputs. Pilot session returned 200,
  password login and scoped `/api/customers` returned 403.
- The existing Caddy LAN/VPN route was unchanged and Caddy was not reloaded.
  Its container ID/restart count stayed identical. Every non-target running
  container had the exact same ID, restart count, start time, and image before
  and after deployment (baseline SHA-256
  `338658aed30d06ca14e08262ecb3a65615077991b69a41d57dd23a6b784a3389`).
- Live synthetic acceptance passed JSON/CSV/TSV, raw-hash idempotence,
  two-period comparison, JSON/Markdown/CSV export, blocked raw download, and
  blocked business mutation. No report bytes were written to a host file.
- Verified backups:
  `/opt/essentials-merchant-amazon-backups/pre-weekly-ai-61e7b38-20260820T085054Z`
  and
  `/opt/essentials-merchant-amazon-backups/live-weekly-ai-9b8edc6-20260820T085753Z`.
  The latter restored into the empty isolated project
  `essentials-merchant-amazon-restore-20260820t085753` with four reports, four
  snapshots/analyses, zero provider-secret rows, and zero schedules. Its
  acceptance containers/network were removed; its two stopped-data volumes
  were retained for an explicit later retention decision.
- Live log scanning found zero configured secret-value matches, zero raw/secret
  field markers, and zero fatal/error markers. Git remained clean and no raw
  report or credential was committed.
- No live OpenAI or Amazon credential exists. Manual analysis remains usable;
  the external provider gates stay blocked until an operator enters real,
  authorized values through the deployed write-only GUI.

## Authoritative files

- `backend/crates/server/src/provider_secrets.rs`, `auth.rs`, `pilot.rs`,
  `marketplace.rs`, `strategy_ai.rs`, and Marketplace/auth routes
- `backend/crates/db/migrations/0019_pilot_provider_secrets.sql` and
  `backend/crates/db/src/provider_secrets.rs`
- `frontend/src/pilot.ts`, `ProviderSettingsPanel.tsx`,
  `MarketplaceIntelligence.tsx`, `AuthContext.tsx`, and `App.tsx`
- `compose.mantle-amazon.yml`, pilot backup/restore scripts, and the five pilot
  documents plus `STRATEGY_AI_GATE.md`
