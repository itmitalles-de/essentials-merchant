# Deferred external gates

Amazon Reports remains the only Amazon transport track. Mantle's optional
OpenAI strategy adapter is active only for the closed aggregate-data path and
is not an Amazon mutation or report-acquisition client. One approved report and
weekly assessment succeeded on 2026-08-20; that success does not authorize any
transactional adapter below. The ports, fakes, mapping tables, recovery
behavior, and retained Commerce tests remain in the repository, but no listed
transactional adapter is implemented or activated. Each still requires a
separate scoped decision.

| Gate | Benefit | Prerequisite | Current technical state | External account | Security review | Business acceptance | Earliest start / milestone decision |
|---|---|---|---|---|---|---|---|
| Stripe Payment Intents adapter | Real card/payment lifecycle and reconciliation | Approved product/regions, settlement model, refund/accounting mapping, idempotency contract | Provider-neutral payment port, deterministic fake, callback signature/replay and reconciliation tests retained; no Stripe adapter | Merchant Stripe sandbox and production onboarding required | Secret storage, webhook signature/rotation, PCI scope, replay, amount/currency/order binding, egress and incident review | Finance/operations approve authorization, capture, failure, refund and payout handling | After successful Amazon pilot; no active implementation now |
| Real payment webhooks | Provider-originated payment state updates | Selected payment adapter and stable public callback endpoint | Signed synthetic callback verifier only | Provider webhook endpoint and signing secret required | TLS, source/auth verification, nonce/replay, rotation, payload retention and alerting | Reconciliation and exception ownership accepted | After successful Amazon pilot; disabled now |
| DHL Parcel Germany adapter | Automated shipment creation/tracking | Contract products, billing numbers, label formats, test receiver data and API access | Shipping port and manual/fake state tests retained; no DHL client | DHL developer/merchant sandbox and contracted product required | Credential storage, address/label PII, idempotency, cancellation, egress and retention review | Warehouse/carrier label and tracking acceptance | After successful Amazon pilot; no active implementation now |
| DPD adapter | Alternative parcel carrier | Carrier selection decision and DPD API contract | Separate `shipping.dpd` module remains `not_installed`; no adapter | DPD account/sandbox required | Same carrier/PII/label controls plus API-specific auth review | Warehouse and exception-flow acceptance | After successful Amazon pilot; no active implementation now |
| Real carrier labels | Printable production shipment documents | One accepted carrier adapter, printer/layout and cancellation flow | No label generator; manual tracking path retained outside pilot | Contracted carrier credentials/products required | Address PII, barcode integrity, access control, storage/deletion and reprint audit | Warehouse signs off samples and operational rollback | After successful Amazon pilot; disabled now |
| DATEV activation | Transfer immutable accounting entries to tax/accounting tooling | DATEV checking-program validation and import into an approved empty test client | Deterministic EXTF-v13 renderer, immutable batches and local fixtures retained; `export.datev` disabled | Approved DATEV test client/adviser workflow required | Financial-data access, encrypted transfer/retention, tamper evidence and operator separation | Tax adviser/accounting signs off format and mappings | After successful Amazon pilot; no activation now |

No automatic procurement, additional marketplace, multi-tenant control plane, or Kubernetes deployment is introduced by any of these retained contracts. The separately documented OpenAI strategy gate has no tools or execution path.
