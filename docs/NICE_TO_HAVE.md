# Deferred Essentials+ Merchant capabilities

These are product options, not implemented stubs. None has an unused dependency, empty migration,
placeholder endpoint, or dormant runtime table in this repository.

## Additional payment providers

Benefit: lets merchants choose regional pricing, payment methods, and risk policies. Scope: one
separate connector module per provider, including authorization/capture/refund, signed webhooks,
idempotency, reconciliation, admin health, and sandbox tests. Prerequisites: a production-accepted
first provider pattern and support ownership. Risks: inconsistent payment state semantics, charge
duplication, sensitive credentials, and operational fragmentation. Deferred because the current
task establishes ports/fakes without pretending to verify provider behavior. Re-evaluate when a
merchant contract requires a named provider and sandbox credentials are available.

## Additional shipping providers

Benefit: regional carrier choice and resilience. Scope: separate connector modules, never a shared
generic DHL/DPD switch, with shipment state maps, callbacks/polling, reconciliation, health, and
audit. Prerequisites: documented carrier contract, test account, data-retention review, and a proven
DHL or equivalent adapter. Risks: tracking semantics, PII in labels/addresses, carrier outages, and
support overhead. Deferred to keep provider ownership explicit. Re-evaluate when shipment volume or
merchant contracts justify another carrier.

## Shipping label generation

Benefit: removes duplicate portal entry and ties labels to fulfillment. Scope: validated address,
service/product selection, label bytes, void/reprint, document retention, and audit. Prerequisites:
production shipping connector and privacy-aware document storage. Risks: billable duplicate labels,
wrong services, personal address data, and retention duties. Deferred because tracking-only/manual
fulfillment is safer before live carrier acceptance. Re-evaluate after one production carrier has
stable reconciliation.

## B2B price lists and additional Vendure channels

Benefit: customer-specific assortments, pricing, currencies, and storefronts. Scope: channel-aware
Core projections, price-list precedence, customer assignment, tax behavior, and migration. Prerequisites:
an ownership decision for B2B pricing and channel-specific mapping tests. Risks: cross-channel data
leakage and stale prices. Deferred because the current contract intentionally supports one channel.
Re-evaluate after a concrete B2B merchant workflow is specified.

## Multi-warehouse inventory

Benefit: exposes availability and fulfillment choice by location. Scope: Core warehouse/location
model, reservations, transfers, allocation strategy, projections, and recovery. Prerequisites:
warehouse source-of-truth and operational booking rules. Risks: overselling, double reservation,
and materially harder recovery. Deferred because current available stock is a single authoritative
quantity. Re-evaluate when a second physical stock location is operationally required.

## Additional marketplaces

Benefit: consolidates sales and operational intelligence beyond Amazon. Scope: one read-only or
commerce connector module per marketplace, explicit ownership, credentials, report schemas,
idempotency, and PII policy. Prerequisites: provider contract, seller roles, fixtures, and data
classification. Risks: incompatible metrics, account writes, and credential scope. Deferred to
finish and live-gate Amazon read-only first. Re-evaluate after the Amazon staging gate and a named
merchant demand.

## Returns portal

Benefit: self-service return requests and clearer status. Scope: storefront workflow, eligibility,
reasons, authorization, carrier handoff, refunds, stock disposition, and audit. Prerequisites:
production payment/shipping integrations and explicit return accounting. Risks: fraud, personal
data, double refunds, and incorrect restocking. Deferred because current returns reports are
raw-only. Re-evaluate after refund and carrier return contracts are production-tested.

## Purchasing and replenishment

Benefit: supports supplier orders and stock planning. Scope: suppliers, purchase orders, receipts,
lead times, reorder proposals, commitments, and audit. Prerequisites: supplier/master-data design
and multi-stage inventory ownership. Risks: incorrect commitments and a major expansion of ERP
scope. Deferred to preserve the compact Core. Re-evaluate when manual purchasing becomes a measured
operational bottleneck.

## Demand forecasting

Benefit: improves replenishment and identifies future stock risk. Scope: historical feature set,
evaluation baseline, horizon/confidence, overrides, and evidence. Prerequisites: sufficient clean
history across comparable granularities and purchasing data. Risks: false precision, seasonality
claims on sparse data, and overstock. Deferred because deterministic descriptive analysis must
accumulate history first. Re-evaluate after at least one full seasonal cycle or a validated external
dataset.

## External AI analysis for Marketplace Intelligence

Benefit: richer synthesis of aggregate trends and hypotheses. Scope: provider-neutral opt-in
interface, schema validation, minimized allowlist payload, prompt/model versioning, failure fallback,
and privacy review. Prerequisites: deterministic baseline, provider contract, retention terms, and
evaluation set. Risks: hallucination, data transfer, prompt drift, cost, and vendor dependency.
Deferred because the current product must work fully offline and rule-based. Re-evaluate after
measured user value and a completed privacy/security review.

## Automatic price or advertising proposals

Benefit: could reduce repetitive optimization work. Scope: recommendation workflow, approval,
guardrails, budgets, rollback, and outcome measurement; automatic execution would be a separately
authorized phase. Prerequisites: trustworthy intelligence, Amazon write APIs, granular permissions,
and human approval policy. Risks: direct revenue impact, runaway spend, marketplace policy breaches,
and poor causal inference. Deferred because Marketplace Intelligence is deliberately read-only.
Re-evaluate only after sustained evidence quality and explicit governance.

## Mobile warehouse app

Benefit: faster scanning, picking, and receiving. Scope: responsive/offline UI, device identity,
barcode handling, conflict resolution, and least-privilege APIs. Prerequisites: warehouse workflow
and inventory reservation model. Risks: offline conflicts, device loss, and operational errors.
Deferred because current inventory operations are desktop-oriented and single-location. Re-evaluate
when warehouse transaction volume supports a dedicated client.

## Multi-tenancy

Benefit: hosts multiple legally separate merchants on shared infrastructure. Scope: tenant identity,
row/channel isolation, keys, quotas, billing, migrations, backup/restore, and support tooling.
Prerequisites: a product/business decision and comprehensive isolation model. Risks: cross-tenant
data exposure and much higher operational complexity. Deferred explicitly; this repository is a
single-merchant product. Re-evaluate only after a separate architecture and threat-model project.

## Kubernetes

Benefit: may help large deployments with orchestration and scaling. Scope: manifests/operators,
secrets, ingress, stateful backups, observability, rollouts, and support. Prerequisites: measured
scale or availability requirements beyond Compose. Risks: cost and operational complexity without
product value. Deferred because one reproducible Compose topology fits the current merchant scope.
Re-evaluate when deployment metrics demonstrate the need.

## Fully automated accounting

Benefit: reduces bookkeeping handoffs. Scope: bank/payment reconciliation, account rules, tax
periods, approvals, locking, audit, exceptions, and regulated interfaces. Prerequisites: accountant
review, validated DATEV flow, immutable ledger completeness, and clear liability boundaries. Risks:
tax/legal errors and silent misposting. Deferred because Essentials+ Merchant currently exports
reviewable immutable entries only. Re-evaluate after DATEV validation and professional domain
review.
