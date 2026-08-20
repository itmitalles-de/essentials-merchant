# Mantle Amazon AI strategy gate

## Purpose

The optional strategy panel turns an existing deterministic Sales and Traffic
analysis into a German-language decision aid. It does not parse reports, create
facts, replace the deterministic comparison, or introduce another analysis
system. It is an explicitly triggered interpretation step inside Marketplace
Intelligence.

The Mantle target entry point is `https://ai-marketing.mantle-climbing.de`. It serves
the same three-service `essentials-merchant-amazon` deployment and opens the
AI-first Marketplace Intelligence view. The route must remain limited to
LAN/VPN clients by split DNS and Caddy source-address policy.

## External account gate

A ChatGPT Pro or other ChatGPT subscription does not include OpenAI API access,
API credits, or an API key. Activation requires a separately billed,
project-scoped OpenAI API key approved for this Mantle workload. No substitute
or fake credential may be generated.

Live status on 2026-08-20 is
`externally_blocked_missing_pay_per_use_api_key`: the route and aggregate gate
are deployed, but the feature flag is false and no provider request has run.
The manual and deterministic workflow is unaffected.

The server reads these private environment entries:

```text
OPENAI_STRATEGY_ENABLED=true
OPENAI_STRATEGY_MODEL=gpt-5.6
OPENAI_API_KEY=<server-side project key>
```

The populated environment file stays on the Docker host with mode `0600`. The
key is supplied only to the backend container. It is not stored in PostgreSQL,
sent to the frontend, included in a backup, printed by a launcher, or committed
to Git. With the feature disabled or the key absent, import, deterministic
analysis, comparison, and export remain available; only the AI action is
disabled with a visible external-gate reason.

## Exact data boundary

The browser sends only the internal analysis ID, the displayed aggregate-input
SHA-256, and an explicit confirmation boolean. The backend reloads the
deterministic result and builds a second closed provider DTO.

Eligible provider input is limited to:

- reporting period, marketplace dimension, report type, granularity, parser
  version, freshness, timezone, currency, and bounded missing-field labels;
- allowlisted catalog aggregates: revenue, units, sessions, page views,
  conversion/unit-session percentage, Buy Box, B2B shares/totals, and the
  retained aggregate inventory metrics;
- allowlisted period values, absolute and percentage delta, trend, and anomaly
  class;
- deterministic uncertainty, missing-data/evidence statements, and open
  questions authored by the server;
- semantic references such as `fact:sessions` or
  `change:ordered_product_sales`.

The request never contains raw report bytes or rows, filenames or paths,
archive/report hashes, database UUID evidence references, ASIN/SKU, seller
secrets, buyer/customer/order PII, free-form report content, or browser tokens.
Input is capped at 128 KiB. Tests intercept the exact outbound request and the
repository transport contract permits only the fixed
`https://api.openai.com/v1/responses` endpoint with one POST client.

## Provider request

The backend uses the OpenAI Responses API with:

- no tools, files, conversations, background execution, or configurable base
  URL;
- redirects disabled and a 60-second timeout;
- `store: false`;
- medium reasoning and a bounded output budget;
- a strict JSON Schema for summary, assessment, opportunities, risks,
  hypotheses, possible actions, open questions, and limitations;
- a one-way pseudonymous safety identifier derived from the internal user ID.

OpenAI API data is not used for model training by default. `store: false`
disables endpoint application-state retention, but it must not be described as
zero retention: standard abuse-monitoring logs can be retained for up to 30
days unless the organization's separately approved data controls say
otherwise. See the official [data controls](https://developers.openai.com/api/docs/guides/your-data)
and [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
documentation.

## Validation and persistence

The model output is untrusted. The backend accepts it only when it matches the
strict typed schema, all counts and strings remain within bounds, and every
evidence reference exists in the transmitted aggregate DTO. Refusals,
incomplete responses, invalid JSON/schema, oversized responses, authentication
errors, timeouts, 429s, and provider failures create no partial assessment.
There is no automatic retry or scheduler.

Successful output is stored immutably in
`amazon_ai_strategy_assessments`, keyed by analysis, aggregate payload hash,
model, and prompt version. Repeating the same request returns that record
without another provider call. Only the validated structured result, redacted
request-reference hash, token counts, creator, and timestamps are stored. The
prompt, provider request body, and raw provider response are not persisted or
logged. Backups include the validated assessment but explicitly exclude API
keys, prompts, and raw provider responses.

## Operator workflow

1. Import one official Sales and Traffic report and review the deterministic
   facts, or import a compatible second period for a comparison.
2. Review the AI panel's aggregate-input SHA-256 and privacy boundary.
3. Confirm the one-time aggregate transmission.
4. Trigger the assessment manually.
5. Read AI content only inside its visibly labelled block. Facts and supported
   deterministic derivations remain separate above it.
6. Validate hypotheses with the named missing evidence before acting.

The output is advice for a human decision. No OpenAI response can invoke or
reach Amazon price, Ads, listing, inventory, order, payment, shipping, or any
other mutation.

## API surface

- `GET /api/marketplace/strategy/status` returns availability, reason, model,
  prompt version, storage mode, and immutable capability flags; never a key or
  secret shape.
- `GET /api/marketplace/analyses/{id}/strategy` returns the current aggregate
  hash and an exact cached assessment when one exists.
- `POST /api/marketplace/analyses/{id}/strategy` requires administrator role,
  the current hash, and explicit aggregate-only confirmation.

The read-only pilot middleware permits only that exact strategy POST path and
rejects near-miss paths. The Amazon Reports transport enum and its five-operation
allowlist are unchanged.
