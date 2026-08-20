# Optional SP-API gate

## Current state

SP-API is an external gate, not a dependency of the manual import path. No fake
credentials are generated and no credential value is stored in Git. Mantle can
submit LWA values once through the write-only internal GUI; the backend encrypts
them before database persistence, never returns them, and excludes their rows
from pilot backups. Without an explicitly approved credential set, manual upload
stays fully usable.

## Allowed capability

The transport allowlist contains exactly:

1. LWA token refresh;
2. Reports API `createReport`;
3. Reports API `getReport`;
4. Reports API `getReportDocument`;
5. HTTPS download from an allowlisted Amazon document host.

Only `GET_SALES_AND_TRAFFIC_REPORT` is approved for the first live request. The
window must be one short, completed UTC period and the request must use the
supported Sales and Traffic options. No scheduler is enabled for the first run.

There is no Orders, Listings, Pricing, Feeds, Ads, Inventory mutation, payment,
shipping, or restricted-data client. Buyer and order PII are neither requested
nor parsed. The operation allowlist is checked in CI.

## Preconditions for a one-shot live test

- Mantle has created a private SP-API application, self-authorized it, and
  entered its LWA refresh token, client ID, client secret, Seller ID,
  Marketplace ID, and region through the write-only GUI.
- The operator confirms both authorization and the read-only Reports-only
  boundary. The backend binds that confirmation to a SHA-256 of seller, region,
  and marketplace; it stores no readable approval context in the secret table.
- The live connection uses the fixed logical secret reference `pilot_seller`,
  the `Brand Analytics` role, and exactly one marketplace.
- All automatic schedules remain disabled.
- The exact deployed Git SHA has green backend, security, Amazon pilot, Docker,
  and recovery checks.

## Runtime safeguards

The client applies bounded exponential backoff, honours `Retry-After`, polls a
bounded number of times, enforces transport and decoded byte limits, and hashes
both downloaded and parser input bytes. Amazon request IDs are retained only as
a twelve-character SHA-256 prefix. Tokens, signed download URLs, secret values,
and complete request IDs are never logged.

Successful SP-API bytes enter the same immutable archive, parser, snapshot,
analysis, comparison, and aggregate-export boundary as a manual report.

On `ai-marketing.mantle-climbing.de`, the single `Analyse` button requests the
last seven fully completed UTC days only when this approved live connection is
configured. It reuses an identical in-flight or completed run, polls with a
bounded ten-minute UI wait, then submits the newly built aggregate hash to the
weekly AI gate. There is no scheduler and no separate Amazon action button in
the AI-first view.

## Gate outcome

Until the credentials and approval above exist, record the outcome as
`externally_blocked_missing_approved_credentials`. This is not an application
failure and must not disable manual upload.
