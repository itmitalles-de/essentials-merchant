# Essentials+ Merchant

Essentials+ Merchant is currently scoped to one internal pilot: **read-only Amazon Marketplace
Intelligence**. It imports official Amazon Reports manually without credentials, can optionally
acquire the same report through an approved SP-API gate, preserves immutable raw evidence, creates
versioned deterministic snapshots and analyses, and exports only PII-minimized aggregates.

The retained ERP and Commerce implementation remains in this repository and stays covered by its
existing tests. It is outside the pilot runtime: Vendure, Storefront, checkout, payment, shipping,
fulfillment mutation, DATEV activation, and every Amazon business mutation are disabled. No
production, Amazon-account, legal, tax, carrier, payment, or DATEV compatibility claim is made.

The repository is `itmitalles-de/essentials-merchant`. Historical internal `erplite` and
`shop-suite-*` values remain stable persistence and integration contracts; databases, volumes,
migrations, token keys, and mapping IDs are deliberately not renamed. See
[compatibility identifiers](docs/COMPATIBILITY_IDENTIFIERS.md).

## Pilot profile

The persisted profile `amazon-read-only` enables exactly:

- `core.operations`, `core.catalog`, `core.inventory`, and `core.orders`;
- `marketplace.amazon_intelligence` and `intelligence.rules`;
- `pilot.amazon_read_only`.

It disables Commerce/Storefront, `payment.test`, every real payment module, every shipping module,
`export.datev`, and custom mutating modules. Existing `not_installed` states are preserved. The
backend applies the profile atomically, disables Amazon schedules, holds pre-existing nonterminal
live runs, and refuses startup if any unexpected module remains active. A server-wide middleware
returns HTTP 409 `pilot_read_only` for all unsafe methods except the small Amazon
configuration/acquisition/analysis allowlist; hiding navigation is never the security boundary.

The standalone topology contains exactly PostgreSQL, the Rust backend, and the React admin:

```text
Admin UI  ->  Core API  ->  PostgreSQL + immutable Amazon archive
                 |
                 +---- LWA + Amazon Reports API only (explicit live gate)
```

Vendure, Storefront, payment, shipping, carrier, and DATEV services are absent from both
`compose.amazon-pilot.yml` and the Mantle live definition `compose.mantle-amazon.yml`. Their code,
databases, compatibility identifiers, ports, fakes, and tests remain available to the retained
full-stack test topology.

## Safe local start

Prepare a local ignored environment from `.env.amazon-pilot.example`. The launcher defaults to a
dry configuration check, fixes the Compose project name to
`essentials-merchant-amazon-pilot`, prints no secrets, and never deletes data:

```bash
scripts/start-amazon-pilot.sh --env-file .env.amazon-pilot
```

Starting requires the explicit `--start` switch. After startup, the launcher checks the running
service set and persisted module state as machine-readable JSON. It stops the application services
and exits nonzero if a Commerce, payment, shipping, or DATEV service/module is unexpectedly active
or an Amazon schedule exists. Full operating instructions are in
[docs/OPERATIONS.md](docs/OPERATIONS.md).

The Mantle deployment uses the separate fixed project `essentials-merchant-amazon`, exact
Git-SHA image tags, and a loopback-only frontend port. Its launcher is
`scripts/start-mantle-amazon.sh`; operator and data-handling details are in
[the Mantle pilot guide](docs/MANTLE_AMAZON_PILOT.md) and
[the manual import guide](docs/MANUAL_REPORT_IMPORT.md).

## Manual report path

The immediately usable path accepts official `GET_SALES_AND_TRAFFIC_REPORT` JSON, CSV, or TSV up
to 10 MiB. Preview is side-effect-free and exposes the detected format, SHA-256, report type,
period, marketplace, granularity, timezone, currency, parser version, missing fields, and aggregate
metrics. The operator confirms those values before one atomic transaction stores the immutable raw
bytes, provenance, snapshot, metrics, and analysis job.

Byte-identical retries return the original run. Different bytes for the same semantic period are
rejected as a conflict. Two non-overlapping periods compare only when marketplace, report type,
granularity, parser, timezone, currency, and period length match. Aggregate JSON, Markdown, and CSV
exports visibly separate facts, supported derivations, hypotheses, possible measures,
uncertainty, missing evidence, and open questions. ZIP is intentionally unsupported.

## Amazon transport boundary

The pilot transport accepts exactly five operations:

1. Login with Amazon access-token refresh;
2. Reports API `createReport`;
3. Reports API `getReport`;
4. Reports API `getReportDocument`;
5. HTTPS download from the validated presigned Amazon/AWS/CloudFront host.

HTTP method and path are derived from a sealed operation enum. Callers cannot provide arbitrary
Amazon URLs. Redirects are disabled, resource IDs are validated, request IDs are stored only as
short SHA-256 references, and persisted transport errors cannot contain a presigned URL. A
repository-wide contract check fails if a forbidden mutation marker, mutating method, Amazon SDK,
additional Amazon host/header owner, or changed allowlist appears.

The pilot has no client or reachable endpoint for Listings Items, Product Pricing, Orders,
Inventory, Ads, Fulfillment, or mutating Feeds. It can never change prices, ads, listings,
inventory, orders, returns, or fulfillment settings. `createReport` is treated only as acquisition
of an analytical read model.

## Reports, archive, and analysis

The retained registry and fixture parser support several historical report fixtures, but the first
allowed live **network request** is only `GET_SALES_AND_TRAFFIC_REPORT`. SP-API execution
additionally requires:

- administrator role and a manual one-shot request;
- one explicitly approved seller hash, region, and marketplace;
- Brand Analytics role and confirmed marketplace participation;
- a completed UTC period of one to seven days with `DAY`/`CHILD` granularity;
- a shaped server-side secret and an ignored staging-approval file;
- no RDT, buyer/order raw dataset, or scheduler.

Transport and decoded bytes are hashed separately and archived immutably. Parsers are versioned,
use decimals, tolerate unknown fields, and expose missing fields. Snapshot comparison requires the
same report type, marketplace dimension, granularity, period length, and parser version.
Deterministic rule output separates facts, delta, trend, anomalies, hypotheses, possible actions,
uncertainty, missing data, and evidence references. Actions are suggestions only and are never
executed.

The admin banner reads **Essentials+ Merchant - Amazon Intelligence Pilot - Read-only**. The UI
implements upload, preview, confirmation, atomic import, second-period comparison, and three
aggregate export formats. It also shows exact active/disabled modules, redacted seller ID, region,
marketplaces, role status, report/retry/rate-limit state, archive hashes and sizes,
parser/snapshot compatibility, missing data, analyses, and the most recent backup verification. It
never displays tokens, client secrets, refresh tokens, buyer data, or full Amazon payloads.

Synthetic fixtures and the fake SP-API prove repository behavior only. The real staging gate is
currently **BLOCKED** until approved credentials, seller roles, marketplace participation, and
encrypted archive storage are supplied. This does not block manual upload. See
[Amazon staging gate](docs/operations/AMAZON_STAGING_GATE.md). Raw reports and business metrics must
never be committed or copied into a general PR description.

## Retained systems outside the pilot

The Rust Core remains authoritative for the historical ERP model. Vendure 3.7.2 remains a separate
Commerce subsystem with its own PostgreSQL database and asset volume. Durable outboxes, inboxes,
HMAC-authenticated internal requests, replay protection, leases, retries, monotonic projections,
correction invoices, immutable accounting entries, provider-neutral payment/shipping ports, and
synthetic providers remain implemented and tested.

None of those retained mutation paths is part of the pilot. Stripe adapters/webhooks, DHL and DPD
adapters/labels, DATEV activation, additional marketplaces, automated procurement,
multi-tenancy, and Kubernetes are explicitly frozen for this milestone. See
[deferred external gates](docs/DEFERRED_EXTERNAL_GATES.md) and
[deferred capabilities](docs/NICE_TO_HAVE.md).

An optional administrator-triggered OpenAI strategy panel is implemented inside Marketplace
Intelligence. Its single `Analyse` button accepts only a hash-confirmed closed aggregate-history
DTO, carries the last validated handover forward, and permits one successful Europe/Berlin
calendar-week run. It is disabled by default, uses no tools, and cannot mutate Amazon or Merchant.
A separately billed project API key and explicit host activation are required; a ChatGPT
subscription is not an API credential. See
[the strategy AI gate](docs/STRATEGY_AI_GATE.md).

## Verification

Rust checks use the pinned Rust 1.90 toolchain, locked dependencies, SQLx offline metadata, and a
disposable PostgreSQL user able to create test databases:

```bash
cd backend
cargo fmt --all -- --check
cargo audit
SQLX_OFFLINE=true cargo clippy --locked --workspace --all-targets -- -D warnings
DATABASE_URL=postgres://USER:PASSWORD@HOST/DISPOSABLE_DB SQLX_OFFLINE=false cargo sqlx migrate run --source crates/db/migrations
DATABASE_URL=postgres://USER:PASSWORD@HOST/DISPOSABLE_DB SQLX_OFFLINE=false cargo sqlx prepare --workspace --check
DATABASE_URL=postgres://USER:PASSWORD@HOST/DISPOSABLE_DB SQLX_OFFLINE=true cargo test --locked --workspace
```

Frontend and retained Commerce checks:

```bash
npm --prefix frontend ci
npm --prefix frontend run build
npm --prefix frontend run lint
npm --prefix commerce ci
npm --prefix commerce run lint
npm --prefix commerce test
npm --prefix commerce run build
```

Pilot-specific checks include:

```bash
node scripts/check-amazon-operation-allowlist.mjs
node scripts/check-dependency-audit.mjs artifacts/security/dependency-audit.json
node scripts/scan-secrets.mjs
npm --prefix frontend run test:pilot:e2e
node scripts/verify-manual-amazon-import.mjs
ops/test-amazon-pilot-backup-restore.sh
```

The Playwright/axe flow logs in as administrator, verifies the banner and exact module state, runs
both the fake transport and the in-memory two-period manual UI workflow through analysis/export,
probes disabled mutation routes, and rejects serious/critical accessibility findings. It does not
start or test the Storefront. Existing clean
vertical, failure/recovery, full backup/restore, and upgrade rehearsals remain required and are not
weakened. See the [verification matrix](docs/VERIFICATION_MATRIX.md) and
[failure matrix](docs/FAILURE_MATRIX.md).

## Pilot backup and restore

The pilot backup contains the Core schema, allowlisted module/audit records, immutable Amazon raw
archives, normalized snapshots, analyses, parser versions, transport hashes, document subtree,
Git commit, and image digests. It explicitly excludes LWA refresh tokens, client/access secrets,
buyer data, and all retained Commerce/payment/shipping business tables.

Restore refuses any non-empty target project. The synthetic rehearsal uses a raw report larger
than 2 MiB and compares report inventory, raw hashes, parser versions, snapshots, analysis,
module/audit state, schedule state, and the read-only profile after an empty-project restore.

## Supply chain and advisories

GitHub Actions are pinned to full commit SHAs, runner/toolchain versions are fixed, container images
use immutable digests, and the Typst release archive is SHA-256 verified. Pilot and retained
Commerce CycloneDX SBOMs plus a redacted dependency report are committed under `docs/security/`;
CI regenerates and checks them without automatic force fixes.

The current retained Commerce tree has 12 production package findings (six high and six moderate,
zero critical) representing 11 distinct GHSAs, all reachable only through Vendure dependencies.
Vendure and Storefront are not started by the pilot, which reduces exposure but does not remediate
those advisories. Vendure remains pinned consistently at 3.7.2 because npm's suggested automatic
change is incompatible. Every finding, code-path assessment, control, upstream status, decision,
and review date is recorded in [docs/security/VENDURE_ADVISORIES.md](docs/security/VENDURE_ADVISORIES.md).

## Next safe external action

Use the manual Sales & Traffic import immediately. If an authorized operator later supplies the
ignored SP-API secret and approval files, run the staging gate in validation mode. Only after every
local and authorization check passes may that operator invoke one explicit network request. No
scheduler or write integration is enabled.
