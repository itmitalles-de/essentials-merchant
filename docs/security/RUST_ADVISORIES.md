# Rust advisory triage

Review date: 2026-08-20

Scope: the locked Rust workspace under `backend/`, including every target.

`cargo audit 0.22.2` reports two vulnerable packages that Cargo retains in the
lockfile. Neither package is present in the dependency graph compiled for any
workspace target:

| Advisory | Locked package | Lockfile parent | Reachability | Decision |
|---|---|---|---|---|
| [RUSTSEC-2026-0235](https://rustsec.org/advisories/RUSTSEC-2026-0235.html) | `rkyv 0.7.46` | Optional `rust_decimal` integration | The workspace disables `rust_decimal` defaults and enables only `serde` and `std`; `cargo tree --locked --target all -i rkyv` is empty. | Temporarily ignored as unreachable. |
| [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html) | `rsa 0.9.10` | Optional `sqlx-mysql` authentication | The workspace disables SQLx defaults and enables PostgreSQL only; both `cargo tree --locked --target all -i sqlx-mysql` and `cargo tree --locked --target all -i rsa` are empty. | Temporarily ignored as unreachable. |

The exceptions live in `backend/.cargo/audit.toml`. CI first repeats all three
inverse-tree checks and fails if any affected package becomes reachable. It
then runs the pinned `cargo-audit` version, so all other current advisories
remain blocking.

Re-evaluate the exceptions whenever SQLx or `rust_decimal` features change,
when either advisory receives a fixed compatible version, or no later than
2026-09-20. An empty tree is a build-reachability statement, not a claim that
the vulnerable lockfile package is fixed.
