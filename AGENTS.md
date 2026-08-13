# Repository guide

## Product boundary

- This repository is Shop Suite, project 2 of 3. Freelancer time tracking belongs in
  `Freelancer`; mail, files, office, and team communication belong in `Workspace Suite`.
- The Core remains a focused Rust/Axum/sqlx ERP and inventory application with a React/Vite
  administration frontend. It is not an ERPNext/Frappe fork or a multi-tenant SaaS platform.
- The visible product name is `Shop Suite`.
- Preserve existing internal `erplite` database, volume, crate, token-storage, and migration names.
  Renaming them is a separate compatibility-sensitive data migration.
- Shop Suite Core owns SKU, ERP master data, available stock, imported orders, invoices, and
  accounting.
- Vendure owns merchandising, facets/categories, cart, checkout, promotions, payment, and Shop API.
- The Storefront talks only to the Vendure Shop API.
- Never share tables or a database between Core and Vendure.
- The implemented commerce slice is product/price/stock projection, test checkout, idempotent paid
  order import with stock booking, and fulfillment/tracking projection. Production providers,
  correction invoices, and reference-tested DATEV EXTF remain future work.

## Engineering rules

- Keep Core migrations additive and preserve existing data and functions.
- Integration writes cross-system intent to an outbox in the local database transaction. Consumers
  must be idempotent and safe after worker restarts; do not imply distributed transactions.
- Keep money in decimal or integer minor units, sent invoices immutable, and stock changes atomic.
- Keep Vendure and Node versions pinned. Generate explicit Vendure migrations; never enable schema
  synchronization.
- Keep secrets in `.env`; `.env.example` contains placeholders only.
- Run the Rust, frontend, commerce, Docker, and vertical checks documented in `README.md` after
  relevant changes.
- Use English in repository code and documentation. German is expected in customer-visible UI and
  example commerce data.
