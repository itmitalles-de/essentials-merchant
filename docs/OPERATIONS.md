# Essentials+ Merchant operations

All destructive rehearsals in this document are designed for disposable synthetic environments.
Never load a real `.env` into a rehearsal, never generate migrations against production, and never
restore over an existing Compose project.

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

Marketplace Intelligence remains disabled until a connection is configured and an administrator
explicitly activates it. The synthetic demo uses `fixture:*` references and never accesses the
environment secret mechanism. A live secret reference resolves only server-side from
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

## External validation gates

- Amazon: no real account acceptance has run.
- Stripe and DHL: ports/fakes are implemented; no real adapter or sandbox acceptance has run.
- DATEV: renderer is disabled until checking-program and approved test-client import succeeds.
- Vendure: 3.7.2 release security fixes are present; transitive npm advisories remain monitored.
