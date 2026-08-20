import { Link } from "react-router-dom";

import { ProviderSettingsPanel } from "../components/ProviderSettingsPanel";
import { ProductMappingPanel } from "../components/ProductMappingPanel";
import { usePilotStatus } from "../hooks/usePilotStatus";

export function PilotProviderSettings() {
  const pilot = usePilotStatus();

  return (
    <div className="marketplace-flow pilot-settings-page">
      <header className="pilot-settings-header">
        <Link to="/ai-marketing" className="pilot-settings-back">← Zur Analyse</Link>
        <h1>Einstellungen</h1>
        <p className="marketplace-muted">
          Zugänge und technische Sicherheitsgrenzen. Gespeicherte Secret-Werte bleiben unsichtbar.
        </p>
      </header>

      {pilot?.enabled && (
        <section
          className="card pilot-settings-boundary"
          role="status"
          style={{ borderColor: pilot.compliant ? "var(--success)" : "var(--danger)" }}
        >
          <strong>{pilot.compliant ? "Read-only-Systemgrenze aktiv" : "Pilotprofil nicht konform"}</strong>
          <p className="marketplace-muted">
            Keine Preis-, Ads-, Listing-, Bestands-, Bestell- oder Zahlungsänderung ist erreichbar.
          </p>
        </section>
      )}

      <ProviderSettingsPanel onConfigured={async () => undefined} />
      <ProductMappingPanel />
    </div>
  );
}
