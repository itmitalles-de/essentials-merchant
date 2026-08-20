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

The checked-out live revision is
`77a2608f222bdc099d696518784cc95052fc9b33` in Compose project
`essentials-merchant-amazon` on `192.168.178.15`. The running backend was built
from `04808832d4a88982a750df9636f488662eb253f6`; the running frontend was built
from `77a2608f222bdc099d696518784cc95052fc9b33`. Parent revision
`d53581fe5a00b1c39b4b923ac65264af80251938` changes only the pinned Simple
Business UI contract and is not a runtime deployment. Internal operators use
`https://ai-marketing.mantle-climbing.de`; the retained fallback is
`https://merchant.mantle-climbing.de/ai-marketing`. Both names resolve
internally to the Docker host. This hostname has its own Caddy source matcher
for the local `192.168.178.0/24` LAN and private `10.0.0.0/8` or
`100.64.0.0/10` VPN sources. Other Mantle hostnames retain their narrower
device allowlist. The frontend has no public host bind and there is no public
registration path. The Mantle dashboard links directly to the canonical AI
hostname.

The canonical hostname has no login form. Its frontend requests a short-lived,
same-origin `mantle-amazon-read-only` session and exposes only the AI-first
Amazon route. The regular login endpoint is disabled in this profile. Anyone
inside the allowed LAN/VPN boundary can run the weekly analysis or replace
write-only credentials, so the Caddy/source-network restriction is mandatory.

Live acceptance first used visibly synthetic, in-memory reports. Sales and
Traffic JSON/CSV/TSV plus two aggregate Sponsored Products campaign periods,
retry idempotence, two-period comparisons, all summary formats, search-term and
identifier rejection, and business-mutation blocking passed. Raw bytes were
sent directly to the upload endpoints and were not written to host files. The
reserved `SYNTHETIC-` marketplace namespace is excluded before weekly
provider-context construction. A later authorized real Sales and Traffic run
completed through the same archive/parser/analysis boundary; neither its raw
path nor business metrics were written to Git or application logs.

The weekly AI mini-tool renders one `Analyse` button before result history, the
fixed KPI/public-context/strategy/handover structure, and a rising-market icon
and favicon. The Mantle route suppresses retained synthetic acceptance cards,
provides a light/dark switch, and shows five truthful activity phases: Amazon
report, validation and KPIs, market and competition, global crises, and
strategy/handover. No simulated percentages are displayed. The successful-run
gate is enforced by a Monday-based database unique index in `Europe/Berlin`,
not only by the button. Public web research is a separate, public-only request;
its bounded citations are then combined with aggregate Amazon history and the
last validated handover in a tool-free synthesis.

Before any paid run, a one-time curated business baseline is built from
approved Mantle Wiki and Notes sources. Only typed business statements, source
paths/titles, statuses, and file hashes are imported; raw Markdown, personal
notes, PII, and secrets stay outside the service. The database row is immutable
and an identical retry is idempotent. Every weekly synthesis receives this
baseline plus the latest validated handover, allowing continuity to improve
without silently rewriting the source material.

Below the action, a terminal-style activity view reports only observable,
sanitized phases such as Amazon acquisition, archive/parser validation,
aggregate preparation, business-context loading, public research, synthesis,
source validation, token counts, and result persistence. It does not expose
credentials, raw reports, signed URLs, provider request bodies, or hidden model
reasoning. The visible `KI-Begründungszusammenfassung` is the validated
user-facing rationale from the structured response.

The analysis route itself contains no credential forms. A gear beside the
light/dark switch opens `/ai-marketing/settings`, which owns provider setup and
the read-only system boundary. The analysis action appears before the pipeline;
hash, model, storage, and other technical metadata are collapsed by default.

Both provider credential sets are present only in the encrypted write-only
store. The first attempt exposed a missing dot in the report-document ID
allowlist; `f984cd3` fixed that exact boundary while retaining the path and
download-host restrictions. The successful post-fix operator run then completed
LWA refresh, report creation, polling/backoff, download, immutable archive,
parsing, deterministic analysis, public research, structured AI synthesis, and
handover validation. Repeating the action returned the cached weekly result and
made no second provider call.

The live image IDs are:

- PostgreSQL: `sha256:75f5a96988cdf694a215073c3e9c001b706b371e2f94df3967f2efdec2787f6b`
- backend: `sha256:36c34249b6833b7aa0401e4bd007c462601bd4f9b9db9ca4c826533755a03caf`
- frontend: `sha256:10871780498e12fb16f90e8359a22439d07112a6c6040b60eff688a1955074c2`

The final provider path passed Rust check and Clippy with warnings denied,
strategy tests 8/8, an isolated DB suite 65/65, frontend build/lint, focused
Chromium/axe E2E 3/3, Nginx validation, the Amazon-operation allowlist, secret
scanning, and the synthetic JSON/CSV/TSV import/comparison/export flow. The
successful live row uses model `gpt-5.6`, prompt
`mantle-amazon-weekly-strategy-v4`, 15 bounded public sources, and the immutable
13-source/30-entry Mantle/Sphagnum baseline. It was stored once for week
`2026-08-17`; the next eligible action is 2026-08-24 00:00 Europe/Berlin.

The three operator-visible failures were resolved at their causes: exact weekly
Nginx timeout (`fff8ede`), bounded provider timeout (`c35b5e6`), and invalid or
truncated structured evidence output (`0480883`). Frontend revision `77a2608`
also restores the sanitized terminal activity context after a page reload.

GitHub did not start any job for recent run `32388594705`: every job was
rejected before checkout because recent account payments failed or the Actions
spending limit must be increased. This is an external CI-account gate, not a
test failure; it must be cleared and the final exact head rerun before PR #5 can
leave draft.

All deployments and the final backup changed only this Compose project's
application containers. PostgreSQL and Caddy retained their container IDs and
zero restart counts. All 26 non-target running containers retained their exact
identity, image, restart count, and start time; the before/after baseline hash
is `fe6c775817727763a60b1b3a6608adc2f17c2ea910abd10176873b0cac6391a9`.
Live Chromium reached `/ai-marketing` with the no-login shell, exactly one
weekly `Analyse` button, the expected post-success lock, branded favicon,
working light/dark themes, fixed output sections, public/global context,
validated rationale and handover, the sanitized persistent activity log, and
zero disclosed secret values. Schema 21 is active, automatic schedules remain
zero, and the final application-log scan found zero sensitive markers.

The final live backup is
`/opt/essentials-merchant-amazon-backups/live-ai-context-77a2608-20260820T1540Z`.
It has mode `0700` and manifest SHA-256
`f1a06fa3cb5d6f070ce0ca90f0a2c2457962c4267b5333014ec0f3c7d3c15e4d`.
The exact schema-21 allowlist includes the immutable curated baseline and the
validated AI assessment but excludes all provider-secret rows. An isolated
empty-target restore retained the Sales and Ads evidence, parser versions,
business baseline, and handover chain while restoring zero provider secrets and
zero schedules. Production itself was not used as a restore target.

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
    history, immutable Mantle/Sphagnum baseline, and last validated handover in
    a separate tool-free request.

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

The manual workflow remains production-capable without Amazon secrets. The
SP-API boundary is documented in [SP_API_GATE.md](SP_API_GATE.md). Approved
credentials are configured and one authorized seven-day Sales and Traffic
request completed through document download, parsing, deterministic analysis,
and the weekly strategy flow. There is no remaining Amazon gate for this
read-only report path.

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

The OpenAI key is configured in the write-only store and one paid structured
assessment completed successfully. The stored weekly row and handover were
validated, and a repeated request returned the cache without a second provider
call. Remaining provider administration is external: confirm the dedicated
project's intended budget/limits and applicable data-control/retention policy.
`store:false` prevents Responses application-state storage but is not by itself
a zero-retention claim. Manual report import, deterministic analysis,
comparison, and export remain available independently of either provider.
