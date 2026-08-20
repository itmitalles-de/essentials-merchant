# Architecture

Essentials+ Merchant now has a narrow default milestone topology and a retained full-stack test
topology. Internal `erplite`, `shop-suite-*`, crate, database, volume, migration, token-storage,
and mapping identifiers are compatibility contracts, not presentation branding.

## Amazon pilot topology

```text
React Admin
    |
Rust/Axum Core -- exact module + HTTP mutation guard
    |                       |
Core PostgreSQL             +-- LWA / Amazon Reports API v2021-06-30
    |                               (five sealed operations only)
immutable pilot archive
```

`compose.amazon-pilot.yml` defines the review/test project and `compose.mantle-amazon.yml` defines
the Mantle live project `essentials-merchant-amazon`. Both contain exactly `db`, `backend`, and
`frontend`; neither defines Vendure, Storefront, payment, shipping, carrier, DATEV, or external-AI
services. The live frontend binds to loopback for a private Caddy route and backend/frontend images
are tagged with the exact deployed Git SHA. Startup atomically applies `amazon-read-only` and
verifies the exact active set:

- `core.operations`, `core.catalog`, `core.inventory`, `core.orders`;
- `marketplace.amazon_intelligence`, `intelligence.rules`;
- `pilot.amazon_read_only`.

Every other module must be inactive. Future required modules are not exempt. Schedules are off,
queued live runs are held, and every unsafe HTTP method is blocked except connection configuration,
fixture/manual report acquisition, and deterministic analysis. Raw archive downloads and stateful
connector-health GETs are blocked as well. Module guards remain defense in depth beneath that
global policy.

## Amazon acquisition and data boundary

The default acquisition path is a manual official Sales and Traffic upload. A side-effect-free
JSON/CSV/TSV parser validates at most 10 MiB, computes the raw hash, rejects PII-like schemas, and
returns confirmable provenance. Only a confirmation-complete preview crosses one PostgreSQL
transaction containing raw archive, immutable receipt, normalized snapshot/metrics, and analysis
job. Raw-hash and semantic-period advisory locks make retries idempotent and reject ambiguous
duplicate periods. The existing deterministic analysis/export pipeline is reused; there is no
separate Mantle analysis service.

Connections persist the seller context needed for Amazon requests, region, marketplace IDs, roles,
mode, and only a logical environment secret reference. API summaries redact the seller ID before
serialization. Credential values are parsed server-side and never enter the database, UI, logs,
archive, backup, or diagnostics.

The live request builder accepts an `AmazonOperation`, not a free method/path. Its complete set is
LWA refresh, Reports `createReport`, `getReport`, `getReportDocument`, and a redirect-disabled HTTPS
download from an Amazon/AWS/CloudFront host. Resource IDs are constrained, request IDs are hashed,
and persisted network errors are static. There are no compiled Amazon SDKs or clients for Listings,
Pricing, Orders, Inventory, Ads, Fulfillment, or mutating Feeds.

The first live report gate permits only an administrator-initiated
`GET_SALES_AND_TRAFFIC_REPORT` after a scoped seller-hash/region/marketplace approval. It requires
one completed one-to-seven-day UTC period, `DAY`/`CHILD`, the Brand Analytics role, no RDT, and no
scheduler. A second snapshot must match report, marketplace, granularity, period length, and parser
version and is blocked until a first real success exists.

Transport bytes and decoded bytes have separate hashes and immutable storage. Versioned parsers
produce normalized decimals and explicit missing fields. Deterministic analysis persists facts,
delta, trend, anomalies, hypotheses, possible actions, uncertainty, missing data, and evidence.
Aggregate JSON, Markdown, and CSV exports recursively deny
buyer/customer/address/email/order/comment/phone fields.
Actions are never executed. An optional, manually triggered OpenAI adapter can receive only a
second closed aggregate DTO after hash confirmation. It has a fixed Responses API POST, no tools,
no Amazon transport authority, no automatic execution, and no raw/product/customer input.

Validated strategy output is stored separately in immutable
`amazon_ai_strategy_assessments`, uniquely keyed by deterministic analysis, aggregate hash, model,
and prompt version. Prompts, raw provider responses, API credentials, archive hashes, and internal
evidence UUIDs are not stored there. The browser renders this output outside the canonical facts
and deterministic-derivation blocks.

## Retained full-stack topology

```text
React Admin -> Rust/Axum Core -> Core PostgreSQL + document volume
                    |   ^
       Core outbox  |   | signed Vendure payment/order events
                    v   |
             Vendure worker -> Vendure PostgreSQL + asset volume
                    ^
Next.js Storefront -> Vendure Shop API
```

Core owns ERP master data, available inventory, imported orders, immutable invoices/accounting,
modules, audit, and Marketplace Intelligence. Vendure owns retained merchandising, cart/checkout,
payment/fulfillment runtime, and Shop/Admin APIs. They share no database or transaction. Durable
outbox/inbox delivery, HMAC with nonce replay protection, leases/retries/dead state, stable mappings,
idempotent consumers, and monotonic product sequences remain covered by failure tests.

Issued invoices and corrections remain immutable Decimal snapshots. DATEV rendering reads only
immutable entries but `export.datev` stays disabled pending a future external acceptance. Payment
and shipping ports/fakes remain tests, not provider adapters or production claims.

## Admin and diagnostics

The pilot UI exposes exact module compliance, disabled mutations, redacted Amazon connection,
roles, marketplace, report/poll/retry/rate-limit status, archive/hash/parser/snapshot state,
missing data, deterministic analysis, and latest backup verification. It never exposes secret
references/values, seller IDs in full, buyer data, or report payloads. Raw archive download and
scheduler controls are unavailable in the pilot.

## Backup and restore boundaries

The retained coordinated backup still captures both databases and both file stores for full-stack
recovery tests. The smaller pilot backup exports Core schema plus an explicit data-table allowlist,
the `amazon-pilot` document subtree, Git revision, parser versions, hashes, and declared image
digests. It explicitly excludes credential values, buyer data, ERP business tables, and all
Vendure/Storefront/payment/shipping stores.

Both restore paths verify manifests/checksums and refuse an existing target. The pilot rehearsal
restores a greater-than-2-MiB synthetic report into an empty project and compares archive,
snapshot, parser, analysis, module, audit, schedule, and read-only status. These are repository
proofs, not external encrypted-retention or RPO/RTO acceptance.

## Supply-chain and testing constraints

- Rust 1.90, Node 22.22.0, SQLx 0.8.6, dependency locks, Actions SHAs, image digests, and Typst hash
  are fixed; no `latest` images or automatic force-fixes.
- SQLx metadata/migrations use only disposable PostgreSQL; Vendure `synchronize` remains false.
- Vendure 3.7.2 is absent from the pilot runtime but retained advisories are not called fixed.
- Pilot Playwright/axe, Amazon allowlist, audit/SBOM/secret checks, Rust/frontend/Commerce suites,
  recovery, full/pilot restores, and upgrade rehearsal are independent acceptance layers.
- Multi-tenancy, Kubernetes, other marketplaces, marketplace writes, automated procurement, and
  live transactional provider adapters remain outside this architecture milestone. The optional
  OpenAI strategy gate is interpretation-only and disabled without a separately approved API key.
