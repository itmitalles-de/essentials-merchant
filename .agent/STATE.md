# Current State

## Product and branch

- Repository: **Essentials+ Merchant**, `itmitalles-de/essentials-merchant`.
- Active branch: `pilot/mantle-amazon-analysis-live`, based on merged PR #4 commit
  `6a5bb899939ee2f04764898938a5404893ebc058`.
- The `amazon-read-only` profile remains fail-closed. Its exact runtime is Core PostgreSQL, Rust
  backend, and Core frontend; Vendure, Storefront, payment, shipping, DATEV, schedules, external AI,
  and every Amazon business mutation are absent or blocked.

## Mantle manual-analysis path

The first useful path no longer depends on SP-API credentials. Authenticated internal operators can
preview and atomically import official `GET_SALES_AND_TRAFFIC_REPORT` JSON, CSV, or TSV up to 10 MiB.
The parser validates the report shape, rejects PII-like input, uses exact decimals, requires explicit
timezone and missing flat-file metadata confirmation, and retains parser/field provenance.

Exact raw bytes and SHA-256 are archived immutably in PostgreSQL. Identical bytes are idempotent;
different bytes for the same semantic marketplace/report/period/comparability key are rejected.
Compatible non-overlapping periods require equal marketplace, report type, date/ASIN granularity,
period length, timezone, currency, and parser. Output visibly separates facts, deterministic
derivations, hypotheses, possible measures, uncertainty, missing evidence, and open questions.
Aggregate-only JSON, Markdown, and CSV exports never contain raw rows.

`compose.mantle-amazon.yml` and `scripts/start-mantle-amazon.sh` define the production-facing Mantle
project `essentials-merchant-amazon`, SHA-tagged backend/frontend images, and a loopback-only frontend
port. Backup/restore supports that Compose file and includes manual-import provenance and parser
versions.

## Verification and live status — 2026-08-20

- PR #4 was reviewed at exact head `ddf0f18d7aa455af715469bae106a8205da8347f`, all seven jobs
  passed, and it was squash-merged; the resulting `main` run also passed all seven jobs.
- The full Mantle wiki `amazon/marketing/**` toolchain and its 27 tests were reviewed. Only parser
  concepts and test cases were adapted; no real/product-identifying fixtures, cache, generated
  reports, or parallel runtime were copied.
- Local current-branch checks pass: Rust fmt, Clippy with warnings denied, migrations 1–16, SQLx
  prepare/check, 18 DB + 13 domain + 8 PDF + 42 server tests, frontend build/lint, and RustSec audit.
- A fresh three-service Compose project passed the Chromium/axe operator flow, in-memory JSON/CSV/TSV
  imports, upload-order-independent comparison, idempotent retry, all three exports, mutation/raw
  download probes, upgrade rehearsal, and backup/empty-project restore.
- RustSec lockfile-only exceptions `RUSTSEC-2026-0235` (`rkyv`) and `RUSTSEC-2023-0071` (`rsa`) are
  documented and CI proves `rkyv`, `rsa`, and `sqlx-mysql` absent from every compiled target before
  applying those narrow exceptions. Retained Vendure advisories remain open and outside the pilot
  runtime.
- Commit `66ce755da8fc1ebed1c4cf2dadd9ec838a4c34c3` is live on `192.168.178.15` as Compose project
  `essentials-merchant-amazon`. The LAN/VPN-only route is `https://merchant.mantle-climbing.de`;
  internal split DNS resolves it to `192.168.178.15`, while public DNS remains on the external host.
- The running image IDs are PostgreSQL
  `sha256:75f5a96988cdf694a215073c3e9c001b706b371e2f94df3967f2efdec2787f6b`, backend
  `sha256:6f0b36ad79b1c54cb9b3f6ae39aeae0f1da99154970d91d443018fd618a323cb`, and frontend
  `sha256:cf2ecd75a4b036e47f87679a2ee41f6efb5d2333aaad519d21f19c0b089b4ca6`.
- Live synthetic acceptance imported JSON, CSV, and TSV entirely in memory. The identical JSON retry
  returned the original run; the second period produced a comparison; aggregate JSON, Markdown, and
  CSV exports succeeded; raw download and business mutation probes were blocked.
- A checksum-backed live backup was restored into the empty isolated project
  `essentials-merchant-amazon-restore-20260820`. Counts, four document hashes, and parser
  `manual-sales-traffic-v1` matched live. The restore project is stopped with its volumes retained.
- All 21 non-Caddy pre-existing baseline containers kept their IDs, start times, and restart counts.
  A concurrent Office/Nextcloud deployment replaced Caddy after the initial baseline; the Merchant
  route preserved that newer route set and used only validated graceful reload. Current Caddy has
  zero restarts since that external replacement.

## External gates

- **SP-API:** externally blocked until explicitly approved LWA credentials, seller/marketplace
  scope, role, and one-shot approval exist. No fake credentials are created; manual upload is fully
  usable. The only transport operations remain LWA refresh, Reports create/get/document, and a
  validated report download.
- **Generative strategy AI:** the live service remains deterministic and has no OpenAI credential.
  A ChatGPT subscription is not an API credential or API billing entitlement. The next approved
  implementation may send only an explicitly requested, minimized aggregate analysis export from a
  dedicated server-side OpenAI API project; raw reports, identifiers, secrets, schedulers, and
  mutation tools remain prohibited.
- **Real report:** none is authorized in repository or local test state. All acceptance data is
  visibly synthetic and generated in memory.

## Authoritative files

- `README.md`, `.agent/ARCHITECTURE.md`, `.agent/DECISIONS.md`, `.agent/TODO.md`
- `docs/MANTLE_AMAZON_PILOT.md`, `docs/MANUAL_REPORT_IMPORT.md`, `docs/SP_API_GATE.md`,
  `docs/DATA_HANDLING.md`, `docs/OPERATIONS.md`
- migration `0016_manual_amazon_report_import.sql`, `compose.mantle-amazon.yml`, `scripts/`, and `ops/`
