import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import {
  api,
  downloadMarketplaceAnalysis,
  downloadMarketplaceRawReport,
} from "../api";
import { useAuth } from "../contexts/AuthContext";
import { usePilotStatus } from "../hooks/usePilotStatus";
import type {
  AmazonConnectionSummary,
  AmazonReportRun,
  MarketplaceImportPreview,
  MarketplaceImportResult,
  MarketplaceOverview,
  MarketplaceRunDetail,
  MarketplaceStrategyAction,
  MarketplaceStrategyFinding,
  MarketplaceStrategyHypothesis,
  MarketplaceStrategyView,
} from "../types";

const salesReport = "GET_SALES_AND_TRAFFIC_REPORT";
const terminalRunStatuses = ["succeeded", "archived", "cancelled", "fatal", "failed"];

interface ImportConfirmation {
  marketplaceId: string;
  currencyCode: string;
  periodStart: string;
  periodEnd: string;
  granularity: string;
  reportType: string;
}

const browserTimezone = () => {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
};

const formatDate = (value: string | null) => {
  if (!value) return "–";
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime())
    ? value
    : new Intl.DateTimeFormat("de-DE", {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(parsed);
};

const dateInputValue = (value: string) => value.slice(0, 10);

function importPath(
  endpoint: "/marketplace/imports/preview" | "/marketplace/imports",
  file: File,
  timezone: string,
  confirmation?: ImportConfirmation & { hash: string },
) {
  const parameters = new URLSearchParams({ filename: file.name, timezone });
  if (confirmation) {
    parameters.set("confirm_hash", confirmation.hash);
    parameters.set("confirm_marketplace_id", confirmation.marketplaceId);
    parameters.set("confirm_currency_code", confirmation.currencyCode);
    parameters.set("confirm_period_start", confirmation.periodStart);
    parameters.set("confirm_period_end", confirmation.periodEnd);
    parameters.set("confirm_granularity", confirmation.granularity);
    parameters.set("confirm_report_type", confirmation.reportType);
  }
  return `${endpoint}?${parameters.toString()}`;
}

export function MarketplaceIntelligence({ aiFirst = false }: { aiFirst?: boolean }) {
  const { role } = useAuth();
  const pilot = usePilotStatus();
  const [overview, setOverview] = useState<MarketplaceOverview | null>(null);
  const [selected, setSelected] = useState<MarketplaceRunDetail | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [importing, setImporting] = useState(false);
  const [file, setFile] = useState<File | null>(null);
  const [fileInputKey, setFileInputKey] = useState(0);
  const [timezone, setTimezone] = useState(browserTimezone);
  const [preview, setPreview] = useState<MarketplaceImportPreview | null>(null);
  const [confirmation, setConfirmation] = useState<ImportConfirmation | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [importResult, setImportResult] = useState<MarketplaceImportResult | null>(null);
  const [sessionImports, setSessionImports] = useState<MarketplaceImportResult[]>([]);

  const reload = async () => {
    try {
      setOverview(await api.get<MarketplaceOverview>("/marketplace"));
    } catch (error) {
      setMessage(
        error instanceof Error
          ? error.message
          : "Marketplace-Daten konnten nicht geladen werden.",
      );
    }
  };

  useEffect(() => {
    void reload();
  }, []);

  useEffect(() => {
    if (!selected || terminalRunStatuses.includes(selected.run.status)) return;
    const timer = window.setInterval(() => {
      api
        .get<MarketplaceRunDetail>(`/marketplace/runs/${selected.run.id}`)
        .then(setSelected)
        .catch(() => undefined);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [selected]);

  const connection = overview?.connections[0] ?? null;
  const marketplaceId = connection?.marketplace_ids[0] ?? null;
  const reports = useMemo(
    () => (overview?.report_types ?? []).filter((report) => report.report_type === salesReport),
    [overview],
  );
  const analyses = useMemo(
    () => [...(overview?.analyses ?? [])].sort((left, right) =>
      right.created_at.localeCompare(left.created_at)),
    [overview],
  );
  const immutableJsonMetadata = preview?.detected_format.toLowerCase().includes("json") ?? false;
  const latestSuccessful = overview?.recent_runs.find((run) => run.status === "succeeded");
  const realSpApiReady = Boolean(
    connection?.mode === "live" && connection.credential_configured && marketplaceId,
  );
  const syntheticSpApiTestReady = Boolean(connection?.mode === "fixture" && marketplaceId);

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

  const previewReport = async (event: FormEvent) => {
    event.preventDefault();
    if (!file) return;
    setPreviewing(true);
    setMessage(null);
    setImportResult(null);
    try {
      const result = await api.postRaw<MarketplaceImportPreview>(
        importPath("/marketplace/imports/preview", file, timezone),
        file,
      );
      setPreview(result);
      setConfirmation({
        marketplaceId: result.marketplace_id,
        currencyCode: result.currency_code,
        periodStart: dateInputValue(result.period_start),
        periodEnd: dateInputValue(result.period_end),
        granularity: result.granularity,
        reportType: result.report_type,
      });
      setConfirmed(false);
      setMessage("Vorschau erstellt. Die Rohdatei wurde noch nicht importiert.");
    } catch (error) {
      setPreview(null);
      setConfirmation(null);
      setMessage(error instanceof Error ? error.message : "Reportvorschau konnte nicht erstellt werden.");
    } finally {
      setPreviewing(false);
    }
  };

  const executeImport = async (event: FormEvent) => {
    event.preventDefault();
    if (!file || !preview || !confirmation || !confirmed) return;
    setImporting(true);
    setMessage(null);
    try {
      const result = await api.postRaw<MarketplaceImportResult>(
        importPath("/marketplace/imports", file, timezone, {
          ...confirmation,
          hash: preview.sha256,
        }),
        file,
      );
      setImportResult(result);
      setSessionImports((current) =>
        current.some((item) => item.run_id === result.run_id) ? current : [...current, result]);
      setMessage(
        result.outcome === "already_imported"
          ? "Dieser unveränderte Report war bereits importiert. Es wurden keine Daten dupliziert."
          : result.comparison_generated
            ? "Import abgeschlossen und kompatibler Periodenvergleich erzeugt."
            : "Import abgeschlossen. Für einen Vergleich fehlt noch ein kompatibler zweiter Zeitraum.",
      );
      await reload();
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Report konnte nicht importiert werden.");
    } finally {
      setImporting(false);
    }
  };

  const beginNextPeriod = () => {
    setFile(null);
    setPreview(null);
    setConfirmation(null);
    setConfirmed(false);
    setImportResult(null);
    setFileInputKey((value) => value + 1);
    setMessage("Nächsten kompatiblen Zeitraum auswählen.");
  };

  const requestReport = async () => {
    if (!connection || !marketplaceId) return;
    setLoading(true);
    setMessage(null);
    try {
      const now = new Date();
      const start = new Date(now);
      start.setUTCDate(start.getUTCDate() - 1);
      start.setUTCHours(0, 0, 0, 0);
      const end = new Date(start);
      end.setUTCHours(23, 59, 59, 0);
      const run = await api.post<AmazonReportRun>(
        `/marketplace/connections/${connection.id}/runs`,
        {
          marketplace_id: marketplaceId,
          report_type: salesReport,
          data_start_time: start.toISOString(),
          data_end_time: end.toISOString(),
          report_options: { dateGranularity: "DAY", asinGranularity: "CHILD" },
        },
      );
      setMessage(
        connection.mode === "fixture"
          ? "Synthetischer read-only Techniktest gestartet."
          : `Abruf ${run.status === "polling" ? "bei Amazon angefordert" : "eingeplant"}.`,
      );
      await openRun(run.id);
      await reload();
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Report konnte nicht angefordert werden.");
    } finally {
      setLoading(false);
    }
  };

  const openRun = async (runId: string) => {
    setSelected(await api.get<MarketplaceRunDetail>(`/marketplace/runs/${runId}`));
  };

  const updateConfirmation = (field: keyof ImportConfirmation, value: string) => {
    setConfirmation((current) => current ? { ...current, [field]: value } : current);
    setConfirmed(false);
  };

  const analysisSection = (
    <section aria-labelledby="analysis-heading">
      <div className="marketplace-section-heading">
        <div>
          <h2 id="analysis-heading">Analyse, Periodenvergleich und KI-Strategie</h2>
          <p className="marketplace-muted">
            Vergleiche entstehen nur aus kompatiblen importierten Zeiträumen. Die KI bewertet
            ausschließlich die daraus abgeleiteten Aggregatkennzahlen.
          </p>
        </div>
        <button type="button" className="secondary" onClick={() => void reload()}>
          Analysen aktualisieren
        </button>
      </div>
      {analyses.length === 0 && (
        <div className="card">
          <p>
            Noch keine Analyse vorhanden. Importiere unten einen Zeitraum; für belastbare Deltas
            anschließend einen kompatiblen zweiten Zeitraum.
          </p>
          <WeeklyStrategyPanel />
        </div>
      )}
      {analyses.map((analysis, index) => (
        <AnalysisCard
          key={analysis.id}
          id={analysis.id}
          result={analysis.result}
          title={`Periodenvergleich · ${formatDate(analysis.created_at)}`}
          showWeeklyStrategy={index === 0}
        />
      ))}
    </section>
  );

  return (
    <div className="marketplace-flow">
      <section className="card">
        <h1>{aiFirst ? "Amazon AI Marketing" : "Amazon Intelligence"}</h1>
        <p>
          Internes read-only Analysewerkzeug für offizielle Amazon-Reports. Uploads erzeugen
          nachvollziehbare Kennzahlen und Empfehlungen, aber keine Preis-, Ads-, Listing-,
          Bestands- oder Bestelländerung.
        </p>
        {aiFirst && (
          <div className="marketplace-callout strategy-intro">
            <strong>Interne Strategiehilfe für Mantle</strong>
            <p>
              Zuerst bleiben Fakten und regelbasierte Ableitungen sichtbar. Eine externe
              KI-Einschätzung wird nur nach deiner ausdrücklichen Hash-Bestätigung erzeugt.
            </p>
          </div>
        )}
        {message && <p className="marketplace-status" role="status">{message}</p>}
      </section>

      {aiFirst && analysisSection}

      <section className="card" aria-labelledby="manual-import-heading">
        <h2 id="manual-import-heading">Manueller Sales-&amp;-Traffic-Import</h2>
        <ol className="marketplace-steps" aria-label="Importablauf">
          <li>Datei hochladen</li>
          <li>Metadaten bestätigen</li>
          <li>Import ausführen</li>
          <li>Analyse vergleichen</li>
        </ol>

        <form onSubmit={previewReport} className="marketplace-form-grid">
          <label htmlFor="amazon-report-file">
            Offizieller Amazon-Report
            <input
              key={fileInputKey}
              id="amazon-report-file"
              type="file"
              required
              accept=".json,.csv,.tsv,application/json,text/csv,text/tab-separated-values"
              onChange={(event) => {
                setFile(event.target.files?.[0] ?? null);
                setPreview(null);
                setConfirmation(null);
                setConfirmed(false);
                setImportResult(null);
              }}
            />
          </label>
          <label htmlFor="amazon-report-timezone">
            Report-Zeitzone
            <input
              id="amazon-report-timezone"
              required
              value={timezone}
              disabled={Boolean(preview)}
              onChange={(event) => setTimezone(event.target.value)}
              placeholder="Europe/Berlin"
            />
          </label>
          <div className="marketplace-form-action">
            <button type="submit" disabled={!file || !timezone || previewing || importing}>
              {previewing ? "Report wird geprüft …" : "Importvorschau erstellen"}
            </button>
          </div>
        </form>
        <p className="marketplace-muted">
          Die Vorschau prüft Dateityp, Größe und Schema serverseitig. Rohdaten werden hier nicht
          angezeigt und nicht in Zusammenfassungsexporte übernommen.
        </p>

        {preview && confirmation && (
          <>
            <div className="marketplace-preview-header">
              <h3>Geprüfte Importvorschau</h3>
              <span className="badge">{preview.detected_format}</span>
            </div>
            <dl className="marketplace-meta-grid">
              <div><dt>SHA-256</dt><dd><code className="marketplace-hash">{preview.sha256}</code></dd></div>
              <div><dt>Reporttyp</dt><dd><code>{preview.report_type}</code></dd></div>
              <div><dt>Parser-Version</dt><dd>{preview.parser_version}</dd></div>
              <div><dt>Marketplace</dt><dd>{preview.marketplace_id || "zu bestätigen"}</dd></div>
              <div><dt>Zeitraum</dt><dd>{preview.period_start || "–"} – {preview.period_end || "–"}</dd></div>
              <div><dt>Granularität</dt><dd>{preview.granularity || "–"}</dd></div>
              <div><dt>Währung</dt><dd>{preview.currency_code || "–"}</dd></div>
              <div><dt>Zeitzone</dt><dd>{preview.timezone}</dd></div>
              <div><dt>Datenfrische</dt><dd>{preview.data_freshness || "nicht bestimmbar"}</dd></div>
            </dl>

            {preview.warnings.length > 0 && (
              <div className="marketplace-callout warning">
                <strong>Parserwarnungen</strong>
                <ul>{preview.warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul>
              </div>
            )}
            {preview.missing_fields.length > 0 && (
              <div className="marketplace-callout">
                <strong>Fehlende Felder</strong>
                <ul>{preview.missing_fields.map((field) => <li key={field}>{field}</li>)}</ul>
              </div>
            )}

            <form onSubmit={executeImport} className="marketplace-confirmation">
              <fieldset disabled={importing || Boolean(importResult)}>
                <legend>Zeitraum und Dimensionen bestätigen</legend>
                <div className="marketplace-form-grid">
                  <label htmlFor="confirm-marketplace">
                    Marketplace
                    <input
                      id="confirm-marketplace"
                      required
                      readOnly={immutableJsonMetadata && Boolean(preview.marketplace_id)}
                      value={confirmation.marketplaceId}
                      onChange={(event) => updateConfirmation("marketplaceId", event.target.value)}
                    />
                  </label>
                  <label htmlFor="confirm-currency">
                    Währung
                    <input
                      id="confirm-currency"
                      required
                      readOnly={immutableJsonMetadata && Boolean(preview.currency_code)}
                      value={confirmation.currencyCode}
                      onChange={(event) => updateConfirmation("currencyCode", event.target.value.toUpperCase())}
                      autoComplete="off"
                    />
                  </label>
                  <label htmlFor="confirm-granularity">
                    Granularität
                    <input
                      id="confirm-granularity"
                      required
                      readOnly
                      value={confirmation.granularity}
                      onChange={(event) => updateConfirmation("granularity", event.target.value)}
                    />
                  </label>
                  <label htmlFor="confirm-period-start">
                    Zeitraum von
                    <input
                      id="confirm-period-start"
                      type="date"
                      required
                      readOnly={immutableJsonMetadata}
                      value={confirmation.periodStart}
                      onChange={(event) => updateConfirmation("periodStart", event.target.value)}
                    />
                  </label>
                  <label htmlFor="confirm-period-end">
                    Zeitraum bis
                    <input
                      id="confirm-period-end"
                      type="date"
                      required
                      readOnly={immutableJsonMetadata}
                      min={confirmation.periodStart}
                      value={confirmation.periodEnd}
                      onChange={(event) => updateConfirmation("periodEnd", event.target.value)}
                    />
                  </label>
                  <label htmlFor="confirm-report-type">
                    Bestätigter Reporttyp
                    <input id="confirm-report-type" readOnly value={confirmation.reportType} />
                  </label>
                </div>
                {immutableJsonMetadata && (
                  <p className="marketplace-muted">
                    Im JSON enthaltene Marketplace- und Währungsmetadaten werden unverändert
                    bestätigt. Fehlende CSV-/TSV-Metadaten können hier ergänzt werden.
                  </p>
                )}
                <label className="marketplace-checkbox" htmlFor="confirm-import">
                  <input
                    id="confirm-import"
                    type="checkbox"
                    checked={confirmed}
                    onChange={(event) => setConfirmed(event.target.checked)}
                  />
                  Hash, Reporttyp, Marketplace, Währung, Zeitraum und Granularität sind geprüft.
                </label>
                <div className="marketplace-actions">
                  <button type="submit" disabled={!confirmed || importing}>
                    {importing ? "Import läuft …" : "Bestätigten Import ausführen"}
                  </button>
                </div>
              </fieldset>
            </form>

            <div className="table-scroll">
              <table>
                <caption>Normalisierte Kennzahlen der Vorschau</caption>
                <thead>
                  <tr><th>Kennzahl</th><th>Dimension</th><th>Wert</th></tr>
                </thead>
                <tbody>
                  {preview.metrics.map((metric, index) => (
                    <tr key={`${metric.metric_name}-${metric.dimension_type}-${metric.dimension_key}-${index}`}>
                      <td>{metric.metric_name}</td>
                      <td>{metric.dimension_type} {metric.dimension_key}</td>
                      <td>{metric.value_numeric} {metric.unit} {metric.currency_code ?? ""}</td>
                    </tr>
                  ))}
                  {preview.metrics.length === 0 && (
                    <tr><td colSpan={3}>Keine normalisierten Kennzahlen in der Vorschau.</td></tr>
                  )}
                </tbody>
              </table>
            </div>

            {importResult && (
              <div className="marketplace-callout success" role="status">
                <strong>
                  {importResult.outcome === "imported" ? "Import abgeschlossen" : "Bereits importiert"}
                </strong>
                <p>
                  Lauf <code>{importResult.run_id}</code>. {importResult.comparison_generated && importResult.analysis_id
                    ? <>Vergleichsanalyse <code>{importResult.analysis_id}</code> wurde erzeugt.</>
                    : "Dieser Zeitraum bildet die Vergleichsbasis."}
                </p>
                <button type="button" className="secondary" onClick={beginNextPeriod}>
                  Zweiten Zeitraum hinzufügen
                </button>
              </div>
            )}
          </>
        )}

        {sessionImports.length > 0 && (
          <details className="marketplace-session-imports">
            <summary>Importe in dieser Sitzung ({sessionImports.length})</summary>
            <ul>
              {sessionImports.map((item) => (
                <li key={item.run_id}>
                  {item.preview.period_start} – {item.preview.period_end} · {item.preview.marketplace_id}
                  {" · "}{item.outcome === "imported" ? "importiert" : "bereits vorhanden"}
                </li>
              ))}
            </ul>
          </details>
        )}
      </section>

      {!aiFirst && analysisSection}

      <section className="card" aria-labelledby="sp-api-heading">
        <h2 id="sp-api-heading">Optionaler SP-API-Abruf</h2>
        {!realSpApiReady && (
          <div className="marketplace-callout warning">
            <strong>Externes Amazon-Gate</strong>
            <p>
              Keine ausdrücklich freigegebenen Live-Credentials sind verifiziert. Der manuelle
              Upload oben bleibt vollständig nutzbar; es werden keine Ersatz-Credentials erzeugt.
            </p>
          </div>
        )}
        {!connection && role === "administrator" && (
          <button type="button" className="secondary" onClick={setupDemo} disabled={loading}>
            Synthetische Demo einrichten
          </button>
        )}
        {!connection && role !== "administrator" && (
          <p>Für dieses optionale Modul ist noch keine berechtigte Verbindung aktiv.</p>
        )}
        {connection && <ConnectionCard connection={connection} marketplaceId={marketplaceId} />}
        {(realSpApiReady || syntheticSpApiTestReady) && (
          <div className="marketplace-actions">
            <button type="button" onClick={requestReport} disabled={loading}>
              Sales &amp; Traffic jetzt abrufen
            </button>
          </div>
        )}
        <p className="marketplace-muted">
          Ausschließlich ein einmaliger Sales-&amp;-Traffic-Abruf. Kein Scheduler, keine Buyer-/Order-PII
          und keine Amazon-Mutation. Letzter erfolgreicher Lauf: {formatDate(latestSuccessful?.completed_at ?? null)}.
        </p>
        {reports.length > 0 && (
          <details>
            <summary>Report-Registry und Berechtigungen</summary>
            <ul>
              {reports.map((report) => (
                <li key={report.report_type}>
                  <code>{report.report_type}</code> · {report.format} · Rollen: {report.required_roles.join(", ")}
                </li>
              ))}
            </ul>
          </details>
        )}
      </section>

      <section className="card">
        <h2>Reportverlauf</h2>
        <div className="table-scroll">
          <table>
            <thead>
              <tr><th>Typ</th><th>Auslöser</th><th>Status</th><th>Zeitraum</th><th>Datenstand</th><th /></tr>
            </thead>
            <tbody>
              {(overview?.recent_runs ?? []).map((run) => (
                <tr key={run.id}>
                  <td><code>{run.report_type}</code></td>
                  <td>{run.trigger_source}</td>
                  <td><span className="badge">{run.status}</span></td>
                  <td>{formatDate(run.data_start_time)} – {formatDate(run.data_end_time)}</td>
                  <td>{formatDate(run.completed_at)}</td>
                  <td>
                    <button type="button" className="secondary" onClick={() => void openRun(run.id)}>
                      Details
                    </button>
                  </td>
                </tr>
              ))}
              {!overview?.recent_runs.length && (
                <tr><td colSpan={6}>Noch keine Reportläufe.</td></tr>
              )}
            </tbody>
          </table>
        </div>
      </section>

      {selected && (
        <RunDetail
          detail={selected}
          allowRawDownload={!pilot?.enabled && role === "administrator"}
        />
      )}
    </div>
  );
}

function ConnectionCard({
  connection,
  marketplaceId,
}: {
  connection: AmazonConnectionSummary;
  marketplaceId: string | null;
}) {
  const connectionStatus = connection.mode === "fixture"
    ? "synthetisch – kein Amazonzugang"
    : connection.credential_configured
      ? "formal konfiguriert – nicht live verifiziert"
      : "extern blockiert";
  return (
    <div className="marketplace-connection">
      <strong>
        Verbindung: {connection.mode === "fixture" ? "Synthetische Demo" : "Amazon SP-API"}
      </strong>
      <span>
        Seller: {connection.seller_id_redacted} · Region: {connection.region.toUpperCase()} ·
        Marketplace: {marketplaceId ?? "nicht bestätigt"}
      </span>
      <span>Verbindungsstatus: {connectionStatus}</span>
      <span>
        Rollenstatus (deklariert): {connection.granted_roles.join(", ")} · Secret-Shape:{" "}
        {connection.mode === "fixture"
          ? "nicht erforderlich (Fixture)"
          : connection.credential_configured
            ? "gültig"
            : "fehlt"}
      </span>
    </div>
  );
}

function RunDetail({
  detail,
  allowRawDownload,
}: {
  detail: MarketplaceRunDetail;
  allowRawDownload: boolean;
}) {
  return (
    <section className="card">
      <h2>Reportlauf · {detail.run.report_type}</h2>
      <p>
        Job: {detail.run.status} · Pollingversuche: {detail.run.poll_attempts} · letzter Abruf:{" "}
        {formatDate(detail.run.completed_at ?? detail.run.updated_at)}
      </p>
      {detail.document && (
        <p>
          Roharchiv: unveränderlich · Transport {detail.document.transport_bytes} Bytes · Decoded{" "}
          {detail.document.decoded_bytes} Bytes · SHA-256 <code className="marketplace-hash">{detail.document.sha256}</code>{" "}
          · Decoded-Hash <code className="marketplace-hash">{detail.document.decoded_sha256}</code>{" "}
          · Import: {detail.document.import_status} · Parser: {detail.document.parser_version ?? "–"}
        </p>
      )}
      {detail.run.failure_message && <p style={{ color: "var(--danger)" }}>{detail.run.failure_message}</p>}
      {allowRawDownload && detail.document && (
        <button
          type="button"
          className="secondary"
          onClick={() => void downloadMarketplaceRawReport(detail.run.id)}
        >
          Rohbericht herunterladen
        </button>
      )}
      {detail.snapshot && (
        <p>
          Snapshot: {detail.snapshot.granularity} · vergleichbar als{" "}
          <code>{detail.snapshot.comparability_key}</code>
        </p>
      )}
      {!detail.snapshot && (
        <p className="marketplace-muted">Snapshot-Kompatibilität: noch keine normalisierten Daten.</p>
      )}
      {detail.transport.length > 0 && (
        <details>
          <summary>Redigierte Transportdiagnose</summary>
          <ul>
            {detail.transport.map((entry) => (
              <li key={entry.id}>
                {entry.operation} · Request-ID {entry.request_id_redacted ?? "fehlt"} · Rate-Limit{" "}
                {entry.rate_limit_limit ?? "nicht gemeldet"} · Retry {entry.retry_after_seconds ?? 0}s
              </li>
            ))}
          </ul>
        </details>
      )}
      <details>
        <summary>Zustandsverlauf</summary>
        <ul>
          {detail.events.map((event) => (
            <li key={event.id}>
              {formatDate(event.created_at)} · <strong>{event.status}</strong> · {event.message}
            </li>
          ))}
        </ul>
      </details>
      {detail.metrics.length > 0 && (
        <details>
          <summary>Normalisierte Kennzahlen</summary>
          <div className="table-scroll">
            <table>
              <thead><tr><th>Kennzahl</th><th>Dimension</th><th>Wert</th></tr></thead>
              <tbody>
                {detail.metrics.map((metric) => (
                  <tr key={metric.id}>
                    <td>{metric.metric_name}</td>
                    <td>{metric.dimension_type} {metric.dimension_key}</td>
                    <td>{metric.value_numeric} {metric.unit} {metric.currency_code}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </details>
      )}
      {detail.analyses.map((analysis) => (
        <AnalysisCard key={analysis.id} id={analysis.id} title="Delta-Analyse" result={analysis.result} />
      ))}
    </section>
  );
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function describeAnalysisItem(value: unknown): string {
  if (value === null || value === undefined) return "–";
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  if (Array.isArray(value)) return value.map(describeAnalysisItem).join(" · ");

  const item = value as Record<string, unknown>;
  if (item.label !== undefined && item.value !== undefined) {
    return `${describeAnalysisItem(item.label)}: ${describeAnalysisItem(item.value)}`;
  }
  if (item.hypothesis !== undefined) return describeAnalysisItem(item.hypothesis);
  if (item.metric !== undefined && item.current !== undefined) {
    const percent = item.percent_change === null || item.percent_change === undefined
      ? "Prozentänderung nicht bestimmbar"
      : `${describeAnalysisItem(item.percent_change)} %`;
    return `${describeAnalysisItem(item.metric)}: ${describeAnalysisItem(item.previous)} → ${describeAnalysisItem(item.current)}; Delta ${describeAnalysisItem(item.difference)} (${percent})`;
  }
  if (item.metric !== undefined && item.value !== undefined) {
    return `${describeAnalysisItem(item.metric)}: ${describeAnalysisItem(item.value)} ${describeAnalysisItem(item.unit)} ${describeAnalysisItem(item.currency)}`.trim();
  }
  if (item.snapshot_id !== undefined && Array.isArray(item.catalog_metrics)) {
    const period = [item.period_start, item.period_end].filter(Boolean).join(" – ");
    return `${period || "Snapshot"}: ${describeAnalysisItem(item.catalog_metrics)}`;
  }
  if (item.kind !== undefined && item.detail !== undefined) {
    return `${describeAnalysisItem(item.kind)}: ${describeAnalysisItem(item.detail)}`;
  }
  return Object.entries(item)
    .filter(([key]) => !["evidence_ref", "evidence_refs", "uncertainty"].includes(key))
    .map(([key, entry]) => `${key}: ${describeAnalysisItem(entry)}`)
    .join(" · ");
}

function AnalysisItems({ items, emptyText }: { items: unknown[]; emptyText: string }) {
  if (items.length === 0) return <p className="marketplace-muted">{emptyText}</p>;
  return (
    <ul>
      {items.map((item, index) => {
        const record = typeof item === "object" && item !== null
          ? item as Record<string, unknown>
          : null;
        const evidence = record
          ? [record.evidence_ref, ...asArray(record.evidence_refs)].filter(Boolean)
          : [];
        return (
          <li key={`${describeAnalysisItem(item)}-${index}`}>
            {describeAnalysisItem(item)}
            {record?.uncertainty !== undefined && (
              <div className="marketplace-muted">Unsicherheit: {describeAnalysisItem(record.uncertainty)}</div>
            )}
            {evidence.length > 0 && (
              <div className="marketplace-muted">Evidenz: {evidence.map(String).join(", ")}</div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

const kpiDefinitions = [
  { keys: ["ordered_product_sales"], label: "Umsatz" },
  { keys: ["units_ordered"], label: "Bestellte Einheiten" },
  { keys: ["sessions"], label: "Sessions" },
  { keys: ["page_views"], label: "Page Views" },
  { keys: ["unit_session_percentage", "conversion_rate"], label: "Conversion" },
  { keys: ["buy_box_percentage"], label: "Buy Box" },
  { keys: ["b2b_revenue_share", "b2b_share", "b2b_units_share"], label: "B2B-Anteil" },
] as const;

const numericValue = (value: unknown): number | null => {
  const parsed = typeof value === "number" ? value : Number.parseFloat(String(value ?? ""));
  return Number.isFinite(parsed) ? parsed : null;
};

const metricRecord = (items: unknown[], keys: readonly string[]) => items
  .map((item) => typeof item === "object" && item !== null ? item as Record<string, unknown> : null)
  .find((item) => item && keys.includes(String(item.metric))) ?? null;

const metricValueLabel = (value: number | null, item: Record<string, unknown> | null) => {
  if (value === null) return "–";
  const formatted = new Intl.NumberFormat("de-DE", { maximumFractionDigits: 2 }).format(value);
  const currency = typeof item?.currency === "string" ? item.currency : null;
  const unit = typeof item?.unit === "string" ? item.unit : null;
  if (currency) return `${formatted} ${currency}`;
  if (unit === "percent" || unit === "%") return `${formatted} %`;
  return formatted;
};

const barWidth = (value: number | null, maximum: number) => {
  if (value === null || maximum <= 0) return 0;
  return Math.max(4, Math.min(100, Math.abs(value) / maximum * 100));
};

function KpiComparisonChart({ result }: { result: Record<string, unknown> }) {
  const changes = asArray(result.changes_since_last_run);
  const facts = asArray(result.facts);
  return (
    <figure className="kpi-chart">
      <figcaption>
        <strong>KPI-Überblick</strong>
        <span>Vorperiode und aktueller Zeitraum · feste Darstellung aus Serverfakten</span>
      </figcaption>
      <div className="kpi-chart-grid">
        {kpiDefinitions.map((definition) => {
          const change = metricRecord(changes, definition.keys);
          const fact = metricRecord(facts, definition.keys);
          const current = numericValue(change?.current ?? fact?.value);
          const previous = numericValue(change?.previous);
          const maximum = Math.max(Math.abs(current ?? 0), Math.abs(previous ?? 0));
          const percent = numericValue(change?.percent_change);
          const trend = String(change?.trend ?? (current === null ? "missing" : "current"));
          const trendLabel = ({
            up: "↑ gestiegen",
            up_from_zero: "↑ neu",
            down: "↓ gesunken",
            down_to_zero: "↓ auf null",
            stable: "→ stabil",
            current: "nur aktuell",
            missing: "nicht vorhanden",
          } as Record<string, string>)[trend] ?? trend;
          const valueSource = change ?? fact;
          return (
            <section className="kpi-tile" key={definition.label}>
              <div className="kpi-tile-heading">
                <h3>{definition.label}</h3>
                <span className={`kpi-trend ${trend}`}>{trendLabel}</span>
              </div>
              <div
                className="kpi-bars"
                role="img"
                aria-label={`${definition.label}: Vorperiode ${metricValueLabel(previous, valueSource)}, aktuell ${metricValueLabel(current, valueSource)}`}
              >
                <div><span>Vorher</span><i><b style={{ width: `${barWidth(previous, maximum)}%` }} /></i></div>
                <div><span>Aktuell</span><i><b style={{ width: `${barWidth(current, maximum)}%` }} /></i></div>
              </div>
              <div className="kpi-values">
                <span>{metricValueLabel(previous, valueSource)}</span>
                <strong>{metricValueLabel(current, valueSource)}</strong>
              </div>
              <p>
                {percent === null
                  ? "Delta nicht bestimmbar"
                  : `${percent >= 0 ? "+" : ""}${new Intl.NumberFormat("de-DE", { maximumFractionDigits: 2 }).format(percent)} %`}
              </p>
            </section>
          );
        })}
      </div>
    </figure>
  );
}

const confidenceLabel = (value: MarketplaceStrategyFinding["confidence"]) => ({
  low: "niedrig",
  medium: "mittel",
  high: "hoch",
})[value];

const priorityLabel = (value: MarketplaceStrategyAction["priority"]) => ({
  now: "jetzt",
  next: "als Nächstes",
  later: "später",
})[value];

function StrategyFindings({
  title,
  items,
}: {
  title: string;
  items: MarketplaceStrategyFinding[];
}) {
  return (
    <section className="strategy-section">
      <h4>{title}</h4>
      {items.length === 0 ? (
        <p className="marketplace-muted">Keine ausgewiesen.</p>
      ) : (
        <ul>
          {items.map((item, index) => (
            <li key={`${item.title}-${index}`}>
              <strong>{item.title}</strong> · Konfidenz: {confidenceLabel(item.confidence)}
              <p>{item.rationale}</p>
              <p className="marketplace-muted">
                Evidenz: {item.evidence_refs.join(", ") || "keine direkte Referenz"}
              </p>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function StrategyResult({ view }: { view: MarketplaceStrategyView }) {
  const assessment = view.assessment;
  if (!assessment) return null;
  return (
    <div className="strategy-result">
      <div className="marketplace-preview-header">
        <h3>KI-Strategieeinschätzung</h3>
        <span className="badge strategy-badge">KI-generiert – keine Faktenquelle</span>
      </div>
      <p className="strategy-summary">{assessment.executive_summary}</p>
      <section className="strategy-section">
        <h4>Bewertung</h4>
        <p>{assessment.assessment}</p>
      </section>
      <div className="strategy-grid">
        <StrategyFindings title="Chancen" items={assessment.opportunities} />
        <StrategyFindings title="Risiken" items={assessment.risks} />
      </div>
      <section className="strategy-section">
        <h4>Hypothesen – nicht als Fakten behandeln</h4>
        {assessment.hypotheses.length === 0 ? (
          <p className="marketplace-muted">Keine zusätzliche Hypothese.</p>
        ) : (
          <ul>
            {assessment.hypotheses.map((item: MarketplaceStrategyHypothesis, index) => (
              <li key={`${item.statement}-${index}`}>
                <strong>{item.statement}</strong> · Konfidenz: {confidenceLabel(item.confidence)}
                <p>{item.rationale}</p>
                <p>Benötigte Evidenz: {item.evidence_needed.join(" · ") || "nicht benannt"}</p>
                <p className="marketplace-muted">
                  Vorhandene Evidenz: {item.evidence_refs.join(", ") || "keine direkte Referenz"}
                </p>
              </li>
            ))}
          </ul>
        )}
      </section>
      <section className="strategy-section">
        <h4>Mögliche Maßnahmen – nur nach menschlicher Entscheidung</h4>
        {assessment.recommended_actions.length === 0 ? (
          <p className="marketplace-muted">Keine zusätzliche Maßnahme.</p>
        ) : (
          <ol>
            {assessment.recommended_actions.map((item, index) => (
              <li key={`${item.title}-${index}`}>
                <strong>{item.title}</strong> · Priorität: {priorityLabel(item.priority)}
                <p>{item.rationale}</p>
                <p>Erwartetes Prüfsignal: {item.expected_signal}</p>
                {item.risks.length > 0 && <p>Risiken: {item.risks.join(" · ")}</p>}
                <p className="marketplace-muted">
                  Evidenz: {item.evidence_refs.join(", ") || "keine direkte Referenz"}
                </p>
              </li>
            ))}
          </ol>
        )}
      </section>
      <div className="strategy-grid">
        <section className="strategy-section">
          <h4>Offene Fragen</h4>
          <AnalysisItems items={assessment.open_questions} emptyText="Keine weitere offene Frage." />
        </section>
        <section className="strategy-section">
          <h4>Grenzen und Unsicherheit</h4>
          <AnalysisItems items={assessment.limitations} emptyText="Keine zusätzliche Grenze benannt." />
        </section>
      </div>
      <section className="strategy-section strategy-handover">
        <h4>Handover bis zum nächsten Wochenlauf</h4>
        {assessment.handover ? (
          <>
            <p>{assessment.handover.continuity_summary}</p>
            <div className="strategy-grid">
              <div>
                <h5>Prioritäten</h5>
                <AnalysisItems
                  items={assessment.handover.priorities_until_next_run}
                  emptyText="Keine Priorität übertragen."
                />
              </div>
              <div>
                <h5>Evidenz sammeln</h5>
                <AnalysisItems
                  items={assessment.handover.evidence_for_next_run}
                  emptyText="Keine zusätzliche Evidenz angefordert."
                />
              </div>
              <div>
                <h5>Im nächsten Lauf prüfen</h5>
                <AnalysisItems
                  items={assessment.handover.next_run_checks}
                  emptyText="Keine Folgeprüfung benannt."
                />
              </div>
            </div>
          </>
        ) : (
          <p className="marketplace-muted">Historischer Lauf ohne strukturiertes Handover.</p>
        )}
      </section>
      <p className="marketplace-muted strategy-metadata">
        Modell {view.status.model} · Prompt {view.status.prompt_version} · Wochenlauf {view.assessment_week_start ?? "historisch"}
        {" · "}erzeugt {formatDate(view.created_at)}
        {view.input_tokens !== null && ` · Input ${view.input_tokens} Tokens`}
        {view.output_tokens !== null && ` · Output ${view.output_tokens} Tokens`}
        {view.cached && " · unverändert wiederverwendet"}
      </p>
    </div>
  );
}

function strategyErrorMessage(error: unknown): string {
  const code = error instanceof Error ? error.message : "strategy_request_failed";
  const messages: Record<string, string> = {
    openai_not_configured: "OpenAI ist auf diesem Server noch nicht freigegeben oder der API-Key fehlt.",
    openai_authentication_failed: "Der konfigurierte OpenAI-Zugang wurde abgelehnt.",
    openai_rate_limited: "Das OpenAI-Limit ist erreicht. Bitte später erneut versuchen.",
    openai_refused: "Das Modell hat diese Einschätzung abgelehnt.",
    openai_invalid_response: "OpenAI lieferte keine gültige strukturierte Einschätzung.",
    openai_unavailable: "OpenAI ist vorübergehend nicht erreichbar.",
    strategy_assessment_busy: "Eine Strategieeinschätzung läuft bereits.",
    weekly_limit_reached: "Der Wochenlauf wurde bereits erstellt.",
    no_analysis_data: "Es sind noch keine freigegebenen Amazon-Aggregatdaten vorhanden.",
    aggregate_confirmation_mismatch: "Die Aggregatdaten haben sich geändert. Bitte den neuen Hash prüfen.",
    aggregate_payload_invalid: "Diese Analyse enthält keine freigegebenen Aggregatdaten für die KI-Strategie.",
    aggregate_payload_too_large: "Die freigegebene Aggregatzusammenfassung ist zu groß.",
  };
  return messages[code] ?? `KI-Strategie konnte nicht geladen werden (${code}).`;
}

function weeklyBlockMessage(view: MarketplaceStrategyView): string | null {
  if (view.block_reason === "weekly_limit_reached") {
    return `Diese Kalenderwoche ist abgeschlossen. Die nächste Analyse ist ab ${formatDate(view.next_available_at)} möglich.`;
  }
  if (view.block_reason === "no_analysis_data") {
    return "Importiere zuerst mindestens einen offiziellen Amazon-Report. Der Button verwendet danach alle freigegebenen Aggregatanalysen.";
  }
  if (view.block_reason === "api_key_missing") {
    return "Der serverseitige Pay-per-use-API-Key fehlt. Import, Kennzahlen und Diagramme bleiben nutzbar.";
  }
  if (view.block_reason === "feature_disabled") {
    return "Die externe KI-Strategie ist serverseitig noch nicht freigegeben.";
  }
  return null;
}

function WeeklyStrategyPanel() {
  const { role } = useAuth();
  const [view, setView] = useState<MarketplaceStrategyView | null>(null);
  const [loading, setLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (role !== "administrator") return;
    let active = true;
    setLoading(true);
    setError(null);
    api.get<MarketplaceStrategyView>("/marketplace/strategy/weekly")
      .then((result) => {
        if (active) setView(result);
      })
      .catch((reason) => {
        if (active) setError(strategyErrorMessage(reason));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [role]);

  if (role !== "administrator") return null;

  const createAssessment = async () => {
    if (!view?.can_run || !view.current_payload_sha256) return;
    setSubmitting(true);
    setError(null);
    try {
      const result = await api.post<MarketplaceStrategyView>(
        "/marketplace/strategy/weekly",
        {
          confirmed_payload_sha256: view.current_payload_sha256,
          confirmed_aggregate_only: true,
        },
      );
      setView(result);
    } catch (reason) {
      setError(strategyErrorMessage(reason));
      try {
        const refreshed = await api.get<MarketplaceStrategyView>("/marketplace/strategy/weekly");
        setView(refreshed);
      } catch {
        // Keep the original actionable error and the last verified hash visible.
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <section className="strategy-panel" aria-labelledby="weekly-strategy" aria-busy={loading || submitting}>
      <div className="marketplace-preview-header">
        <h3 id="weekly-strategy">Wöchentliche KI-Marketinganalyse</h3>
        <span className="badge">maximal 1× pro Kalenderwoche</span>
      </div>
      <p>
        Ein Klick verarbeitet alle aktuell freigegebenen Aggregatanalysen und nimmt das validierte
        Handover des letzten Wochenlaufs als Kontext. OpenAI erhält keine Rohdatei, Reportzeile,
        ASIN/SKU, Buyer-/Order-PII oder Secrets.
      </p>
      {loading && <p role="status">Aggregatgrenze wird geprüft …</p>}
      {error && <p className="marketplace-callout warning" role="alert">{error}</p>}
      {view && (
        <>
          <dl className="strategy-contract">
            <div>
              <dt>Aktueller Aggregat-Hash</dt>
              <dd><code className="marketplace-hash">{view.current_payload_sha256 ?? "noch keine Daten"}</code></dd>
            </div>
            {view.assessment_payload_sha256 && (
              <div>
                <dt>Vom angezeigten KI-Lauf bewertet</dt>
                <dd><code className="marketplace-hash">{view.assessment_payload_sha256}</code></dd>
              </div>
            )}
            <div><dt>Eingelesene Analysen</dt><dd>{view.source_analysis_count}</dd></div>
            <div><dt>Letzter Lauf als Kontext</dt><dd>{view.previous_run_context ? "ja" : "noch nicht vorhanden"}</dd></div>
            <div><dt>Wochenfenster</dt><dd>ab {view.week_start} · Europe/Berlin</dd></div>
            <div><dt>Modell</dt><dd>{view.status.model}</dd></div>
            <div><dt>Speicherung bei Anfrage</dt><dd><code>store: false</code></dd></div>
            <div><dt>Amazon-Mutation</dt><dd>nicht vorhanden</dd></div>
          </dl>
          {weeklyBlockMessage(view) && (
            <div
              className={`marketplace-callout ${view.block_reason === "weekly_limit_reached" ? "success" : "warning"}`}
              role="status"
            >
              <strong>
                {view.block_reason === "weekly_limit_reached" ? "Wochenlimit aktiv" : "Analyse noch nicht ausführbar"}
              </strong>
              <p>{weeklyBlockMessage(view)}</p>
            </div>
          )}
          {view.assessment && view.current_payload_sha256 !== view.assessment_payload_sha256 && (
            <div className="marketplace-callout warning" role="status">
              <strong>Neuere Importdaten vorhanden</strong>
              <p>
                Die unten angezeigte KI-Antwort gehört zum bewerteten Hash. Neu importierte Daten
                stehen im festen KPI-Bereich oben und fließen erst in den nächsten Wochenlauf ein.
              </p>
            </div>
          )}
          <button
            type="button"
            className="weekly-analysis-button"
            disabled={!view.can_run || submitting}
            onClick={() => void createAssessment()}
          >
            {submitting ? "Analyse läuft …" : "Analyse"}
          </button>
          <p className="marketplace-muted">
            Der Klick bestätigt die einmalige Übermittlung des angezeigten Aggregat-Hashes. Ein
            fehlgeschlagener Provideraufruf verbraucht das Wochenfenster nicht; ein erfolgreich
            gespeicherter Lauf sperrt es serverseitig bis zum nächsten Montag.
          </p>
          <StrategyResult view={view} />
        </>
      )}
      <p className="marketplace-muted">
        Die Ausgabe ist eine Entscheidungshilfe. Es wird keine Preis-, Ads-, Listing-, Bestands-
        oder sonstige Amazon-Änderung ausgeführt.
      </p>
    </section>
  );
}

function AnalysisCard({
  id,
  result,
  title,
  showWeeklyStrategy = false,
}: {
  id: string;
  result: Record<string, unknown>;
  title: string;
  showWeeklyStrategy?: boolean;
}) {
  const context = typeof result.context === "object" && result.context !== null
    ? result.context as Record<string, unknown>
    : {};
  const facts = asArray(result.facts);
  const derivations = [
    result.overall_trend === undefined ? null : { label: "Trend", value: result.overall_trend },
    result.seasonality === undefined ? null : { label: "Saisonalität", value: result.seasonality },
    ...asArray(result.derived_observations),
    ...asArray(result.changes_since_last_run),
    ...asArray(result.anomalies),
  ].filter((item) => item !== null);
  const hypotheses = asArray(result.hypotheses);
  const openQuestions = [
    ...asArray(result.open_questions),
    ...asArray(result.missing_evidence).map((value) => ({
      label: "Fehlende Evidenz",
      value,
    })),
    ...asArray(result.missing_data).map((value) => ({
      label: "Fehlende Daten",
      value,
    })),
  ];
  const options = asArray(result.options);
  const missingFields = asArray(context.missing_fields);

  return (
    <section className="card analysis-card">
      <h2>{title}</h2>
      <dl className="marketplace-meta-grid">
        <div>
          <dt>Zeitraum</dt>
          <dd>{describeAnalysisItem(context.period_start)} – {describeAnalysisItem(context.period_end)}</dd>
        </div>
        <div><dt>Marketplace</dt><dd>{describeAnalysisItem(context.marketplace)}</dd></div>
        <div><dt>Reporttyp</dt><dd>{describeAnalysisItem(context.report_type)}</dd></div>
        <div><dt>Granularität</dt><dd>{describeAnalysisItem(context.granularity)}</dd></div>
        <div><dt>Parser-Version</dt><dd>{describeAnalysisItem(context.parser_version)}</dd></div>
        <div><dt>Datenfrische</dt><dd>{describeAnalysisItem(context.data_freshness)}</dd></div>
        <div><dt>Zeitzone</dt><dd>{describeAnalysisItem(context.source_timezone)}</dd></div>
        <div><dt>Währung</dt><dd>{describeAnalysisItem(context.currency)}</dd></div>
        <div>
          <dt>Fehlende Felder</dt>
          <dd>{missingFields.length > 0 ? missingFields.map(describeAnalysisItem).join(", ") : "keine"}</dd>
        </div>
      </dl>
      <KpiComparisonChart result={result} />
      <div className="analysis-separation">
        <section className="analysis-block">
          <h3>Fakten</h3>
          <AnalysisItems items={facts} emptyText="Noch keine belastbaren Fakten verfügbar." />
        </section>
        <section className="analysis-block">
          <h3>Belastbare Ableitungen</h3>
          <AnalysisItems
            items={derivations}
            emptyText="Noch keine belastbare Ableitung aus kompatiblen Zeiträumen."
          />
        </section>
        <section className="analysis-block">
          <h3>Hypothesen</h3>
          <AnalysisItems items={hypotheses} emptyText="Keine Hypothese aus den vorhandenen Daten." />
        </section>
        <section className="analysis-block">
          <h3>Offene Fragen</h3>
          <AnalysisItems items={openQuestions} emptyText="Keine offene Evidenzfrage ausgewiesen." />
        </section>
      </div>

      <p><strong>Gesamtunsicherheit:</strong> {String(result.uncertainty ?? "nicht bewertet")}</p>
      {options.length > 0 && (
        <>
          <h3>Mögliche Maßnahmen</h3>
          <ul>
            {options.map((option, index) => {
              const item = option as Record<string, unknown>;
              return (
                <li key={`${String(item.action)}-${index}`}>
                  <strong>{String(item.action)}</strong> · Wirkung: {String(item.expected_effect)} ·
                  Aufwand: {String(item.effort)} · Unsicherheit: {String(item.uncertainty)}
                  {Array.isArray(item.risks) && <div>Risiken: {item.risks.join(" ")}</div>}
                  {Array.isArray(item.evidence_refs) && (
                    <div className="marketplace-muted">
                      Evidenz: {item.evidence_refs.join(", ") || "keine direkte Evidenzreferenz"}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        </>
      )}
      {showWeeklyStrategy && <WeeklyStrategyPanel />}
      <div className="marketplace-actions" aria-label="Zusammenfassung exportieren">
        <button
          type="button"
          className="secondary"
          onClick={() => void downloadMarketplaceAnalysis(id, "json")}
        >
          PII-minimierten Analyseexport laden
        </button>
        <button
          type="button"
          className="secondary"
          onClick={() => void downloadMarketplaceAnalysis(id, "markdown")}
        >
          Markdown exportieren
        </button>
        <button
          type="button"
          className="secondary"
          onClick={() => void downloadMarketplaceAnalysis(id, "csv")}
        >
          CSV exportieren
        </button>
      </div>
      <p style={{ color: "var(--warning)" }}>
        Regelanalysen sind Entscheidungshilfen. Essentials+ Merchant nimmt keine Preis-, Werbe-,
        Listing-, Bestands- oder Bestelländerungen vor.
      </p>
    </section>
  );
}
