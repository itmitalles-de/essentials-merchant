# Mantle Amazon AI strategy gate

## Purpose

The optional weekly strategy panel turns the available deterministic Sales and
Traffic analyses into a German-language decision aid. It does not parse
reports, create facts, replace the deterministic comparison, or introduce
another analysis system. An administrator starts it with the single `Analyse`
button; a successful result closes the Europe/Berlin calendar week.

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

The browser sends only the displayed aggregate-input SHA-256 and an explicit
aggregate-only confirmation boolean when the operator clicks `Analyse`. The
backend reloads the newest deterministic results, reduces them through a second
closed provider DTO, removes duplicates, and retains at most eight newest-first
analysis documents. It also adds the previous immutable, validated AI result as
untrusted continuity context. No browser-supplied report or analysis body is
accepted.

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
- the preceding validated AI summary, findings, questions, actions, and
  handover, with old evidence references removed;
- semantic references such as `analysis:1:fact:sessions` or
  `analysis:2:change:ordered_product_sales`.

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
  hypotheses, possible actions, open questions, limitations, and the fixed
  handover sections;
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
There is no automatic retry or scheduler. A failed provider call does not close
the weekly window.

Successful output is stored immutably in
`amazon_ai_strategy_assessments`. A partial unique index permits exactly one
non-legacy row per Europe/Berlin calendar-week start. The row references the
anchor deterministic analysis and, when available, its previous AI assessment;
the validated result itself always contains a continuity summary, priorities,
evidence to collect, and checks for the next run. Repeating a request after the
weekly result exists returns the stored row without another provider call. Only
the validated structured result, aggregate hash, redacted request-reference
hash, token counts, creator, week, previous-row reference, and timestamps are
stored. The prompt, provider request body, and raw provider response are not
persisted or logged. Backups include the validated assessment but explicitly
exclude API keys, prompts, and raw provider responses.

## Operator workflow

1. Import one official Sales and Traffic report and review the deterministic
   facts, or import a compatible second period for a comparison.
2. Review the fixed KPI cards, comparison bars, aggregate-input SHA-256, input
   count, and previous-run status.
3. Click `Analyse`. The click confirms this one aggregate-only request.
4. Read the fixed output sections below the deterministic analysis: summary,
   assessment, chances, risks, hypotheses, possible actions, open questions,
   limitations, and handover.
5. Validate hypotheses with the named missing evidence before acting.
6. Until the following Monday 00:00 Europe/Berlin, the button remains disabled
   and the stored result remains visible. New imports are marked as not yet
   covered by the assessed hash.

The output is advice for a human decision. No OpenAI response can invoke or
reach Amazon price, Ads, listing, inventory, order, payment, shipping, or any
other mutation.

## API surface

- `GET /api/marketplace/strategy/status` returns availability, reason, model,
  prompt version, storage mode, and immutable capability flags; never a key or
  secret shape.
- `GET /api/marketplace/strategy/weekly` returns the current aggregate hash,
  weekly availability, next eligible time, input count, previous-run context
  flag, and latest validated assessment.
- `POST /api/marketplace/strategy/weekly` requires administrator role, the
  current hash, and aggregate-only confirmation. Once a successful weekly row
  exists, later repeat requests return it without another provider call.

The read-only pilot middleware permits only that exact strategy POST path and
rejects near-miss paths. The Amazon Reports transport enum and its five-operation
allowlist are unchanged.
