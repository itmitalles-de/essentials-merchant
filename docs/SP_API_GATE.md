# Optional SP-API gate

## Current state

SP-API is an external gate, not a dependency of the manual import path. No fake
credentials are generated and no credential value is stored in Git or the
database. Without an explicitly approved secret reference, manual upload stays
fully usable.

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

- Mantle has supplied and explicitly approved LWA refresh token, client ID, and
  client secret through the host secret environment.
- The approved seller, marketplace, report type, period, options, and expiry are
  recorded in the staging-gate file without secret values.
- The live connection uses the approved logical secret reference and one
  marketplace.
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

## Gate outcome

Until the credentials and approval above exist, record the outcome as
`externally_blocked_missing_approved_credentials`. This is not an application
failure and must not disable manual upload.
