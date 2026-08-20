# Manual Amazon report import

## Accepted input

The manual boundary accepts two official aggregate Amazon report families as
JSON, CSV, or TSV. The maximum request size is 10 MiB. ZIP, XLSX, PDFs,
screenshots, and order-level exports are rejected.

JSON must contain `reportSpecification` with
`GET_SALES_AND_TRAFFIC_REPORT`, one marketplace, supported date granularity,
and a valid period. It must contain Sales and Traffic rows by date or ASIN.

CSV and TSV use a small, explicit alias set for Amazon headings. UTF-8 BOM is
accepted. Decimal commas, decimal points, thousands separators, and accounting
parentheses are normalized only when the representation is unambiguous. Dates
such as `01/02/2026` are rejected instead of guessing a locale. Unknown columns
are ignored only after PII-like customer, buyer, order, address, e-mail, and
phone headings have been rejected.

The Ads path accepts only an aggregate Sponsored Products campaign report. It
requires impressions, clicks, and spend; attributed sales, orders, and units
are optional. A 7-, 14-, or 30-day attribution window must be present in the
official field names or confirmed by the operator. Rows must share one report
period, one currency, and one marketplace. Mixed or ambiguous attribution
windows fail closed. Every row must also carry a campaign-name or campaign-ID
dimension proving campaign-level report shape; its value is checked for
presence and then discarded rather than normalized.

Search-term, keyword, targeting-expression, advertised/purchased ASIN, and SKU
dimensions are rejected. Campaign names, campaign IDs, portfolio fields, and
all other unknown row values are excluded from normalization. They can exist
only inside the immutable confidential raw archive and never enter analysis,
summary export, or OpenAI input. The Ads parser is
`manual-ads-sp-campaign-v1`; it does not call the Amazon Ads API and exposes no
campaign mutation.

## Preview and confirmation

Preview does not write to the database. It returns:

- SHA-256 and detected format;
- report type and parser version;
- period, marketplace, and granularity;
- source timezone and currency;
- Ads attribution window when applicable;
- data freshness;
- aggregate metric preview;
- missing optional fields and warnings.

The execute request repeats the exact bytes and confirms hash, report type,
marketplace, period, granularity, and, for Ads, attribution window. Any mismatch
returns a visible validation error. This prevents the UI or an API caller from
relabelling parsed data.

Amazon date-only exports do not prove an instant-level timezone. The operator
must confirm the business timezone; the stored snapshot and comparability key
retain it. The production default is `Europe/Berlin`, but it is never inferred
as an Amazon-provided fact.

## Archive, idempotence, and failure behavior

The SHA-256 is recomputed at both parsing and storage boundaries. The exact
bytes are stored in `amazon_report_documents`; database triggers prevent update
or deletion. `amazon_manual_report_imports` records non-PII provenance and is
also immutable.

An advisory transaction lock and a unique raw SHA-256 serialize concurrent
retries. Re-uploading the same bytes returns `already_imported` and the original
run ID. It does not create a second archive, snapshot, metric set, or analysis.
A different byte stream for an already archived marketplace, report type,
period, granularity, parser, currency, and timezone is rejected as a visible
semantic duplicate instead of creating an ambiguous second snapshot.

Parsing completes before persistence. Archive, provenance, snapshot, all
metrics, and the analysis job are then committed in one PostgreSQL transaction.
Any database error rolls the whole transaction back. Parser errors are returned
to the operator and do not create a partial run.

## Metrics and comparisons

Catalog aggregates are stored with decimal arithmetic:

- ordered product sales and ISO 4217 currency;
- ordered units;
- sessions;
- page views;
- conversion or unit session percentage;
- Buy Box percentage when present;
- B2B sales, units, and derived share when present.

A comparison is allowed only for equal report type, marketplace, granularity,
parser version, source timezone, currency, and period length. The periods must
be non-overlapping and chronologically ordered.

The Sponsored Products campaign path stores only account-level aggregate
evidence:

- impressions, clicks, spend;
- attributed sales, orders, and units when present;
- derived CTR, CPC, ROAS, and ACOS using exact decimal arithmetic;
- the confirmed attribution window and explicit missing fields.

Ads periods additionally require equal attribution window through their
comparability key. Attributed Ads sales are not relabelled as total Amazon
sales, and the analysis treats Ads movement as correlation rather than causal
proof.

## Summary exports

JSON, Markdown, and CSV exports are generated from an allowlist of aggregate
metrics and analysis fields. They include facts, deterministic derivations,
hypotheses, possible measures, uncertainty, missing evidence, and open
questions. They never contain raw bytes, rows, buyer/order fields, or product
identifiers.
