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
      await api.put(`/modules/${module.module_key}`, { enabled: !module.enabled });
      reload();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Modul konnte nicht geändert werden.");
    }
  };

  const checkHealth = async (moduleKey: string) => {
    try {
      const result = await api.get<ConnectorHealth>(`/modules/${moduleKey}/health`);
      setHealth((current) => ({ ...current, [moduleKey]: result }));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Healthcheck konnte nicht ausgeführt werden.");
    }
  };

  return (
    <div style={{ display: "grid", gap: "1rem", maxWidth: 900 }}>
      <div className="card">
        <h1>Essentials Plus · Admin-Center</h1>
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
                key={module.module_key}
                style={{ display: "flex", gap: "1rem", alignItems: "center", justifyContent: "space-between" }}
              >
                <div>
                  <strong>{module.display_name}</strong>
                  <div style={{ color: "var(--fg-muted)", fontSize: "0.9rem" }}>
                    {module.module_kind === "optional" ? "Optionales Modul" : module.module_kind === "connector" ? "Connector-Modul" : "Kernmodul"}
                  </div>
                  {health[module.module_key] && (
                    <div style={{ color: "var(--fg-muted)", fontSize: "0.85rem" }}>
                      Health: {health[module.module_key].health_status} · {health[module.module_key].message}
                    </div>
                  )}
                </div>
                <div style={{ display: "flex", gap: "0.5rem", alignItems: "center" }}>
                  {module.module_kind === "connector" && (
                    <button className="secondary" onClick={() => checkHealth(module.module_key)}>
                      Konfiguration prüfen
                    </button>
                  )}
                  <button className="secondary" onClick={() => toggle(module)} disabled={module.module_kind === "core"}>
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
