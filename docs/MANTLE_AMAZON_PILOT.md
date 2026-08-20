# Mantle Amazon analysis pilot

## Purpose and boundary

This pilot gives Mantle an internal, read-only path from an official Amazon
Sales and Traffic report to an evidence-linked comparison. It works without
SP-API credentials. SP-API remains an optional acquisition channel for the same
archive, parser, metric, analysis, and export pipeline.

The service never changes prices, advertising, listings, inventory, orders,
payments, shipping, or tax/accounting data. The production profile starts only
PostgreSQL, the Merchant backend, and the Core frontend. Vendure, Storefront,
payment, shipping, DATEV, and external AI are outside the deployment.

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
| Valuable but not part of the first path | Ads, inventory, profitability, portfolio, and competitor analysis remain later evidence sources. |
| Real or identifying data | Product mappings, experiments, ASIN/SKU fixtures, names, and historical business metrics were not copied. |

## Runtime flow

1. An authenticated internal user uploads JSON, CSV, or TSV bytes for preview.
2. The backend enforces the byte limit, rejects PII-like columns, identifies the
   format, computes SHA-256, and validates the complete Sales and Traffic schema.
3. The user confirms marketplace, period, granularity, report type, and source
   timezone. Confirmation must match the parsed report; it cannot rewrite it.
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

Raw report downloads are blocked by the Amazon read-only pilot middleware, even
for administrators. The raw bytes are available only to the database backup and
restore path.

## First supported report

- Amazon report type: `GET_SALES_AND_TRAFFIC_REPORT`
- Manual formats: JSON, CSV, TSV
- Required facts: ordered product sales, ordered units, sessions, page views
- Conditional facts: unit session percentage/conversion, Buy Box percentage,
  B2B sales, B2B units, and B2B share
- Parser: `manual-sales-traffic-v1`

ZIP is intentionally not accepted in the first production path. This avoids an
unnecessary decompression and archive-member attack surface.

## External gate

The manual workflow is production-capable without Amazon secrets. The SP-API
gate is documented in [SP_API_GATE.md](SP_API_GATE.md) and stays externally
blocked until explicitly approved credentials and a one-shot staging gate are
available.
