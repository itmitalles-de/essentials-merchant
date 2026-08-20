# Amazon staging gate

Status on 2026-08-19: **BLOCKED — external approval and credentials were not available to this repository session.** No real Amazon request was made, and no synthetic run is described as staging or production evidence.

## Only permitted first request

The first real request is one manual `GET_SALES_AND_TRAFFIC_REPORT` acquisition through Reports API v2021-06-30. It must use one explicitly approved seller, its matching SP-API region, one confirmed marketplace participation, the confirmed `Brand Analytics` role, `DAY` date granularity, `CHILD` ASIN granularity, and a completed UTC period of at most seven calendar days. The report does not require an RDT and is not a buyer/order dataset.

No scheduler may be enabled. The backend independently rejects any pilot live run that is scheduled, targets another report type, has a future/long period, has different options, lacks a shaped secret reference, or does not match `AMAZON_STAGING_APPROVAL`. A second comparable period is accepted only after a prior compatible run reached `succeeded` with parser `sales-traffic-json-v2`.

## External prerequisites

- Written seller approval, represented locally by a non-public approval reference.
- Seller Developer authorization with the Reports API and `Brand Analytics` role.
- Confirmed region and marketplace participation.
- LWA refresh token, client ID, and client secret stored only as `AMAZON_SECRET_PILOT_SELLER` in the ignored pilot environment file.
- A lowercase SHA-256 of the approved seller ID, region, and marketplace in `AMAZON_STAGING_APPROVAL`; the clear seller ID remains only in the ignored gate file.
- PostgreSQL pilot volume hosted on an operator-confirmed encrypted local filesystem. The gate records that attestation; the application does not claim to detect disk encryption itself.
- A private output location. `.amazon-staging-result.json` and the gate/environment files are ignored by Git.

No token, client secret, refresh token, clear seller ID, buyer data, full Amazon payload, ASIN, or revenue value may be copied into a commit, issue, or general PR text.

## Manual procedure

1. Copy `.env.amazon-pilot.example` to the ignored `.env.amazon-pilot`, set strong Core credentials, `AMAZON_SECRET_PILOT_SELLER`, and the matching `AMAZON_STAGING_APPROVAL` JSON.
2. Copy `.amazon-staging-gate.example.json` to `.amazon-staging-gate.json`. Fill the approved seller context, one completed short UTC period, approval reference, role/marketplace confirmations, and encrypted-storage attestation.
3. Run the default configuration check with `scripts/start-amazon-pilot.sh --check --env-file .env.amazon-pilot`.
4. Start only the named pilot project with `scripts/start-amazon-pilot.sh --start --env-file .env.amazon-pilot`.
5. Validate the external gate without making an Amazon Reports request with `scripts/request-amazon-staging-report.sh --check`.
6. After a second-person review of the local gate file, make exactly one request with `scripts/request-amazon-staging-report.sh --execute`.

The last command writes a mode-0600 local result containing UTC time, report type, marketplace, period, redacted request IDs, status, rate-limit/retry metadata, polling duration, byte counts, transport/decoded hashes, parser version, normalized-record count, missing data, freshness, and a PII-minimized analysis structure. It never downloads the raw report from the archive, prints credentials, enables a scheduler, or performs an Amazon business mutation.

## Gate evidence states

- Synthetic fixture: automated and expected to pass; no Amazon network.
- Local Compose pilot: automated and expected to pass; only `db`, `backend`, and `frontend`.
- Amazon staging: blocked until the prerequisites above exist and the manual command succeeds.
- Real seller production operation: not approved by this milestone.
- Second snapshot: blocked until the first real report has status `succeeded`; remains manual and comparable.
