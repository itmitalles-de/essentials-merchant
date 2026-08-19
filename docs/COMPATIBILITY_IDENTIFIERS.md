# Essentials+ Merchant compatibility identifiers

## Public identity

- Product name: **Essentials+ Merchant**.
- Repository: `itmitalles-de/essentials-merchant`.
- Public clone URL: `https://github.com/itmitalles-de/essentials-merchant.git`.

Public documentation, badges, workflow links, and support references must use that repository and
product identity. `ErpLite` and `Shop Suite` are not current public product names.

## Historical internal identifiers

Existing identifiers containing `erplite`, `shop-suite`, or `shop_suite` are intentionally retained
when they participate in code, persisted state, deployment names, or integration contracts. Current
examples include Rust crate/workspace names, the `erplite` PostgreSQL database and user, Docker
volumes such as `erplite_db_data` and `erplite_invoices`, the browser token key `erplite-token`,
`module_key` aliases, integration/plugin paths, package names, and `shop-suite-*` event or mapping
values.

These names are implementation compatibility identifiers, not product or repository branding.

## Persistence contracts

The following are migration-sensitive and are not renamed by the Amazon pilot:

- PostgreSQL database names, users, schemas, tables, migration numbers, and migration history;
- Docker volume names and backup store identifiers;
- module `module_key` aliases and existing internal IDs;
- token-storage keys, external mapping keys, idempotency keys, and event-type values;
- Rust crate/package names and existing `shop-suite-*` package/plugin compatibility values.

Backups, restores, rolling upgrades, clients, and deployed installations may depend on these exact
strings. A future rename would require an explicit versioned migration, compatibility window,
backup/restore rehearsal, and rollback plan.

## Deliberately unplanned cosmetic migration

No database, user, Docker volume, migration, token-storage, mapping, package, crate, or event rename
is planned merely to make internal strings match the current brand. The local checkout directory may
also remain named `erplite`; it is not a public repository reference and changing it provides no
runtime value.
