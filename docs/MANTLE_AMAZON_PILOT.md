# Mantle Amazon analysis pilot

## Purpose and boundary

This pilot gives Mantle an internal, read-only path from official Amazon Sales
and Traffic and aggregate Sponsored Products campaign reports to
evidence-linked comparisons. Both manual paths work without provider
credentials. SP-API remains an optional acquisition channel for Sales and
Traffic through the same archive, parser, metric, analysis, and export pipeline;
Amazon Ads API access is a separate future gate.

The service never changes prices, advertising, listings, inventory, orders,
payments, shipping, or tax/accounting data. The production profile starts only
PostgreSQL, the Merchant backend, and the Core frontend. Vendure, Storefront,
payment, shipping, and DATEV are outside the deployment. External AI uses the
existing backend only, remains unavailable without a separately billed key, and
has no mutation capability. Its only tool is a bounded public web-search step
that never receives internal Amazon evidence.

## Current Mantle deployment

The accepted live revision is `7c7f7dafc69be789ed7cfb95e8f30207ca410015`
in Compose project `essentials-merchant-amazon` on `192.168.178.15`. Internal
operators use `https://ai-marketing.mantle-climbing.de`; the retained fallback is
`https://merchant.mantle-climbing.de/ai-marketing`. Both names resolve internally
to the Docker host. Caddy accepts only private, loopback, or VPN source ranges.
The frontend has no public host bind and there is no public registration path.
The Mantle dashboard links directly to the canonical AI hostname.

The canonical hostname has no login form. Its frontend requests a short-lived,
same-origin `mantle-amazon-read-only` session and exposes only the AI-first
Amazon route. The regular login endpoint is disabled in this profile. Anyone
inside the allowed LAN/VPN boundary can run the weekly analysis or replace
write-only credentials, so the Caddy/source-network restriction is mandatory.

The live acceptance used only visibly synthetic, in-memory reports. Sales and
Traffic JSON/CSV/TSV plus two aggregate Sponsored Products campaign periods,
retry idempotence, two-period comparisons, all summary formats, search-term and
identifier rejection, and business-mutation blocking passed. Raw bytes were
sent directly to the upload endpoints and were not written to host files. No
authorized real report has been imported. The reserved `SYNTHETIC-` marketplace
namespace is excluded before weekly provider-context construction, so these
retained acceptance analyses can never become evidence in a real AI run.

The weekly AI mini-tool renders one `Analyse` button, the fixed KPI/public
context/strategy/handover structure, and a rising-market icon and favicon. The
successful-run gate is enforced by a Monday-based database unique index in
`Europe/Berlin`, not only by the button. Public web research is a separate,
public-only request; its bounded citations are then combined with aggregate
Amazon history and the last validated handover in a tool-free synthesis. No
separately billed OpenAI key is provisioned, so the live strategy status is
fail-closed with `api_key_missing` and no provider request has succeeded.

The live image IDs are:

- PostgreSQL: `sha256:75f5a96988cdf694a215073c3e9c001b706b371e2f94df3967f2efdec2787f6b`
- backend: `sha256:ad6594b44fff4e7bca650b64c929e295cc8d9b600b1b7e3cb501edddacf290ff`
- frontend: `sha256:c6142da17061cde07af52dfc80af7acd58b80d5b8f0e0fc929b7059576f645bc`

Exact-head CI run `32363658273` passed all seven jobs for runtime parent
`9c6f8ba30c829c42255f33311fcd838e9f761049`. Live revision `608691b` adds only
the exact two-report backup allowlist, its recovery fixture, security assertion,
and documentation. Its Sales+Ads backup/empty-target restore passed locally.
Live safeguard `7c7f7da` excludes `SYNTHETIC-` analyses from weekly provider
context; all 21 DB tests, DB Clippy with warnings denied, and the workspace
all-target check passed locally. The deployed endpoint sees all six retained
acceptance analyses but returns `source_analysis_count: 0` and
`no_analysis_data`.
GitHub did not start any job for run `32365245827`: every job was rejected before
checkout because recent account payments failed or the Actions spending limit
must be increased. This is an external CI-account gate, not a test failure; it
must be cleared before PR #5 can leave draft.

The deployment preserved the PostgreSQL container and all 26 non-target
container identities, images, restart counts, and start times; their exact
baseline SHA-256 remained
`becf4957d74030debf19035fce8bf2489a102d9c1f6b9eba8cabb1eb83b85212`.
Caddy was not changed or reloaded. Live Chromium reached `/ai-marketing` with
zero login headings/buttons, exactly one `Analyse` button, the branded favicon,
and the Ads import. Schema 20 is active and automatic schedules remain zero.
The frontend access-log format deliberately omits query parameters; the final
synthetic run produced zero filename, sentinel, query, secret, or fatal log
matches.

The final live backup is
`/opt/essentials-merchant-amazon-backups/live-market-context-608691b-20260820T1146Z`.
It was created with mode `0700`, verified six checksummed artifacts, registered
manifest SHA-256
`58eb2cd3375624159e717a08ca7fecb68b6a5a46e488d0d2ead0a99caa716bc0`
against live revision `608691b`, and excludes all provider-secret rows. The
isolated local empty-target restore matched both Sales and Ads raw hashes,
snapshots and parser versions, retained the AI handover chain, restored zero
provider secrets and zero schedules, and removed its disposable containers and
volumes afterward. No additional restore was run against production.

## Relationship to the Mantle wiki toolchain

The full `mantle-climbing-de/wiki/amazon/marketing` tree was reviewed before
implementation. Its useful ideas were integrated into the existing Merchant
Marketplace Intelligence boundary; no third runtime analysis system was added.

| Classification | Treatment |
| --- | --- |
| Reusable | Header aliases, locale-aware decimals, canonical SHA-256 metadata, and comparison test cases informed the Merchant parser. |
| Extractable | Pure CSV/TSV normalization and deterministic comparison rules were adapted behind the Merchant API. |
| Historical documentation | Existing case studies, strategy documents, and generated reviews remain in the wiki. |
| Redundant | The wiki CLI, local cache/snapshot storage, and static report writer were not copied. |
| Reused in this extension | Aggregate Ads KPIs and competitor/category/global-context questions are implemented through the existing Merchant archive and analysis boundary. |
| Valuable but not migrated | Inventory, profitability, portfolio, product-level Ads, and historical business-specific strategy remain later evidence sources. |
| Real or identifying data | Product mappings, experiments, ASIN/SKU fixtures, names, and historical business metrics were not copied. |

## Runtime flow

1. An internal user with the scoped pilot session uploads JSON, CSV, or TSV
   bytes for preview.
2. The backend enforces the byte limit, rejects PII-like columns, identifies the
   format, computes SHA-256, and validates either the complete Sales and Traffic
   schema or the aggregate Sponsored Products campaign schema. Ads search-term,
   keyword, targeting, ASIN, SKU, and product dimensions are rejected.
3. The user confirms marketplace, period, granularity, report type, source
   timezone and currency, plus the Ads attribution window when applicable.
   Confirmation must match parsed source values; it cannot rewrite them.
4. One database transaction archives the exact bytes, provenance, normalized
   snapshot, metrics, and analysis job. A parser failure stores none of them.
5. A retry of identical bytes resolves to the original run.
6. The deterministic rules engine produces facts and, when a compatible earlier
   period exists, absolute and percentage deltas, trends, outliers, supported
   derivations, hypotheses, possible measures, uncertainty, missing evidence,
   and open questions.
7. JSON, Markdown, and CSV exports contain only allowlisted aggregates.

Compatibility requires the same marketplace, report type, date granularity,
parser version, currency, source timezone, and period length. Periods must not
overlap, and the predecessor is selected by report period rather than import
time.

## Operator workflow

The Marketplace Intelligence page implements the following workflow:

1. Upload report.
2. Review format, hash, and report type.
3. Confirm period.
4. Confirm marketplace and source timezone.
5. Review normalized metrics, missing fields, and warnings.
6. Execute the atomic import.
7. Review the analysis.
8. Upload a second compatible period.
9. Review the deterministic comparison.
10. Export an aggregate JSON, Markdown, or CSV summary.
11. Optionally upload one or two aggregate Sponsored Products campaign periods
    and review the identifier-free Ads KPI comparison.
12. Click the single `Analyse` button. If approved Amazon credentials exist, it
    first obtains exactly one seven-day Sales and Traffic report; otherwise it
    uses the manual imports. It then uses every eligible bounded aggregate
    analysis plus the last validated AI handover, renders the fixed strategy
    structure below the deterministic analysis, and is disabled after one
    successful Europe/Berlin calendar-week run. It first researches public
    competitor, category/market, and global trend/crisis evidence without
    internal metrics, then synthesizes that bounded research with the aggregate
    history in a separate tool-free request.

Raw report downloads are blocked by the Amazon read-only pilot middleware, even
for administrators. The raw bytes are available only to the database backup and
restore path.

## Supported manual reports

- Amazon report type: `GET_SALES_AND_TRAFFIC_REPORT`
- Manual formats: JSON, CSV, TSV
- Required facts: ordered product sales, ordered units, sessions, page views
- Conditional facts: unit session percentage/conversion, Buy Box percentage,
  B2B sales, B2B units, and B2B share
- Parser: `manual-sales-traffic-v1`

- Internal report type:
  `AMAZON_ADS_SPONSORED_PRODUCTS_CAMPAIGN_REPORT`
- Source: official aggregate Sponsored Products campaign report
- Manual formats: JSON, CSV, TSV
- Required facts: impressions, clicks, spend
- Conditional facts: attributed sales, orders, units
- Derived facts: CTR, CPC, ROAS, ACOS
- Required comparison metadata: marketplace, period, currency, timezone, and
  7/14/30-day attribution window
- Parser: `manual-ads-sp-campaign-v1`

ZIP is intentionally not accepted in the first production path. This avoids an
unnecessary decompression and archive-member attack surface.

## External gate

The manual workflow is production-capable without Amazon secrets. The SP-API
gate is documented in [SP_API_GATE.md](SP_API_GATE.md) and stays externally
blocked until explicitly approved credentials and a one-shot staging gate are
available.

Generative strategy synthesis is implemented behind a separate external gate.
The rules engine remains the source of facts and supported derivations. The
OpenAI adapter requires a separately funded, project-scoped API key entered
through the write-only internal GUI and receives only a stricter
aggregate-history DTO after the `Analyse` click
confirms the displayed hash. A separate web request sees only a fixed public
Mantle category brief; the synthesis request has no tools. Neither can receive
raw reports or product/customer/campaign identifiers, run automatically, or
gain a mutation tool. Validated model output is immutable, limited to one
successful Mantle calendar-week row, includes fixed competitor, category,
global crisis, uncertainty, source, and next-run handover sections, and remains
visibly separate from facts and deterministic derivations. Full activation and
data-control details are in
[STRATEGY_AI_GATE.md](STRATEGY_AI_GATE.md).

Until a real key is entered, strategy status is
`api_key_missing` (shown in the UI as an external pay-per-use gate). Manual report import,
deterministic analysis, comparison, and export remain available; no fake key or
provider success may be claimed.
