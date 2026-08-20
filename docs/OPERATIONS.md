# Essentials+ Merchant operations

All destructive rehearsals in this document are designed for disposable synthetic environments.
Never load a real `.env` into a rehearsal, never generate migrations against production, and never
restore over an existing Compose project.

## Amazon Intelligence pilot startup

The standalone `compose.amazon-pilot.yml` contains exactly `db`, `backend`, and `frontend`. It does
not define or start Vendure, Storefront, payment, shipping, carrier, or DATEV services. The fixed
Compose project is `essentials-merchant-amazon-pilot`.

`scripts/start-amazon-pilot.sh --env-file .env.amazon-pilot` defaults to configuration validation.
Add `--start` only for an explicitly prepared local environment. After startup, the script queries
the persisted pilot status and fails closed if the exact module allowlist differs, a schedule is
enabled, or a forbidden service is running. It never deletes data or prints secret values.

## Health and readiness

- `GET /api/health` reports process health.
- `GET /api/readiness` verifies that the Core database is reachable.
- Vendure and Storefront have Compose healthchecks.
- `GET /api/integration-diagnostics` is administrator-only and combines Core queues, redacted
  Vendure observations, lease state, mappings, and recent administrative audit events.

Errors in diagnostics are normalized to one line and 512 characters. Full event payloads, buyer
data, credentials, HMAC values, OAuth tokens, and provider responses are never returned.

## Integration key rotation

The current Core/Vendure HMAC pair is `INTEGRATION_KEY_ID` plus `INTEGRATION_SECRET`. For a
coordinated rotation:

1. Deploy the existing pair as `INTEGRATION_PREVIOUS_KEY_ID` and
   `INTEGRATION_PREVIOUS_SECRET`, and deploy the new pair as current to both systems.
2. Verify readiness and delivery with the new current key. Core accepts both key IDs while the
   overlap exists; Vendure signs with current only.
3. After the maximum request/retry window, remove both previous-key variables from both systems.

Never log either secret. A nonce is accepted only once, and timestamps outside the configured
clock-skew window are rejected, so host clocks must be synchronized.

## Dead-letter recovery

Use the Admin-Center integration view. A requeue action is available only for a dead event and
requires a unique idempotency key. Core records actor, action, target, previous state, and timestamp
in immutable administrative audit history. Repeating the same request returns the earlier result.

Do not edit inbox/outbox rows manually. Diagnose and remove the underlying schema, data, target, or
configuration failure first. Requeue cannot bypass a disabled module.

## Coordinated backup

The backup covers:

- Core PostgreSQL, including module configuration without secrets, mappings, inbox/outbox,
  invoices, corrections, accounting entries, Marketplace raw reports and normalized data;
- Vendure PostgreSQL;
- Core invoice/document volume;
- Vendure asset volume;
- secret-redacted Compose topology metadata.

The script requires an explicit project and a target path that does not exist. It verifies all
required services, quiesces Core and Vendure writers, dumps both databases, archives both file
stores, emits module configuration without secret values, records schema/app versions and Git
revision, calculates SHA-256 and byte length for every data file, and resumes services through a
trap on success or failure.

```bash
COMPOSE_PROJECT_NAME=merchant-production \
COMPOSE_ENV_FILE=/secure/path/to/runtime.env \
ops/backup.sh /secure/new/path/merchant-backup-YYYYMMDD
```

Store the resulting directory and manifest with the same access control and retention as the
underlying business data. A checksum proves integrity, not confidentiality; encrypt backup storage
and transport outside this repository.

## Empty-project restore

`ops/restore.sh` validates the manifest and every checksum before creating anything. It rejects a
target project if any Compose container or declared volume already exists. It restores the two
databases and file volumes, then starts the whole stack.

```bash
COMPOSE_PROJECT_NAME=merchant-isolated-restore \
COMPOSE_ENV_FILE=/secure/path/to/restore.env \
ops/restore.sh /secure/path/merchant-backup-YYYYMMDD
```

Post-restore acceptance must include readiness, queue diagnostics, schema versions, document/asset
sampling, and the complete vertical test with synthetic data. DNS, ports, secrets, and object-store
credentials are deployment concerns and are intentionally not restored from metadata.

## Automated backup/restore rehearsal

The repository-owned rehearsal generates random Compose project names and synthetic secrets,
starts a fresh source stack, runs the vertical test, adds byte fixtures to both file volumes,
backs up, verifies, restores into a completely empty stack, compares row/file invariants, and runs
the vertical test again. Cleanup removes both projects and volumes even after failure.

```bash
ops/test-backup-restore.sh
```

The rehearsal is a local infrastructure proof. It is not proof that an external backup system,
retention policy, encryption key, disaster location, or production-sized restore meets an RPO/RTO.

## Amazon pilot backup and empty restore

`ops/backup-amazon-pilot.sh` exports the Core schema plus allowlisted pilot data only: users,
module state, audit, Amazon connections/jobs, immutable raw archives, normalized snapshots,
analyses, transport observations, and backup verifications. It archives only the `amazon-pilot`
document subtree and records file hashes, Git revision, parser versions, and declared image
digests. Its manifest explicitly excludes Amazon tokens/secrets, customer/order/invoice/payment/
shipping data, buyer data, Vendure data, and Storefront data.

Before quiescing services, the backup fails closed if a live connection contains anything other
than a constrained logical secret-reference name or if a raw archive belongs to a report type
outside the aggregate Sales & Traffic pilot. It does not silently copy a historical potential-PII
archive into a pilot backup.

`ops/restore-amazon-pilot.sh` refuses a non-empty destination project. The automated rehearsal
seeds a report larger than 2 MiB, restores into an empty project, and compares report inventory,
raw hashes, parser/snapshot/analysis fingerprints, audit, exact module state, schedule state, and
the read-only pilot profile:

```bash
ops/test-amazon-pilot-backup-restore.sh
```

## Upgrade rehearsal

`ops/test-upgrade-rehearsal.sh` starts a temporary PostgreSQL 16 container, migrates through schema
10, inserts synthetic issued invoice and Marketplace data, then migrates through the current schema.
It verifies immutable invoice/line values, module compatibility aliases/state, lossless raw archive
migration, and the expected final SQLx migration number.

```bash
ops/test-upgrade-rehearsal.sh
```

Vendure schema changes still require a restored two-database Compose rehearsal because its TypeORM
migrations and Core events are separate. Keep `synchronize: false` in every environment.

## Marketplace operations

The pilot profile enables the Marketplace Intelligence module, but no acquisition runs without an
administrator-created connection and explicit manual request. Automatic schedules remain disabled.
The synthetic demo uses `fixture:*` references and never accesses the environment secret mechanism.
A live secret reference resolves only server-side from
`AMAZON_SECRET_<NORMALIZED_REFERENCE>` and contains LWA refresh token, client ID, and client secret.

The required external staging gate is:

1. approved seller and app authorization;
2. correct `na`/`eu`/`fe` endpoint and marketplace participation;
3. assigned role for the selected non-restricted report;
4. one manual report through request, polling, document download, archive, parser, and analysis;
5. observed 429/retry headers and data freshness;
6. proof that logs, frontend, diagnostics, and analysis export contain no token or buyer PII.

No live report type requiring RDT may be enabled until its registry classification and minimized
storage/retention design are separately reviewed.

The first real request is specified in `docs/operations/AMAZON_STAGING_GATE.md`. Its command
defaults to validation and requires an ignored approval file, ignored environment file, encrypted
archive attestation, exact seller/region/marketplace approval, and a manual `--execute`. No
automatic scheduler is used. Until those external facts are provided, the gate is `BLOCKED` and no
fixture or local run may be described as Amazon staging.

## Mantle Amazon live deployment

The Mantle service uses Compose project `essentials-merchant-amazon` and
`compose.mantle-amazon.yml`. Its service allowlist is exactly `db`, `backend`,
and `frontend`. Backend and frontend image tags must equal the full deployed Git
SHA; the PostgreSQL image is pinned by digest. The frontend binds only to
`127.0.0.1:18090` by default and must be published through an existing Caddy
route restricted to private/LAN/VPN source ranges.

Before every deployment, capture without rendering environment values:

- `docker compose ls`;
- all container IDs, image IDs, creation times, status, and restart counts;
- mounts belonging to the target project;
- `df -h` and `docker system df`;
- current images for the three target services;
- the active Caddy route and its container/process ID;
- other running build, pull, update, or deployment processes.

Do not continue while another deployment is changing the target project. Never
use `docker compose down -v`, `docker system prune`, a restart of all containers,
or a blanket Caddy restart. Only `essentials-merchant-amazon` resources may be
changed. Validate Caddy configuration first, then use its graceful reload
mechanism; compare all non-target container IDs and restart counts afterwards.

The private `.env.mantle-amazon` must have mode `0600` and a
`MERCHANT_GIT_SHA` equal to the checked-out commit. It deliberately has no
placeholder Amazon credential. Configuration validation is the default:

```bash
scripts/start-mantle-amazon.sh --check --env-file .env.mantle-amazon
```

An explicitly authorized deployment uses:

```bash
scripts/start-mantle-amazon.sh --start --env-file .env.mantle-amazon
```

The script builds only the allowlisted images, waits for health, checks the
persisted module allowlist, and verifies that automatic Amazon schedules equal
zero. It stops only the target application services if the profile fails closed.

### Live backup

Create a new host-restricted directory and run the pilot backup against the live
Compose file:

```bash
COMPOSE_PROJECT_NAME=essentials-merchant-amazon \
COMPOSE_ENV_FILE=.env.mantle-amazon \
PILOT_COMPOSE_FILE=compose.mantle-amazon.yml \
ops/backup-amazon-pilot.sh /secure/new/path/mantle-amazon-backup-YYYYMMDD
```

The backup briefly stops only this project's backend and frontend; PostgreSQL
stays up. The trap restarts those two services even if backup validation fails.

### Empty-target restore acceptance

Use a never-before-used project name and an unused loopback port. The restore
script refuses existing containers or volumes and never overwrites production:

```bash
COMPOSE_PROJECT_NAME=essentials-merchant-amazon-restore-YYYYMMDD \
COMPOSE_ENV_FILE=.env.mantle-amazon \
PILOT_COMPOSE_FILE=compose.mantle-amazon.yml \
RESTORE_FRONTEND_PORT=18091 \
ops/restore-amazon-pilot.sh /secure/path/mantle-amazon-backup-YYYYMMDD
```

Acceptance compares raw SHA-256, run/snapshot/metric/analysis counts, parser
versions, module state, zero schedules, and HTTP readiness. Retain or remove the
isolated restore project only under a separate, explicit data-destruction
decision; the restore procedure itself does not delete volumes.

### Synthetic live acceptance

The first live data path uses two in-memory JSON comparison reports plus one
CSV and one TSV probe, all marked as synthetic. It must verify the first import
is idempotent, produce a comparison, and generate JSON, Markdown, and CSV
exports without writing raw report bytes to disk. Record only hashes, run IDs,
aggregate test values, export hashes, and the deployed Git/image IDs.

Run `scripts/verify-manual-amazon-import.mjs` with the internal base URL and
administrator credentials supplied only through its documented environment
variables. The script imports the newer JSON period first, verifies an
idempotent retry, creates the comparison after the older period, imports the
CSV/TSV probes, hashes all three summary formats, and confirms that raw download
and business mutations are blocked. It never writes report bytes or credentials
to disk or stdout.

An authorized real report may be imported once after this acceptance. Do not
record its local path, raw bytes, ASIN/SKU values, or business metrics in Git or
deployment logs. If authorization cannot be proven, stop after the synthetic
run.

## External validation gates

- Amazon: no approved seller credentials, roles, or marketplace participation were supplied; no
  real request has run.
- Stripe, payment webhooks, DHL, DPD, and carrier labels: ports/fakes are retained, but all adapters
  and account work are frozen until after a successful Amazon pilot.
- DATEV: retained renderer stays disabled; checking-program/test-client work is frozen for this
  milestone.
- Vendure: retained at 3.7.2 and not started by the pilot; known transitive advisories remain open
  and are individually triaged in `docs/security/VENDURE_ADVISORIES.md`.
