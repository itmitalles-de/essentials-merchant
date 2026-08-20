# Decisions

Record only durable choices that future work might otherwise undo. Implementation and operations
remain authoritative in the linked source and documentation.

## 2026-08-12 — Keep Core and Vendure as separate systems of record

**Decision:** Core owns SKU/master data, available stock, imported orders, invoices, immutable
accounting, modules, diagnostics, and Marketplace Intelligence. Vendure owns merchandising, cart,
checkout, promotions, payment/fulfillment runtime, and Shop/Admin APIs. Each keeps its PostgreSQL
database.

**Reason:** ERP/accounting integrity and commerce have different lifecycles. Shared tables or
moving Core authority into Vendure would make upgrades and failure ownership ambiguous.

**Consequences:** Cross-system work uses explicit at-least-once events, mappings, monotonic
projections, idempotent consumers, and recovery tests; there is no distributed transaction.

## 2026-08-13 — Exact visible brand, stable internal compatibility names

**Decision:** The visible name is exactly `Essentials+ Merchant`. Existing `erplite`, crate,
database, volume, migration, mapping, token-storage, and `shop-suite-*` identifiers stay unchanged.

**Reason:** Presentation branding must not break deployed persistence, APIs, or imports.

**Consequences:** The repository slug is `itmitalles-de/essentials-merchant`. Any future internal
identifier rename is a separate versioned migration with backup, rollback, and compatibility
planning. The license is unchanged.

## 2026-08-13 — Durable delivery plus signed internal requests

**Decision:** Local transactions enqueue outbox intent; consumers are idempotent and use persisted
leases, attempts, capped exponential backoff, dead state, and controlled requeue. Core/Vendure HTTP
requests use HMAC-SHA-256 over method, path, timestamp, nonce, and body hash with persisted nonce
replay protection and current/previous-key overlap.

**Reason:** Process, network, and database failures are normal boundaries, while a static shared
header neither authenticates request contents nor prevents replay.

**Consequences:** Payload and mapping uniqueness, not delivery count, protects business effects.
Production still needs TLS/private networking and synchronized clocks. Diagnostics and logs are
redacted; the test environment alone may shorten timing and trigger process failpoints.

## 2026-08-13 — Persist the module contract inside this repository

**Decision:** Essentials+ module manifests and state are implemented directly in Core without a
shared runtime library or control plane. Administrators see the full catalog; ordinary users and
APIs require enabled state plus permission. Dependencies, conflicts, configuration health, and
transitions are checked transactionally and audited.

**Reason:** Product-specific ownership and failure behavior belong next to this product's APIs,
jobs, webhooks, and persistence.

**Consequences:** Required Core modules cannot be disabled. Disabling an optional module stops its
navigation, APIs, jobs, and webhooks but retains all data/history. DHL, DPD, payment, shipping, and
Marketplace connectors are independent modules.

## 2026-08-13 — Preserve invoice and accounting immutability

**Decision:** Money is Decimal/integer minor units. Issued invoices are immutable snapshots;
corrections are separate numbered documents with an explicit source reference and reversed
snapshotted entries. Accounting exports derive only from immutable entries.

**Reason:** Later master-data changes, float behavior, or retries must not rewrite issued financial
history or create duplicate corrections/bookings.

**Consequences:** A full correction is one-per-source and request-idempotent and never books stock.
DATEV rendering remains disabled behind external checker/test-client validation; no tax/legal or
DATEV-compatibility claim is made.

## 2026-08-13 — Marketplace Intelligence stays deterministic and Amazon-read-only

**Decision:** `marketplace.amazon_intelligence` uses LWA OAuth and Reports API `v2021-06-30`, no
IAM/SigV4 and no Amazon write operation. It stores exact transport bytes, decoded bytes and hashes,
versioned normalized snapshots, and deterministic rule analyses. No external LLM provider is part
of this implementation.

**Reason:** The feature must work offline with synthetic fixtures and must not send raw reports or
buyer PII to another provider. Different parser/granularity/period keys are not silently compared.

**Consequences:** Sales & Traffic JSON v2 and Inventory Planning TSV v1 are analysable; Returns and
Settlement V2 are raw-only. Unknown types never become successfully analysed. A real seller/role/
marketplace acceptance remains an explicit external gate.

## 2026-08-13 — Stripe and DHL are candidates; ports/fakes are the verified scope

**Decision:** Stripe Payment Intents is the payment candidate and DHL Parcel Germany the shipping
candidate, based on official APIs, European small-merchant fit, sandbox/authentication,
idempotency/webhook/reconciliation capabilities, and operating burden. DPD remains a separate
disabled connector module. Real adapters are not claimed without account-specific sandbox
contracts.

**Reason:** Public documentation establishes direction but cannot prove enabled products,
credentials, callback configuration, negotiated fields, or account behavior.

**Consequences:** Provider-neutral ports, complete local fake providers, signed callback/replay
checks, status mapping, retries, reconciliation, money/order checks, carrier/tracking, and audit are
implemented and tested. Stripe/DHL production adapters and sandbox acceptance remain external work.

## 2026-08-13 — Pin Vendure and rehearse schema/backup changes

**Decision:** Vendure packages remain pinned together at 3.7.2, TypeORM `synchronize` remains
false, SQLx offline metadata is committed, and migrations/upgrades run only against disposable or
restored non-production data. Backups quiesce both writers and restore only into an empty project.

**Reason:** Automatic schema drift, forced dependency downgrades, or partial two-store backups are
not reproducible recovery strategies.

**Consequences:** The incompatible npm forced fix is prohibited. Every change to persistence must
rerun SQLx/migrations, the two-database recovery flow, and checksum-backed restore rehearsal.

## 2026-08-19 — Make Amazon Intelligence the only active external pilot

**Decision:** The persisted `amazon-read-only` profile has an exact positive module allowlist and a
server-wide fail-closed HTTP mutation boundary. Its standalone Compose topology contains only
PostgreSQL, Core, and the admin frontend. Vendure, Storefront, payment, shipping, DATEV, schedules,
and custom mutations are absent or disabled; retained code and tests remain.

**Reason:** A runtime assembled from an explicit small boundary is reviewable and materially
reduces attack surface while keeping historical ERP/Commerce compatibility intact. Navigation
visibility alone cannot establish a read-only system.

**Consequences:** Any unknown or future active module, including a required one, makes the profile
non-compliant. Applying the profile preserves `not_installed` state, disables schedules, and holds
queued live jobs. Other providers and DATEV receive no implementation work before a successful
Amazon pilot and later approval.

## 2026-08-19 — Seal transport to an exact Amazon operation enum

**Decision:** Pilot code derives HTTP method and path only from LWA refresh, `createReport`,
`getReport`, `getReportDocument`, or validated presigned download operations. A repository contract
check owns the Amazon host/header markers and rejects mutation clients, mutating methods, SDKs, and
allowlist drift.

**Reason:** A descriptive allowlist next to a free-form request builder would not prevent a future
caller from reaching a write API.

**Consequences:** Live Sales & Traffic acquisition also requires a server-side seller/region/
marketplace approval and administrator one-shot request. Redirects and non-Amazon download hosts
are rejected; persisted request IDs and failures are redacted. The staging gate stays blocked when
external credentials or roles are unavailable.

## 2026-08-19 — Reduce dormant Commerce exposure without claiming remediation

**Decision:** Vendure/Storefront are not started in the Amazon profile. Actions, language/runtime
versions, images, and downloaded archives are pinned or checksum-verified; lockfile-derived SBOMs
and a redacted dependency gate are maintained. Known Vendure-path advisories remain individually
open until a compatible upstream remediation exists.

**Reason:** Removing an unnecessary runtime reduces reachable attack surface, but it does not fix
retained dependency vulnerabilities. Forced incompatible audit fixes are not trustworthy release
engineering.

**Consequences:** Security documents distinguish installed, reachable-in-pilot, compensated, and
remediated states. CI fails on new/critical audit drift and never runs automatic force-fixes.
