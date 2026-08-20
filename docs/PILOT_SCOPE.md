# Amazon Intelligence pilot scope

## Fixed objective

The first pilot is an internal, read-only Amazon Marketplace Intelligence system. Its exact UI title is:

> Essentials+ Merchant - Amazon Intelligence Pilot - Read-only

The pilot acquires Amazon Reports data, preserves immutable source evidence, parses it with versioned deterministic code, creates comparable snapshots and deterministic rule analyses, and exports PII-minimized aggregates. Suggested actions remain text; no action executor exists in the pilot.

## Persisted module profile

The profile uses the existing `essentials_modules` registry. `ESSENTIALS_MODULE_PROFILE=amazon-read-only` applies one atomic persisted profile; it is a deployment selector, not a parallel feature-flag system. Startup fails if the resulting state is not the exact allowlist or if any automatic Amazon schedule remains enabled.

Active modules:

- `core.operations`
- `core.catalog`
- `core.inventory`
- `core.orders`
- `marketplace.amazon_intelligence`
- `intelligence.rules`
- `pilot.amazon_read_only`

The Core modules remain enabled because their existing schema, authorization, audit, and domain services underpin the application. While the pilot module is active, a global server middleware rejects every unsafe HTTP method unless it is one of the narrowly listed report-acquisition or deterministic-analysis endpoints. Core business POST/PUT/DELETE routes therefore remain compiled and tested for the retained ERP but are not writable in the pilot.

Explicitly inactive/not installed:

- `commerce.vendure`, `commerce.storefront`
- `payment.test` and every real payment connector
- `shipping.manual`, `shipping.dhl`, `shipping.dpd`
- `accounting.invoices`, `accounting.corrections`, `export.datev`
- `custom.catalog` and any future module outside the exact active allowlist

The standalone [`compose.amazon-pilot.yml`](../compose.amazon-pilot.yml) contains only `db`, `backend`, and `frontend`. It cannot start Vendure, Storefront, a payment service, a shipping service, or a DATEV service because none is defined in that deployment graph.

## Amazon transport boundary

The sealed transport operation enum and CI contract permit exactly:

1. LWA token refresh.
2. `createReport`.
3. `getReport`.
4. `getReportDocument`.
5. HTTPS retrieval of the returned pre-signed Amazon document URL.

Method and Reports API path are derived inside that enum boundary; callers cannot supply a free HTTP method/path. Downloads reject non-HTTPS and non-Amazon AWS/CloudFront hosts, redirects are disabled, resource IDs are constrained, transport errors omit URLs, and request IDs are stored only as a 12-character SHA-256 prefix. `scripts/check-amazon-operation-allowlist.mjs` compares the enum and allowlist exactly, scans backend imports/dependencies and endpoint markers, and enforces sole ownership of Amazon hosts and `x-amz-access-token`.

There is no Listings Items, Product Pricing, Orders, Inventory, Ads, Fulfillment, or Feeds mutation client. The pilot can never change Amazon prices, ads, listings, stock, orders, returns, or fulfillment configuration. `createReport` is treated only as read-only analytical acquisition.

## Data and diagnostic boundary

- Raw transport and decoded bytes are stored separately with SHA-256 hashes. Database triggers prevent raw/decoded updates and archive-row deletion.
- Parser versions and comparability keys travel with snapshots and backups.
- Analyses are deterministic rules with facts, delta, trend, anomalies, hypotheses, optional actions, uncertainty, missing data, and evidence references.
- Exports recursively remove buyer/customer/address/email/order/comment/phone fields and contain aggregate evidence only.
- The UI displays redacted seller ID, region, marketplaces, role declarations, credential-shape status, latest job/success, retry/rate-limit metadata, archive hashes/size, parser, snapshot compatibility, missing data, and the last backup verification.
- Tokens, client secrets, refresh tokens, raw payloads, clear seller IDs, and buyer data are never displayed.
- A live pilot connection accepts only the logical reference `pilot_seller`; its LWA values remain
  exclusively in `AMAZON_SECRET_PILOT_SELLER` and are never persisted.

## Reproducible start

`scripts/start-amazon-pilot.sh` defaults to `--check`, requires an explicitly named local environment file, renders no environment values, checks the exact service allowlist, and makes no data change. `--start` always uses Compose project `essentials-merchant-amazon-pilot`, starts the three named services, verifies the persisted state as JSON, and stops application services fail-closed if the module/service check differs.

The synthetic browser flow logs in as administrator, checks the banner/modules, creates a fixture connection, follows report polling through snapshot and analysis, downloads a PII-minimized export, verifies blocked mutation routes, and rejects serious/critical axe findings. Synthetic evidence is not Amazon staging evidence.

## Evidence levels

| Level | Current meaning |
|---|---|
| Synthetic fake | Local fixture/fake SP-API, deterministic and automated |
| Local Compose | Exact three-service pilot with disposable PostgreSQL |
| Amazon staging | Externally blocked until the documented approval/credential/role gate succeeds |
| Real seller | Not claimed by repository tests or this milestone |
| Production | Not approved; no availability, RPO/RTO, legal, tax, or provider certification claim |
| Unverified | External disk encryption, live rate limits/roles/marketplace participation, container/Rust advisory scanners not installed locally |

See [`operations/AMAZON_STAGING_GATE.md`](operations/AMAZON_STAGING_GATE.md), [`DEFERRED_EXTERNAL_GATES.md`](DEFERRED_EXTERNAL_GATES.md), and [`security/VENDURE_ADVISORIES.md`](security/VENDURE_ADVISORIES.md).
