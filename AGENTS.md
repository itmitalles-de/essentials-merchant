# AGENTS.md

## Produktgrenze

Dieses Repository ist **Shop Suite**, Hauptprojekt 2 von 3. Es verbindet einen fokussierten ERP-/Warenwirtschaftskern mit Vendure als Headless-Commerce-System.

- Hierher gehören Kunden, Artikel, Bestand, Aufträge, Rechnungen, Buchungssätze, DATEV sowie Shop-/Zahlungs-/Versandintegration.
- Zeiterfassung und Freelancer-Arbeitsabläufe gehören in **Freelancer**.
- Mail, Dateien, Office und Teamkommunikation gehören in **Workspace Suite**.
- Kein ERPNext-/Frappe-Fork, keine gemeinsam gehostete Multi-Tenant-SaaS-Plattform.

## Bestehende Architektur

- Core: Rust, Axum, sqlx/PostgreSQL, Typst
- Admin-Frontend: React, Vite, TypeScript
- Betrieb: Docker Compose
- Aktuell implementiert: Auth, Firmendaten, Kunden/USt, Rechnungen/PDF, Artikel/Lager, Aufträge und manuelle Erfüllung
- Noch nicht implementiert: Vendure, Storefront-Anbindung, Payment, Label, DATEV-Export

Interne Namen wie `erplite`, bestehende Datenbanknamen und Volumes bleiben erhalten, bis eine eigene getestete Migration sie ändert.

## Commerce-Grenze

- Shop Suite Core ist führend für SKU, Bestand, importierte Aufträge, Rechnungen und Buchhaltung.
- Vendure ist führend für kundenorientierten Katalog, Warenkorb, Checkout, Aktionen, Zahlungen und Shop API.
- Keine gemeinsame Datenbank und keine direkten Cross-DB-Schreibzugriffe.
- Synchronisation nur über explizite Adapter, Mappingtabellen, idempotente Events/Webhooks und Outbox.
- Ein Event muss gefahrlos erneut verarbeitet werden können.

## Arbeitsweise

1. `README.md`, Workflow, Migrationen und betroffene Module vollständig lesen.
2. CI zuerst grün machen; keine neue große Funktion auf rotem Hauptzweig.
3. Vertikale Slices statt paralleler halbfertiger Subsysteme.
4. Geld nie als Float; gesendete Rechnungen unveränderlich; Bestandsbuchungen atomar und idempotent.
5. Keine Secrets, echten Zahlungsdaten oder realen Kundendaten committen.
6. Versions- und Datenmigrationen explizit dokumentieren.

## Verifikation

- Backend: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
- Frontend: `npm ci`, `npm run build`, `npm run lint`
- Compose: `docker compose config`, `docker compose build`
- Bei Integrationsänderungen: Test von Duplikaten, Retries, Reihenfolgefehlern und Teilausfällen
- Smoke-Test: Produkt/SKU → Bestellung → Import → Bestand → Rechnung/Versandstatus

Fertig bedeutet: CI grün, Migrationen reproduzierbar, Kernfluss getestet, Dokumentation aktuell und keine unbeabsichtigte Verantwortungsverschiebung zwischen Core und Vendure.
