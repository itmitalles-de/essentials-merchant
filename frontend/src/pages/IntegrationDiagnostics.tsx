import { useEffect, useState } from "react";
import { api } from "../api";
import type {
  IntegrationDiagnosticEvent,
  IntegrationDiagnostics as Diagnostics,
  IntegrationQueueSummary,
} from "../types";

export function IntegrationDiagnostics() {
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const reload = () =>
    api
      .get<Diagnostics>("/integration-diagnostics")
      .then(setDiagnostics)
      .catch((caught: Error) => setError(caught.message));

  useEffect(() => {
    void reload();
  }, []);

  const requeue = async (event: IntegrationDiagnosticEvent) => {
    setBusy(`${event.source}:${event.event_id}`);
    setError(null);
    try {
      await api.postWithHeaders(
        `/integration-diagnostics/events/${event.source}/${encodeURIComponent(event.event_id)}/requeue`,
        undefined,
        { "idempotency-key": crypto.randomUUID() },
      );
      reload();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Requeue ist fehlgeschlagen.");
    } finally {
      setBusy(null);
    }
  };

  if (!diagnostics) return <div className="card">Integrationsdiagnose wird geladen …</div>;

  return (
    <div style={{ display: "grid", gap: "1rem" }}>
      <section className="card">
        <h1>Integrationsdiagnose</h1>
        <p>
          Redigierte Betriebsdaten für Core und Vendure. Käuferdaten, Payloads, Tokens und Secrets
          werden hier nicht angezeigt.
        </p>
        <p>
          Core: {diagnostics.core_database_ready ? "bereit" : "nicht bereit"} · Vendure: {diagnostics.vendure_health}
          {diagnostics.vendure_observed_at ? ` · beobachtet ${formatTime(diagnostics.vendure_observed_at)}` : ""}
        </p>
        {error && <p style={{ color: "var(--danger)" }}>{error}</p>}
      </section>

      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit,minmax(280px,1fr))", gap: "1rem" }}>
        <QueueCard title="Core-Outbox" queue={diagnostics.core_outbox} />
        <QueueCard title="Vendure-Outbox" queue={diagnostics.vendure_outbox} />
        <section className="card">
          <h2>Core-Inbox</h2>
          <p>Verarbeitet: {diagnostics.core_inbox.completed} · Fehlgeschlagen: {diagnostics.core_inbox.failed}</p>
          <p>Letzte Verarbeitung: {formatTime(diagnostics.core_inbox.last_processed_at)}</p>
        </section>
      </div>

      <section className="card" style={{ overflowX: "auto" }}>
        <h2>Events</h2>
        <table>
          <thead><tr><th>Quelle</th><th>Typ</th><th>Status</th><th>Versuche</th><th>Lease</th><th>Fehler</th><th /></tr></thead>
          <tbody>
            {diagnostics.events.map((event) => (
              <tr key={`${event.source}:${event.event_id}`}>
                <td>{event.source}</td><td>{event.event_type}</td><td>{event.status}</td>
                <td>{event.attempts}</td><td>{formatTime(event.locked_at)}</td><td>{event.last_error ?? "—"}</td>
                <td>{event.status === "dead" && <button className="secondary" disabled={busy !== null} onClick={() => requeue(event)}>Sicher requeue</button>}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section className="card">
        <h2>Mappings</h2>
        {diagnostics.mappings.length === 0 ? <p>Keine Mappings vorhanden.</p> : diagnostics.mappings.map((mapping) => (
          <p key={mapping.entity_type}>{mapping.entity_type}: {mapping.count} · zuletzt {formatTime(mapping.last_updated_at)}</p>
        ))}
      </section>

      <section className="card" style={{ overflowX: "auto" }}>
        <h2>Administrative Aktionen</h2>
        <table><thead><tr><th>Zeit</th><th>Aktion</th><th>Ziel</th><th>Ergebnis</th></tr></thead><tbody>
          {diagnostics.audit.map((entry) => <tr key={entry.id}><td>{formatTime(entry.created_at)}</td><td>{entry.action}</td><td>{entry.target_type}:{entry.target_id}</td><td>{String(entry.details.outcome ?? "—")}</td></tr>)}
        </tbody></table>
      </section>
    </div>
  );
}

function QueueCard({ title, queue }: { title: string; queue: IntegrationQueueSummary }) {
  return <section className="card"><h2>{title}</h2><p>Offen: {queue.pending} · Lease: {queue.processing} · Zugestellt: {queue.delivered} · Tot: {queue.dead}</p><p>Ältestes offenes Event: {formatTime(queue.oldest_open_at)}</p><p>Letzter Erfolg: {formatTime(queue.last_success_at)}</p><p>Letzter Fehler: {queue.last_error ?? "—"}</p></section>;
}

function formatTime(value: string | null) {
  return value ? new Date(value).toLocaleString("de-DE") : "—";
}
