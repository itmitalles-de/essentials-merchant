# Repository agent guide

## Product boundary

This repository is Merchant, an Essentials Plus product. It combines a focused Rust ERP/inventory Core
with a separate Vendure commerce subsystem. The repository is persistent
project memory; the current chat or agent session is temporary working memory.

Freelancer time tracking belongs in `Freelancer`. Files, mail, office, and team
communication belong in `Workspace Suite`. Do not turn this project into a
multi-tenant SaaS platform or migrate the ERP Core into Vendure.

## Startup

1. Inspect `git status` and preserve all existing worktree changes.
2. Read `.agent/STATE.md` for the current verified repository state.
3. Read `.agent/TODO.md` when continuing existing work.
4. Read `.agent/DECISIONS.md` or `.agent/ARCHITECTURE.md` only when relevant.
5. Inspect recent relevant commits and the specific implementation area needed.
6. Check current CI and open pull requests before relying on an old handoff claim.

Use `README.md` as the authoritative operational and validation guide. Read
`docs/CODEX_PROMPT.md` only for the original staged commerce brief; completed
steps there are historical requirements, not current tasks.

## Source-of-truth boundaries

- Merchant Core owns SKU, ERP master data, available stock, imported orders,
  invoices, and accounting data.
- Vendure owns merchandising, facets/categories, cart, checkout, promotions,
  payment state, and Shop/Admin APIs.
- The Storefront uses only the Vendure Shop API.
- Core and Vendure have separate PostgreSQL databases. Never share tables.
- Preserve internal `erplite` database, volume, crate, token-storage, and
  migration names; renaming them requires an explicit compatibility migration.

## Essentials Plus module contract

- The Admin-Center groups the module catalog by product area. Administrators see
  the complete catalog; normal users see only enabled modules with an explicit
  permission.
- Optional modules must check their enabled state at the API boundary and before
  claiming jobs or accepting webhooks. Disabling a module stops navigation,
  jobs, and webhooks but never deletes its data.
- Marketplace Intelligence is an optional read-only module. It must never use
  Amazon feeds, listings, orders, prices, advertising, or inventory write APIs.
- DHL, DPD, and future carrier integrations are independent connector modules.
  They require configuration validation and a healthcheck, and must not be
  coupled to Marketplace Intelligence.

## Data and accounting invariants

- Keep money in `Decimal` or integer minor units; never use binary floating point.
- Draft invoices may change. Sent invoices are immutable snapshots with stable,
  unique numbering; corrections require an explicit correction flow.
- Stock movements and their aggregate stock update must remain atomic.
- Imported external events and orders must remain idempotent and book stock once.
- Cross-system writes use local transactional outboxes. Consumers are at-least-
  once, restart-safe, and idempotent; do not claim a distributed transaction.
- Keep migrations additive and preserve existing data and functions.

## Security and operations

- Never read, print, or commit the local `.env`; `.env.example` is placeholders only.
- Keep integration traffic on private/TLS-protected networks outside local Compose.
- The test payment and manual fulfillment are not production providers.
- Keep Vendure and Node versions pinned, `synchronize: false`, and schema changes
  in reviewed explicit migrations.
- Back up Core DB, Vendure DB, invoices, and Vendure assets as separate stores
  with matching application versions.

## Context hygiene

- Use targeted `rg`, narrow file reads, and scoped tests before broad builds.
- Do not load all migrations, generated Vendure schema, Storefront build output,
  dependency trees, or integration implementations by default.
- Avoid giant log dumps and rereading large files when focused excerpts suffice.
- Use isolated or subagent investigations, where supported, for large independent
  Core, Vendure, or Storefront explorations.
- Summarize durable findings in `.agent/` rather than preserving them only in chat.
- Use English in code and repository documentation. German remains appropriate
  for customer-visible UI and example commerce data.

## Validation

Run the relevant commands from `README.md`: Rust format/Clippy/tests, frontend
build/lint, commerce lint/tests/build, and the Compose vertical flow. Run scoped
checks first. Refresh and verify `backend/.sqlx` after Core migrations or checked
SQL changes. Never point migration generation or SQLx preparation at production.

## Handoff

Before ending substantial work:

1. Validate the changed scope and record exactly what ran.
2. Update `.agent/STATE.md` with concise verified reality.
3. Update `.agent/TODO.md`, the authoritative repository task handoff.
4. Record durable decisions only when one was actually made.
5. Update architecture only when implemented boundaries or data flow changed.

Assume the next session has no useful memory of the current conversation.

When visible context use reaches roughly 50-70%, prefer a coherent stopping
point, validate, update the handoff, and continue in a fresh session. Do not stop
halfway through an atomic change solely to meet that guideline.

For an unspecified continuation request, read state and TODO, inspect Git status
and recent relevant commits, then continue the highest-priority unfinished task
without redoing completed work.
