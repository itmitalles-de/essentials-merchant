# Next Agent Handoff

## Prompt

Continue Shop Suite from the committed vertical Vendure commerce slice. Read `AGENTS.md`,
`README.md`, and `docs/CODEX_PROMPT.md` first. Preserve the existing Core/Vendure database boundary
and internal `erplite` compatibility names. Before changing scope, inspect current CI and the issue
tracker. The highest-value next task is to extend failure-recovery coverage, not to add another
provider.

Before every commit, replace this handoff with the current concrete state, remaining work, blockers,
relevant files, and exact verification performed. Commit the `TODO.md` update with the code.

## Current state

- Active goal: The first Shop Suite ↔ Vendure vertical commerce core is implemented and verified.
- Completed: SQLx offline CI fix; visible Shop Suite naming; Vendure 3.7.2 server/worker and separate
  PostgreSQL database; Dashboard; Next.js Storefront; product/price/stock projection; durable
  mapping/inbox/outboxes; idempotent paid-order import and stock booking; fulfillment/tracking
  projection; full Compose and vertical CI job.
- Remaining: Add deliberate Core/Vendure outage and worker-restart cases to the vertical CI test,
  then select one production payment and one shipping provider. Correction invoices and
  reference-tested DATEV EXTF follow those reliability/provider steps.
- Blockers or decisions: Vendure 3.7.2 currently carries upstream npm production advisories. Do not
  apply npm's proposed forced downgrade; update when a compatible Vendure patch is released and
  verify the whole vertical test.
- Relevant files: `README.md`, `.github/workflows/ci.yml`, `docker-compose.yml`,
  `backend/crates/db/src/commerce.rs`, `backend/crates/db/migrations/0008_commerce_integration.sql`,
  `commerce/server/src/plugins/shop-suite-integration/`, `commerce/storefront/`,
  `commerce/test/vertical.mjs`.
- Verification: Rust fmt, offline Clippy, SQLx cache check, 23 Rust tests, frontend build/lint,
  commerce typecheck/tests/build, all Compose images, healthy clean stack, and a passing vertical
  flow proving one imported order, stock 10 → 8, and tracking projected back to Vendure.
