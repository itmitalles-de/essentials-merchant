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

## 2026-08-20 — Reuse one Merchant analysis boundary for manual and SP-API acquisition

**Decision:** Official manual Sales and Traffic JSON/CSV/TSV is the default acquisition path. It
feeds the same immutable archive, normalized snapshot, deterministic comparison, and aggregate
export boundary as optional SP-API acquisition. The Mantle wiki runtime/cache/report generator is
not copied. Identical raw bytes are idempotent; different bytes for the same semantic period are a
visible conflict rather than a second competing snapshot.

**Reason:** Mantle needs useful analysis before external credentials exist, while duplicate
implementations and ambiguous period revisions would weaken evidence and operations.

**Consequences:** Operators explicitly confirm missing timezone/flat-file metadata, parser errors
cannot leave partial data, and comparisons require equal marketplace, report, granularity, parser,
period length, currency, and timezone. ZIP remains unsupported. SP-API is optional and retains the
same exact read-only operation gate with no scheduler on first use.

## 2026-08-20 — Gate generative strategy behind minimized aggregate evidence

**Decision:** The Mantle OpenAI integration is an explicit operator-triggered adapter over a closed
DTO derived from the existing deterministic analysis, not a second parser or analysis system.
Deterministic facts and derivations remain canonical. Model output is visibly classified as
assessment, hypotheses, possible measures, uncertainty, missing evidence, and open questions. The
adapter has no Amazon or Merchant mutation tool, scheduler, raw-report access, or product/customer
identifier input.

**Reason:** Mantle wants conversational strategy support, but a ChatGPT subscription does not supply
server API credentials or API billing. Raw Amazon business reports must not be disclosed merely to
gain narrative output, and probabilistic text must not be presented as evidence.

**Consequences:** The adapter fails closed without a dedicated project-scoped
OpenAI API key and approved provider data controls. The later Mantle no-login
decision governs its encrypted write-only storage; a host-environment key
remains only as a legacy fallback. Requests use the fixed Responses API, `store: false`, bounded
payloads, and explicit aggregate-hash confirmation. The synthesis request has no tools and uses a
strict output schema. Only validated output
and redacted metadata are persisted immutably and idempotently; prompts/raw provider responses are
not stored. The Mantle environment later supplied and technically proved the
external credential, while environments without it retain the fully usable
deterministic path and a disabled gate rather than a simulated provider result.

## 2026-08-20 — Make Mantle strategy a single weekly continuity loop

**Decision:** Mantle exposes exactly one `Analyse` button over the existing Marketplace
Intelligence results. Each request uses at most thirteen distinct newest-first aggregate analyses and
the last validated strategy result as untrusted continuity context. Every accepted response has the
same strict structure and ends with a handover containing continuity, interim priorities, evidence
to collect, and next-run checks. A successful immutable row is unique per Monday-based
Europe/Berlin calendar week.

**Reason:** The operator wants a stable weekly management ritual, not one unrelated model answer per
report card. A server/database cadence boundary controls pay-per-use spend and accidental repeat
clicks, while explicit handover preserves continuity without relying on provider-side conversation
storage or exposing raw reports.

The thirteen-period window is the bounded 91-day full baseline. Subsequent
runs add current evidence and the last validated handover, so they remain an
incremental weekly continuity loop without a separate paid-analysis action.

**Consequences:** The UI has one button and one fixed KPI/strategy layout. KPI bars remain
deterministic and are never model-generated. The browser posts only the current aggregate hash and
confirmation; failed provider calls do not consume the week, while an accepted row disables another
run until the next local Monday. New imports after a run are visibly marked as outside the assessed
hash. This workflow reads imported aggregates only and does not bypass the separate SP-API
credential gate.

## 2026-08-20 — Keep Amazon product identity internal while enabling variant analysis

**Decision:** Product classification is an append-only Merchant metadata
boundary. Only Child ASINs observed in validated live Sales and Traffic
snapshots can be mapped, every revision requires explicit operator
confirmation, and the settings page retains optional SKU/provenance for
internal lookup. OpenAI receives only reviewed brand/family/variant/pack-size
labels, bounded aggregate metrics, semantic evidence references, and coverage
counts; Child ASIN and SKU never enter either provider request.

**Reason:** Mantle needs pack-size and product-family strategy instead of one
portfolio total, but Amazon identifiers are neither necessary nor appropriate
provider context. Versioned mappings preserve correction history and avoid a
second analysis implementation.

**Consequences:** Saving a mapping writes local audit metadata only and has no
Amazon transport or mutation capability. Unmapped products are not guessed;
the assessment must retain the visible coverage limitation.

## 2026-08-20 — Keep public research separate from private Amazon evidence

**Decision:** One weekly action first runs a bounded public web-research request containing only a
fixed Mantle/category/market brief and the current date. A separate tool-free synthesis request
receives the closed internal aggregate DTO, last handover, and bounded public text with
server-canonicalized references and citation excerpts kept in provider citation order. Fixed output
sections separate public observed facts,
possible consumption effects, confidence, and uncertainty for competitors, category/market trends,
and global events or crises.

**Reason:** Current market context is useful for strategy, but placing internal sales metrics in a
web-search request would expand disclosure and let untrusted pages influence private-data queries.
Public events can suggest hypotheses about demand or price sensitivity; they cannot establish the
cause of an internal metric change.

**Consequences:** The public request permits only the built-in `web_search` tool and at most three
tool calls. It receives no report period, metric, hash, identifier, or handover. Both Responses use
`store: false`; the second request has no tools. Public source URLs are bounded and canonicalized,
and every accepted public signal needs a validated `public:*` reference. One button can incur two
provider requests, but only a successful validated synthesis consumes the weekly slot.

## 2026-08-20 — Add Ads only as aggregate evidence through the manual boundary

**Decision:** Official aggregate Sponsored Products campaign reports use the existing manual
archive, snapshot, deterministic comparison, strategy-input, and export boundary. The first Ads
path is JSON/CSV/TSV upload, not an Ads API client. It normalizes only impressions, clicks, spend,
attributed outcomes, CTR, CPC, ROAS, and ACOS with a confirmed attribution window.

**Reason:** Ads evidence reduces a major uncertainty in Sales and Traffic interpretation without
waiting for separate Amazon Ads authorization or introducing a competing analysis system.

**Consequences:** Search-term, keyword, targeting, ASIN/SKU, and product-level reports are rejected.
Every accepted row must have a campaign-name or campaign-ID dimension proving campaign-level
shape. Campaign names and IDs remain only in the confidential raw archive and never enter normalized
metrics, summaries, or OpenAI. Ads comparisons require matching marketplace, granularity, parser,
period length, currency, timezone, and attribution window. There is no campaign, bid, budget, or
targeting mutation path; a future read-only Ads API gate requires an independent review.

## 2026-08-20 — Use a LAN-scoped no-login shell with write-only provider setup

**Decision:** The canonical Mantle hostname always replaces browser credentials
with a 12-hour `mantle-amazon-read-only` JWT issued only through the dedicated
same-origin frontend proxy. The normal login endpoint is disabled in this mode.
The scoped token can reach only explicit Amazon pilot reads, manual acquisition,
the weekly strategy command, and provider configuration; it cannot reach ERP,
raw-report, scheduler, health-write, or near-miss routes. Caddy's LAN/VPN source
matcher is the human access-control boundary requested for this pilot.

OpenAI and Amazon LWA credentials are accepted by write-only GUI endpoints,
encrypted in the backend with AES-256-GCM and a host-only 32-byte master key,
and stored as opaque ciphertext. Status returns only configured state, field
names, approval state, and timestamps. Pilot backups include the table schema
but explicitly exclude its data, so restores require credential re-entry. This
supersedes the earlier host-environment-only credential-storage consequence for
the Mantle deployment; the environment mechanism remains a legacy fallback.

**Reason:** The operator explicitly requires no login and GUI-based credential
entry without readback. A narrow ephemeral token prevents a stale Merchant
admin token from silently widening the UI, while application-layer encryption
avoids plaintext persistence and backup propagation. No-login necessarily means
every client already inside the allowed network can replace credentials, which
is documented rather than hidden.

**Consequences:** `PILOT_SECRETS_KEY` is mandatory for the Mantle no-login
profile and must never be printed or backed up. The single weekly `Analyse`
button obtains one seven-day Sales and Traffic report when approved Amazon
credentials exist, otherwise uses manual imports, then submits the closed
aggregate history plus previous handover only to the tool-free synthesis step;
the preceding public research sees neither. Provider failures do not
consume the weekly slot; no scheduler or mutation capability is added.

## 2026-08-20 — Keep business knowledge curated, immutable, and separate from AI memory

**Decision:** Mantle/Sphagnum reference knowledge crosses from approved Wiki
and Notes sources only once as a typed, reviewed business-context bundle. The
database stores the bounded statements, source provenance, status, and content
hashes in one immutable singleton row. Raw documents, personal notes, PII, and
secrets are never copied. Weekly continuity is carried by the separately
validated handover; model output cannot rewrite the source baseline.

**Reason:** Every analysis needs stable knowledge of the business, but copying
the document corpus or letting probabilistic output mutate its own source truth
would expand disclosure, create unverifiable drift, and form a second knowledge
system.

**Consequences:** Identical imports are idempotent and a different second
baseline is rejected. Every synthesis receives the same reviewed baseline plus
the latest validated handover. A source update requires a separately reviewed
migration/versioning decision rather than a hidden in-place edit.
