# Historical Codex prompt: Essentials+ Merchant including Vendure

This is the original, completed Vendure brief. Current work must follow `AGENTS.md`,
`.agent/STATE.md`, and `.agent/TODO.md`; do not repeat its completed CI or Vendure steps.
The current visible product is **Essentials+ Merchant**. The repository slug and all existing
`erplite` compatibility identifiers remain unchanged.

The reliability, module-contract, correction-invoice, deterministic Marketplace Intelligence,
backup/restore, provider-port, and guarded DATEV export work that followed this historical brief is
documented in `README.md`, `docs/FAILURE_MATRIX.md`, `docs/VERIFICATION_MATRIX.md`, and
`.agent/STATE.md`. Do not repeat completed Vendure or SQLx work from this prompt.
The repository slug and internal compatibility identifiers remain `erplite`.

## Ziel

Erzeuge einen belastbaren vertikalen Commerce-Kern: Ein Artikel aus Essentials+ Merchant wird in Vendure angeboten, eine Testbestellung wird genau einmal in Essentials+ Merchant importiert, Bestand und Auftragsstatus bleiben konsistent und der Versandstatus kann zurückgespielt werden.

## Reihenfolge

### 1. Hauptzweig stabilisieren

- Reproduziere den aktuellen Backend-CI-Fehler. Der beobachtete Fehler entsteht bei `cargo clippy`, weil SQLx-Query-Makros gegen eine leere, nicht migrierte CI-Datenbank laufen; Frontend- und Docker-Jobs sind grün.
- Behebe die Ursache minimal. Nutze entweder den bereits eingecheckten SQLx-Offline-Cache korrekt oder migriere die CI-Datenbank reproduzierbar vor Compile/Test. Wähle eine konsistente Variante und dokumentiere, wie der Cache nach Schemaänderungen aktualisiert wird.
- Führe Format, Clippy, Tests, Frontend-Build/Lint und Compose-Build aus. Erst nach grüner CI weitermachen.

### 2. Produktname konsolidieren

- Sichtbare Bezeichnungen müssen `Essentials+ Merchant` heißen.
- Interne DB-, Volume-, Crate- und Migrationsnamen `erplite` nicht mechanisch ändern. Das wäre eine getrennte Datenmigration.
- Aktualisiere Dokumentation und Beispiele ohne bestehende Deployments zu brechen.

### 3. Vendure als klar getrenntes Subsystem hinzufügen

- Verwende die aktuelle freigegebene Vendure-3.7.x-Version, Node 22 oder 24, PostgreSQL, separaten Server und Worker sowie das aktuelle Vendure Dashboard.
- Lege das Subsystem nachvollziehbar im Monorepo ab, zum Beispiel unter `commerce/`; keine zweite lose Repository-Schattenkopie.
- Verwende eine eigene Vendure-Datenbank. Keine Tabellenfreigabe mit dem Rust-Core.
- Ergänze eine Next.js-Storefront, zunächst mit einem kleinen deutschen Beispieldatensatz und Fake-/Testzahlung.
- Secrets nur über `.env`; sichere `.env.example`; keine Default-Superadmin-Zugangsdaten in einer öffentlichen Umgebung.

### 4. Integrationsvertrag implementieren

Verantwortlichkeiten:

- Essentials+ Merchant Core: SKU, ERP-relevante Stammdaten, verfügbarer Bestand, importierter Auftrag, Rechnung, Buchhaltung.
- Vendure: Merchandising, Kategorien/Facetten, Warenkorb, Checkout, Aktionen, Zahlung, Shop API.
- Storefront: ausschließlich Vendure Shop API.

Baue einen kleinen expliziten Adapter:

1. SKU/Preis/Bestandsprojektion Core → Vendure.
2. Mapping zwischen internen UUIDs und Vendure-IDs.
3. Import einer bezahlten/autorisierten Vendure-Bestellung → Core.
4. Idempotency-Key pro externem Auftrag/Event.
5. Outbox und Retry-Verhalten; keine verteilte Transaktion vortäuschen.
6. Fulfillment-/Tracking-Status Core → Vendure.

Teste mindestens doppelte Events, verspätete Events, Neustart während Verarbeitung und temporär nicht erreichbares Zielsystem.

### 5. Erst danach fachlich erweitern

- DATEV-EXTF aus Buchungssätzen
- Storno-/Korrekturrechnung
- Payment- und Versandprovider
- Versandlabel
- Marktplatzadapter
- B2B-Preislisten/Channels

## Nicht tun

- Keine Vollmigration zu Vendure als ERP.
- Keine gemeinsame Datenbank.
- Kein Rewrite des Rust-Cores.
- Kein Multi-Tenant-SaaS.
- Keine gleichzeitige Entwicklung von fünf Integrationen.
- Keine rechtlichen oder DATEV-Kompatibilitätsbehauptungen ohne verifizierbare Tests/Referenzformat.

## Fertig wenn

- Gesamte CI grün ist.
- Ein reproduzierbarer Docker-Compose-Start Core, Vendure, Worker, Storefront und Datenbanken hochfährt.
- Der vertikale Test SKU → Storefront → Testcheckout → genau-einmal-Import → Bestandsbuchung → Versandstatus besteht.
- README, Architektur und Betriebsanleitung den tatsächlichen Zustand beschreiben.
- Der Abschlussbericht enthält Tests, bekannte Grenzen, Datenmigrationsrisiken und die drei nächsten sinnvollen Schritte.
