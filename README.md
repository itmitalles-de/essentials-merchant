# Essentials+ Merchant

Essentials+ Merchant is currently scoped to one internal pilot: **read-only Amazon Marketplace
Intelligence**. It acquires approved Amazon Reports data, preserves immutable raw evidence, creates
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

Vendure, Storefront, payment, shipping, carrier, and DATEV services are absent from
`compose.amazon-pilot.yml`. Their code, databases, compatibility identifiers, ports, fakes, and
tests remain available to the retained full-stack test topology.

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
allowed live request is only `GET_SALES_AND_TRAFFIC_REPORT`. Live execution additionally requires:

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
shows exact active/disabled modules, redacted seller ID, region, marketplaces, role status, recent
report/retry/rate-limit state, archive hashes and sizes, parser/snapshot compatibility, missing
data, analyses, and the most recent backup verification. It never displays tokens, client secrets,
refresh tokens, buyer data, or full Amazon payloads.

Synthetic fixtures and the fake SP-API prove repository behavior only. The real staging gate is
currently **BLOCKED** until approved credentials, seller roles, marketplace participation, and
encrypted archive storage are supplied. See
[Amazon staging gate](docs/operations/AMAZON_STAGING_GATE.md). Raw reports and business metrics must
never be committed or copied into a general PR description.

## Retained systems outside the pilot

The Rust Core remains authoritative for the historical ERP model. Vendure 3.7.2 remains a separate
Commerce subsystem with its own PostgreSQL database and asset volume. Durable outboxes, inboxes,
HMAC-authenticated internal requests, replay protection, leases, retries, monotonic projections,
correction invoices, immutable accounting entries, provider-neutral payment/shipping ports, and
synthetic providers remain implemented and tested.

None of those retained mutation paths is part of the pilot. Stripe adapters/webhooks, DHL and DPD
adapters/labels, DATEV activation, additional marketplaces, external AI, automated procurement,
multi-tenancy, and Kubernetes are explicitly frozen for this milestone. See
[deferred external gates](docs/DEFERRED_EXTERNAL_GATES.md) and
[deferred capabilities](docs/NICE_TO_HAVE.md).

## Verification

Rust checks use the pinned Rust 1.90 toolchain, locked dependencies, SQLx offline metadata, and a
disposable PostgreSQL user able to create test databases:

```bash
cd backend
cargo fmt --all -- --check
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
ops/test-amazon-pilot-backup-restore.sh
```

The Playwright/axe flow logs in as administrator, verifies the banner and exact module state, runs a
fake report through polling/snapshot/analysis/export, probes disabled mutation routes, and rejects
serious/critical accessibility findings. It does not start or test the Storefront. Existing clean
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

After an authorized operator supplies the ignored secret and approval files, run the staging gate
in validation mode. Only after every local and authorization check passes may that operator invoke
one explicit manual Sales & Traffic request. A second compatible snapshot is permitted only after
the first real job succeeds; no scheduler or write integration is enabled.
