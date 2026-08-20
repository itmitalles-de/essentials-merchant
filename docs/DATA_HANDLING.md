# Amazon data handling

## Data classes

| Class | Examples | Handling |
| --- | --- | --- |
| Raw confidential report | Exact uploaded JSON/CSV/TSV bytes | Immutable PostgreSQL archive; backup-only access in the pilot profile; never Git. |
| Aggregate business metric | Revenue, units, sessions, page views, percentages | Decimal normalized records; internal UI and allowlisted summary export. |
| Operational metadata | SHA-256, format, report type, marketplace, period, parser version, freshness | Immutable provenance and internal diagnostics. |
| Provider secret | LWA refresh token, LWA client ID/secret, OpenAI API key | Accepted only by write-only UI endpoints, encrypted with AES-256-GCM before persistence, never returned, and excluded even as ciphertext from pilot backup data. |
| Host secret | Provider master key, JWT/admin/database secrets | Host environment only; never database, logs, exports, backup manifest, or Git. |
| PII or order data | Buyer/customer name, e-mail, phone, address, order ID | Not required, rejected by the manual importer, not requested from SP-API. |

## Raw-report controls

- Requests larger than 10 MiB are rejected before parsing.
- Only JSON, CSV, and TSV are accepted; file extension alone is not trusted.
- SHA-256 is computed from the received bytes and checked again at storage.
- Database triggers prevent raw archive update or deletion.
- Parser failure happens before the storage transaction.
- Raw download routes are blocked by the read-only pilot policy.
- Raw bytes never enter analysis results or JSON/Markdown/CSV summaries.
- `.env`, report staging paths, artifacts, and generated outputs are ignored by
  Git; the repository secret scan runs in CI and before release.

## Privacy validation

Structured JSON is accepted only for the aggregated Sales and Traffic schema.
Tabular inputs fail closed on headings that resemble buyer, recipient, customer,
address, e-mail, phone, order, payment, or free-text comment data. A filename or
local source path is not persisted; only format and byte count are retained.

The UI shows aggregate data. Evidence references point to internal snapshot and
metric IDs rather than raw rows or product/customer identifiers.

## Backup and restore

Pilot backups contain the schema, module state, users, Amazon tables (including
raw archives and manual-import provenance), parser versions, redacted Compose
metadata, image IDs, and integrity manifests. Backup directories are
confidential operational data and must use host-restricted permissions.
The backup script enforces umask `077` before creating any dump or manifest.
The `pilot_provider_secrets` table definition is restored, but its rows are
explicitly excluded. A restored stack therefore contains no OpenAI or Amazon
credential ciphertext and requires deliberate credential re-entry in the UI.

Restore is permitted only into an empty, isolated Compose project. The restore
procedure verifies manifest hashes before writing, starts the database first,
restores schema and data, then starts the backend/frontend and rechecks the
read-only module allowlist and zero automatic schedules.

No production retention deletion is implemented in the pilot. Introducing one
requires a reviewed retention policy and a deliberate replacement for the
current immutable-delete triggers.

## Optional external strategy synthesis

The live Mantle adapter is enabled, but no external AI request is possible
until an operator stores a separately billed project API key through the
write-only UI. The only eligible input is an explicitly requested,
hash-confirmed, closed aggregate DTO containing up to eight distinct newest-first
period/marketplace analyses, allowlisted metrics and deterministic deltas,
freshness, bounded missing-field labels, semantic evidence references, and the
last validated structured AI result as untrusted continuity context. Raw bytes,
raw rows, ASIN/SKU or customer identifiers, old database evidence UUIDs, local
paths, archive hashes, secrets, and free report text are prohibited.

The provider key must be project-scoped. The browser sends it once over the
internal HTTPS route; the backend encrypts it with the host-only
`PILOT_SECRETS_KEY` and the API can subsequently report only configured state,
field names, and timestamp. A legacy environment-key fallback remains for
non-Mantle deployments. Requests disable provider-side response storage.
Neither request nor response may be logged verbatim. Generated
content is untrusted strategy assistance and is stored/displayed only inside a
visibly separate AI block as assessment, hypotheses, possible measures,
uncertainty, missing evidence, open questions, and a fixed next-run handover; it
cannot alter Amazon or Merchant state. Exactly one successful row is accepted
per Europe/Berlin calendar week. The immutable database row contains only
validated structured output and redacted metadata, never the prompt or raw
provider response. See
[STRATEGY_AI_GATE.md](STRATEGY_AI_GATE.md) for the exact contract and provider
retention caveat.
