# Decisions

Record only durable choices that future agents might otherwise undo or reopen.
Implementation and operational detail remains authoritative in `README.md` and
the referenced source files.

## 2026-08-12 - Keep Core and Vendure as separate systems of record

**Decision:** Merchant Core owns SKU, ERP master data, available stock,
imported orders, invoices, and accounting. Vendure owns merchandising, cart,
checkout, promotions, payment, and Shop/Admin APIs. Each uses its own database.

**Reason:** ERP/accounting integrity and commerce concerns have different
lifecycles; sharing tables would couple upgrades and make ownership ambiguous.

**Alternatives considered:** Moving ERP responsibility into Vendure or sharing a
single PostgreSQL schema.

**Consequences:** Integrations exchange explicit events/projections. The
Storefront uses only the Vendure Shop API and never queries Core directly.

## 2026-08-12 - Preserve internal erplite compatibility names

**Decision:** Use `Merchant` under the Essentials Plus working brand for visible product naming while retaining
existing `erplite` database, volume, crate, token-storage, and migration names.

**Reason:** Mechanical renaming would break deployed persistence and clients.

**Alternatives considered:** Renaming every internal identifier with the brand.

**Consequences:** Any future internal rename is a separately planned data and
compatibility migration with backup, rollback, and deployment coordination.

## 2026-08-13 - Use transactional outboxes and idempotent consumers

**Decision:** Cross-system intent is written to an outbox in the owning local
database transaction and delivered at least once. Inbox/event uniqueness,
mapping uniqueness, leases, retries, and monotonic sequences protect consumers.

**Reason:** Core and Vendure cannot share a database transaction, and process or
network failures must not lose events or double-book stock.

**Alternatives considered:** Synchronous dual writes and claims of distributed
exactly-once delivery.

**Consequences:** Every consumer must be replay-safe. Recovery and worker-restart
tests are required; duplicates and delays are normal operating conditions.

## 2026-08-11 - Preserve invoice and money invariants

**Decision:** Monetary values use decimals or integer minor units. Only draft
invoices are editable; issuing assigns a unique number and snapshots customer/
company data. Paid/cancelled states are terminal under the implemented lifecycle.

**Reason:** Later master-data changes or floating-point behavior must not alter
issued financial documents.

**Alternatives considered:** Live joins for issued invoice data or editable sent
invoices.

**Consequences:** Corrections require an explicit correction document/flow.
DATEV exports must derive from immutable accounting entries and be reference-tested.

## 2026-08-13 - Pin commerce runtime and use explicit migrations

**Decision:** Pin Vendure and Node-compatible dependencies, keep TypeORM
`synchronize: false`, and commit explicit reviewed migrations. SQLx checked
queries use the committed offline cache in CI.

**Reason:** Automatic schema changes and uncoordinated dependency drift risk both
databases and make builds unreproducible.

**Alternatives considered:** Runtime schema synchronization and floating Vendure
versions.

**Consequences:** Refresh `backend/.sqlx` after relevant Core schema/query changes,
generate Vendure migrations only against disposable databases, and test upgrades
on restored copies before production.

## 2026-08-13 - Keep providers out of the first vertical slice

**Decision:** The implemented flow uses a clearly labeled test payment and manual
fulfillment until failure recovery is covered.

**Reason:** Reliability of the ownership and event model is the prerequisite for
safe provider integration.

**Alternatives considered:** Adding multiple production providers during the
initial Vendure integration.

**Consequences:** Test payment must never be exposed as production payment. Add
one payment and one shipping provider only after recovery coverage is complete.

## 2026-08-13 - Marketplace Intelligence is an optional read-only Essentials Plus module

**Decision:** Marketplace Intelligence uses a persistent Core-side job and archive model, Amazon
Reports API v2021-06-30 with LWA OAuth, and deterministic analysis before an optional provider.
It is disabled by default and makes no Amazon write operation.

**Reason:** Amazon access, report roles, and data quality vary by seller. The system must remain
demonstrable with fixtures and safe when disabled or unavailable.

**Consequences:** Admins see the full module catalog; normal users need an enabled module and a
grant. Disabling stops worker claims and API actions but retains audits, raw documents, snapshots,
and analyses. Amazon secrets remain environment-owned and are referred to only by logical keys.
