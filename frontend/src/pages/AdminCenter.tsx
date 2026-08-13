import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import type { ConnectorHealth, EssentialsModule } from "../types";

export function AdminCenter() {
  const [modules, setModules] = useState<EssentialsModule[]>([]);
  const [health, setHealth] = useState<Record<string, ConnectorHealth>>({});
  const [error, setError] = useState<string | null>(null);

  const reload = () => {
    api.get<EssentialsModule[]>("/modules").then(setModules).catch((err: Error) => setError(err.message));
  };
  useEffect(reload, []);

  const groups = useMemo(() => {
    return modules.reduce<Record<string, EssentialsModule[]>>((accumulator, module) => {
      (accumulator[module.module_group] ??= []).push(module);
      return accumulator;
    }, {});
  }, [modules]);

  const toggle = async (module: EssentialsModule) => {
    setError(null);
    try {
      await api.putWithHeaders(`/modules/${module.module_id}`, { enabled: !module.enabled }, {
        "idempotency-key": `module-${module.module_id}-${crypto.randomUUID()}`,
      });
      reload();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Modul konnte nicht geändert werden.");
    }
  };

  const checkHealth = async (moduleId: string) => {
    try {
      const result = await api.get<ConnectorHealth>(`/modules/${moduleId}/health`);
      setHealth((current) => ({ ...current, [moduleId]: result }));
      reload();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Healthcheck konnte nicht ausgeführt werden.");
    }
  };

  return (
    <div style={{ display: "grid", gap: "1rem", maxWidth: 900 }}>
      <div className="card">
        <h1>Essentials+ Merchant · Admin-Center</h1>
        <p>
          Module sind thematisch gruppiert. Deaktivieren entfernt die Navigation und stoppt zugehörige
          Jobs sowie Webhooks, ohne vorhandene Daten zu löschen.
        </p>
        {error && <p style={{ color: "var(--danger)" }}>{error}</p>}
      </div>
      {Object.entries(groups).map(([group, entries]) => (
        <section className="card" key={group}>
          <h2>{group}</h2>
          <div style={{ display: "grid", gap: "0.75rem" }}>
            {entries.map((module) => (
              <div
                key={module.module_id}
                style={{ display: "flex", gap: "1rem", alignItems: "center", justifyContent: "space-between" }}
              >
                <div>
                  <strong>{module.display_name}</strong>
                  <div style={{ color: "var(--fg-muted)", fontSize: "0.9rem" }}>
                    {module.module_id} · v{module.version} · {stateLabel(module.state)}
                  </div>
                  <div style={{ color: "var(--fg-muted)", fontSize: "0.85rem" }}>
                    {module.module_kind === "optional" ? "Optionales Modul" : module.module_kind === "connector" ? "Connector-Modul" : "Kernmodul"}
                    {module.dependencies.length > 0 ? ` · benötigt ${module.dependencies.join(", ")}` : ""}
                  </div>
                  {health[module.module_id] && (
                    <div style={{ color: "var(--fg-muted)", fontSize: "0.85rem" }}>
                      Health: {health[module.module_id].health_status} · {health[module.module_id].message}
                    </div>
                  )}
                </div>
                <div style={{ display: "flex", gap: "0.5rem", alignItems: "center" }}>
                  {module.module_kind === "connector" && (
                    <button className="secondary" onClick={() => checkHealth(module.module_id)}>
                      Konfiguration prüfen
                    </button>
                  )}
                  <button className="secondary" onClick={() => toggle(module)} disabled={module.required || module.state === "not_installed"}>
                    {module.enabled ? "Deaktivieren" : "Aktivieren"}
                  </button>
                </div>
              </div>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}

function stateLabel(state: EssentialsModule["state"]) {
  return ({
    not_installed: "nicht installiert",
    needs_configuration: "Konfiguration erforderlich",
    disabled: "deaktiviert",
    enabled: "aktiviert",
    degraded: "beeinträchtigt",
  } as const)[state];
}
