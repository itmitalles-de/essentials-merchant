# Claude Code guide

Read `AGENTS.md` first. It defines product ownership, accounting invariants,
security boundaries, validation expectations, and the handoff workflow.

For continuation work:

1. Inspect `git status`.
2. Read `.agent/STATE.md`.
3. Read `.agent/TODO.md`.
4. Inspect recent relevant commits and current CI.

Demand-load when relevant:

- `.agent/DECISIONS.md` for durable ownership and compatibility choices
- `.agent/ARCHITECTURE.md` for component and data-flow navigation
- `README.md` for exact setup, SQLx, migration, and validation commands
- `docs/CODEX_PROMPT.md` only for original commerce acceptance requirements

Important caveats:

- Core and Vendure are separate systems of record with separate databases.
- Preserve compatibility-sensitive internal `erplite` names.
- Treat cross-system delivery as at-least-once and consumers as idempotent.
- Sent invoices are immutable; money and VAT calculations use decimal/minor units.
- Never read, print, or commit local `.env` values.
- Do not load generated Vendure migrations, dependency trees, or every integration
  implementation unless the task specifically requires them.

Common scoped checks:

```bash
cd backend
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-targets -- -D warnings

cd ../frontend
npm run build
npm run lint

cd ../commerce
npm run lint
npm test
npm run build
```

Rust integration tests need a disposable PostgreSQL database and Typst. The full
vertical flow needs the configured Compose stack; do not claim it from static
checks alone.

Before ending substantial work, validate and update `.agent/STATE.md` and
`.agent/TODO.md`. Update decisions or architecture only when they truly changed.
