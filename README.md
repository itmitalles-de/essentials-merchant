# ErpLite

Schlanke ERP-Lösung für den deutschen Markt — Rechnungen mit korrekter Umsatzsteuer, einfache Lagerverwaltung und Buchhaltung mit DATEV-Export. Kein ERPNext/Frappe-Fork, sondern eine Neuentwicklung, gezielt auf das reduziert, was deutsche KMU tatsächlich brauchen.

**Status**: In aktiver Entwicklung. Umgesetzt sind die Grundlagen, Authentifizierung und Firmeneinstellungen, Kunden- und USt-Verwaltung, Rechnungen mit PDF-Erzeugung sowie die einfache Artikel- und Lagerverwaltung.

## Funktionen

- Kunden verwalten, Rechnungen mit Netto/USt-Satz/Brutto pro Position (19 %/7 %/0 %), USt-Aufschlüsselung nach §14 UStG
- Rechnungs-PDF-Erzeugung (Typst), fortlaufende Rechnungsnummern, Statusverfolgung (Entwurf/versendet/bezahlt/überfällig/storniert)
- Einfache Artikel- und Lagerverwaltung (Lagerbestand automatisch bei Rechnungsversand angepasst)
- Buchhaltung: automatische Buchungssätze bei Rechnungs-/Zahlungsstatus, DATEV-EXTF-CSV-Export für den Steuerberater (geplant)

## Web-Stack

- **Backend**: Rust (Axum), PostgreSQL (sqlx, versionierte Migrationen), PDF-Erzeugung via Typst
- **Frontend**: React + Vite + TypeScript, Dark Mode (System/Hell/Dunkel) + EN/DE (System/Englisch/Deutsch), beides persistiert
- **Deployment**: Docker Compose (`db`, `backend`, `frontend`)

## Setup

```bash
cp .env.example .env
# .env anpassen: POSTGRES_PASSWORD, JWT_SECRET, ADMIN_PASSWORD
docker network inspect proxy_net >/dev/null 2>&1 || docker network create proxy_net
docker compose up -d --build
```

Danach ist die App unter `http://localhost:8090` erreichbar (Port über `FRONTEND_PORT` in `.env` konfigurierbar).

### Lokale Entwicklung ohne Docker

```bash
# Backend
cd backend
cargo run -p server   # benötigt DATABASE_URL als Env-Var (Postgres), weitere Vars folgen ab Phase 2 (Auth)

# Frontend
cd frontend
npm install
npm run dev
```

## Architekturentscheidungen

- Single-Tenant: eine Installation pro Kunde (wie das freelancer-Projekt), keine gemeinsam gehostete SaaS-Instanz
- Single-User pro Installation: ein Admin-Zugang, aus `.env` geseedet, kein Multi-Tenant innerhalb einer Installation
- Geldbeträge grundsätzlich als `rust_decimal::Decimal`, nie als Fließkommazahl
- Gesendete Rechnungen sind unveränderlich; Korrekturen erfolgen künftig über eine Stornorechnung (noch nicht implementiert)
- DATEV-Export ist ein isolierter Baustein (Buchungssätze → EXTF-CSV), entkoppelt vom Buchhaltungs-Domain-Modell — Format muss vor Produktivnutzung gegen die aktuelle DATEV-Formatbeschreibung verifiziert werden
- Kontenrahmen: SKR03-Subset als Standard (Company-Setting), SKR04 nicht im MVP
