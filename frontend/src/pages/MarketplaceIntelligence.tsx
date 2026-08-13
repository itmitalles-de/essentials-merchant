import { useEffect, useMemo, useState } from "react";
import { api, downloadMarketplaceRawReport } from "../api";
import { useAuth } from "../contexts/AuthContext";
import type {
  AmazonConnectionSummary,
  AmazonReportRun,
  MarketplaceOverview,
  MarketplaceRunDetail,
} from "../types";

const salesReport = "GET_SALES_AND_TRAFFIC_REPORT";
const inventoryReport = "GET_FBA_INVENTORY_PLANNING_DATA";

const formatDate = (value: string | null) => (value ? new Intl.DateTimeFormat("de-DE", { dateStyle: "medium", timeStyle: "short" }).format(new Date(value)) : "–");

export function MarketplaceIntelligence() {
  const { role } = useAuth();
  const [overview, setOverview] = useState<MarketplaceOverview | null>(null);
  const [selected, setSelected] = useState<MarketplaceRunDetail | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const reload = async () => {
    try {
      setOverview(await api.get<MarketplaceOverview>("/marketplace"));
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Marketplace-Daten konnten nicht geladen werden.");
    }
  };
  useEffect(() => { void reload(); }, []);

  const connection = overview?.connections[0] ?? null;
  const marketplaceId = connection?.marketplace_ids[0] ?? "A1PA6795UKMFR9";
  const reports = useMemo(() => overview?.report_types ?? [], [overview]);

  const setupDemo = async () => {
    setLoading(true);
    setMessage(null);
    try {
      await api.post("/marketplace/demo");
      setMessage("Synthetische Demo-Verbindung eingerichtet. Sie enthält keine Amazon-Zugangsdaten.");
      await reload();
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Demo konnte nicht eingerichtet werden.");
    } finally {
      setLoading(false);
    }
  };

  const requestReport = async (reportType: string) => {
    if (!connection) return;
    setLoading(true);
    setMessage(null);
    try {
      const now = new Date();
      const start = new Date(now);
      start.setDate(start.getDate() - 7);
      const run = await api.post<AmazonReportRun>(`/marketplace/connections/${connection.id}/runs`, {
        marketplace_id: marketplaceId,
        report_type: reportType,
        data_start_time: start.toISOString(),
        data_end_time: now.toISOString(),
        report_options: {},
      });
      setMessage(`Abruf ${run.status === "polling" ? "bei Amazon angefordert" : "eingeplant"}.`);
      await reload();
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Report konnte nicht angefordert werden.");
    } finally {
      setLoading(false);
    }
  };

  const configureSchedule = async () => {
    if (!connection) return;
    setLoading(true);
    try {
      await api.put(`/marketplace/connections/${connection.id}/schedules`, {
        marketplace_id: marketplaceId,
        report_type: salesReport,
        report_options: {},
        interval_seconds: 86400,
        enabled: true,
      });
      setMessage("Täglicher Sales-&-Traffic-Abruf aktiviert.");
      await reload();
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Zeitplan konnte nicht gespeichert werden.");
    } finally {
      setLoading(false);
    }
  };

  const openRun = async (runId: string) => {
    setSelected(await api.get<MarketplaceRunDetail>(`/marketplace/runs/${runId}`));
  };

  const runTotalAnalysis = async () => {
    if (!connection) return;
    setLoading(true);
    try {
      const end = new Date();
      const start = new Date(end);
      start.setDate(start.getDate() - 30);
      await api.post(`/marketplace/connections/${connection.id}/analyses`, {
        marketplace_id: marketplaceId,
        report_type: salesReport,
        period_start: start.toISOString(),
        period_end: end.toISOString(),
      });
      setMessage("Gesamtanalyse wurde eingeplant.");
      await reload();
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Gesamtanalyse konnte nicht erstellt werden.");
    } finally {
      setLoading(false);
    }
  };

  const latestSuccessful = overview?.recent_runs.find((run) => run.status === "succeeded");
  return (
    <div style={{ display: "grid", gap: "1rem", maxWidth: 1200 }}>
      <section className="card">
        <h1>Marketplace Intelligence</h1>
        <p>Read-only Amazon-SP-API-Reports, nachvollziehbare Kennzahlen und Empfehlungen. KI-Ausgaben sind Hinweise, keine automatischen Änderungen an Amazon.</p>
        {message && <p style={{ color: "var(--fg-muted)" }}>{message}</p>}
        {!connection && role === "administrator" && (
          <button onClick={setupDemo} disabled={loading}>Synthetische Demo einrichten</button>
        )}
        {!connection && role !== "administrator" && <p>Für dieses optionale Modul ist noch keine berechtigte Verbindung aktiv.</p>}
        {connection && <ConnectionCard connection={connection} marketplaceId={marketplaceId} />}
      </section>

      {connection && (
        <section className="card">
          <h2>Abruf und Zeitplan</h2>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "0.75rem", alignItems: "center" }}>
            <button onClick={() => requestReport(salesReport)} disabled={loading}>Sales &amp; Traffic jetzt abrufen</button>
            <button className="secondary" onClick={() => requestReport(inventoryReport)} disabled={loading}>FBA Inventory Planning abrufen</button>
            {role === "administrator" && <button className="secondary" onClick={configureSchedule} disabled={loading}>Täglichen Sales-Zeitplan aktivieren</button>}
            <button className="secondary" onClick={runTotalAnalysis} disabled={loading}>30-Tage-Gesamtanalyse</button>
          </div>
          <p style={{ color: "var(--fg-muted)", marginBottom: 0 }}>
            Datenfrische: letzter erfolgreicher Report {formatDate(latestSuccessful?.completed_at ?? null)}.
            {overview?.schedules.some((schedule) => schedule.enabled) ? " Ein Zeitplan ist aktiv." : " Kein Zeitplan aktiv."}
          </p>
          <details style={{ marginTop: "0.75rem" }}>
            <summary>Report-Registry und Berechtigungen</summary>
            <ul>
              {reports.map((report) => <li key={report.report_type}><code>{report.report_type}</code> · {report.format} · Rollen: {report.required_roles.join(", ")} · {report.analysis_capable ? "analysierbar" : "nur Roharchiv"}</li>)}
            </ul>
          </details>
        </section>
      )}

      <section className="card">
        <h2>Reportverlauf</h2>
        <table>
          <thead><tr><th>Typ</th><th>Auslöser</th><th>Status</th><th>Zeitraum</th><th>Datenstand</th><th /></tr></thead>
          <tbody>
            {(overview?.recent_runs ?? []).map((run) => (
              <tr key={run.id}>
                <td><code>{run.report_type}</code></td><td>{run.trigger_source}</td><td><span className="badge">{run.status}</span></td>
                <td>{formatDate(run.data_start_time)} – {formatDate(run.data_end_time)}</td>
                <td>{formatDate(run.completed_at)}</td>
                <td><button className="secondary" onClick={() => openRun(run.id)}>Details</button></td>
              </tr>
            ))}
            {!overview?.recent_runs.length && <tr><td colSpan={6}>Noch keine Reportläufe.</td></tr>}
          </tbody>
        </table>
      </section>

      {selected && <RunDetail detail={selected} administrator={role === "administrator"} />}
      {(overview?.analyses ?? []).map((analysis) => <AnalysisCard key={analysis.id} result={analysis.result} title={`Analyse · ${formatDate(analysis.created_at)}`} />)}
    </div>
  );
}

function ConnectionCard({ connection, marketplaceId }: { connection: AmazonConnectionSummary; marketplaceId: string }) {
  return <div style={{ display: "grid", gap: "0.25rem" }}>
    <strong>Verbindung: {connection.mode === "fixture" ? "Synthetische Demo" : "Amazon SP-API"}</strong>
    <span>Seller: {connection.seller_id} · Region: {connection.region.toUpperCase()} · Marketplace: {marketplaceId}</span>
    <span>Rollen: {connection.granted_roles.join(", ")} · Zugangskonfiguration: {connection.credential_configured ? "hinterlegt" : "fehlt"}</span>
  </div>;
}

function RunDetail({ detail, administrator }: { detail: MarketplaceRunDetail; administrator: boolean }) {
  return <section className="card">
    <h2>Reportlauf · {detail.run.report_type}</h2>
    {detail.document && <p>Roharchiv: SHA-256 <code>{detail.document.sha256}</code> · Import: {detail.document.import_status} · Parser: {detail.document.parser_version ?? "–"}</p>}
    {detail.run.failure_message && <p style={{ color: "var(--danger)" }}>{detail.run.failure_message}</p>}
    {administrator && detail.document && <button className="secondary" onClick={() => void downloadMarketplaceRawReport(detail.run.id)}>Rohbericht herunterladen</button>}
    {detail.snapshot && <p>Snapshot: {detail.snapshot.granularity} · vergleichbar als <code>{detail.snapshot.comparability_key}</code></p>}
    <details><summary>Zustandsverlauf</summary><ul>{detail.events.map((event) => <li key={event.id}>{formatDate(event.created_at)} · <strong>{event.status}</strong> · {event.message}</li>)}</ul></details>
    {detail.metrics.length > 0 && <details><summary>Normalisierte Kennzahlen</summary><table><thead><tr><th>Kennzahl</th><th>Dimension</th><th>Wert</th></tr></thead><tbody>{detail.metrics.map((metric) => <tr key={metric.id}><td>{metric.metric_name}</td><td>{metric.dimension_type} {metric.dimension_key}</td><td>{metric.value_numeric} {metric.unit} {metric.currency_code}</td></tr>)}</tbody></table></details>}
    {detail.analyses.map((analysis) => <AnalysisCard key={analysis.id} title="Delta-Analyse" result={analysis.result} />)}
  </section>;
}

function AnalysisCard({ result, title }: { result: Record<string, unknown>; title: string }) {
  const options = Array.isArray(result.options) ? result.options as Array<Record<string, unknown>> : [];
  return <section className="card">
    <h2>{title}</h2>
    <p>{String(result.overall_trend ?? "Noch keine Trendbewertung.")}</p>
    {options.length > 0 && <><h3>Mögliche Handlungsoptionen</h3><ul>{options.map((option, index) => <li key={index}><strong>{String(option.action)}</strong> · Wirkung: {String(option.expected_effect)} · Aufwand: {String(option.effort)} · Unsicherheit: {String(option.uncertainty)}</li>)}</ul></>}
    {Array.isArray(result.missing_data) && <p style={{ color: "var(--fg-muted)" }}>Fehlende Daten: {(result.missing_data as unknown[]).join(" ")}</p>}
    <p style={{ color: "var(--warning)" }}>KI-Ausgaben und Analysen sind Empfehlungen. Merchant nimmt keine Preis-, Werbe-, Listing-, Bestands- oder Bestelländerungen vor.</p>
  </section>;
}
