import { useEffect, useState } from "react";
import type { FormEvent } from "react";

import { api } from "../api";
import type { PilotProviderSecretsStatus } from "../types";

function formatDate(value: string | null): string {
  if (!value) return "–";
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime())
    ? value
    : new Intl.DateTimeFormat("de-DE", {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(parsed);
}

function providerSettingsError(error: unknown): string {
  const code = error instanceof Error ? error.message : "provider_secret_store_failed";
  const messages: Record<string, string> = {
    provider_secret_invalid: "Die Zugangsdaten sind unvollständig oder haben ein ungültiges Format.",
    provider_secret_store_unavailable: "Der hostseitige Verschlüsselungsschlüssel fehlt.",
    provider_secret_store_failed: "Zugangsdaten konnten sicher nicht gespeichert werden.",
  };
  return messages[code] ?? `Zugangsdaten konnten nicht gespeichert werden (${code}).`;
}

export function ProviderSettingsPanel({ onConfigured }: { onConfigured: () => Promise<void> }) {
  const [status, setStatus] = useState<PilotProviderSecretsStatus | null>(null);
  const [openAiKey, setOpenAiKey] = useState("");
  const [billingConfirmed, setBillingConfirmed] = useState(false);
  const [amazon, setAmazon] = useState({
    lwaClientId: "",
    lwaClientSecret: "",
    lwaRefreshToken: "",
    sellerId: "",
    marketplaceId: "",
    region: "eu",
    authorized: false,
    readOnly: false,
  });
  const [saving, setSaving] = useState<"openai" | "amazon" | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const loadStatus = async () => {
    setStatus(await api.get<PilotProviderSecretsStatus>("/pilot/provider-secrets/status"));
  };

  useEffect(() => {
    void loadStatus().catch(() => setMessage("Zugangsdatenstatus konnte nicht geladen werden."));
  }, []);

  const saveOpenAi = async (event: FormEvent) => {
    event.preventDefault();
    setSaving("openai");
    setMessage(null);
    try {
      await api.post("/pilot/provider-secrets/openai", { api_key: openAiKey });
      setOpenAiKey("");
      setBillingConfirmed(false);
      await loadStatus();
      await onConfigured();
      setMessage("OpenAI-Zugang wurde gespeichert. Der Wert kann nicht wieder angezeigt werden.");
    } catch (error) {
      setMessage(providerSettingsError(error));
    } finally {
      setSaving(null);
    }
  };

  const saveAmazon = async (event: FormEvent) => {
    event.preventDefault();
    setSaving("amazon");
    setMessage(null);
    try {
      await api.post("/pilot/provider-secrets/amazon", {
        lwa_client_id: amazon.lwaClientId,
        lwa_client_secret: amazon.lwaClientSecret,
        lwa_refresh_token: amazon.lwaRefreshToken,
        seller_id: amazon.sellerId,
        marketplace_id: amazon.marketplaceId,
        region: amazon.region,
        confirm_authorized: amazon.authorized,
        confirm_read_only: amazon.readOnly,
      });
      setAmazon((current) => ({
        ...current,
        lwaClientId: "",
        lwaClientSecret: "",
        lwaRefreshToken: "",
        sellerId: "",
        marketplaceId: "",
        authorized: false,
        readOnly: false,
      }));
      await loadStatus();
      await onConfigured();
      setMessage("Amazon-Zugang wurde gespeichert und für einen read-only Sales-&-Traffic-Abruf freigegeben.");
    } catch (error) {
      setMessage(providerSettingsError(error));
    } finally {
      setSaving(null);
    }
  };

  const amazonComplete = Boolean(
    amazon.lwaClientId
      && amazon.lwaClientSecret
      && amazon.lwaRefreshToken
      && amazon.sellerId
      && amazon.marketplaceId
      && amazon.authorized
      && amazon.readOnly,
  );

  return (
    <section className="card provider-settings" aria-labelledby="provider-settings-heading">
      <div className="marketplace-section-heading">
        <div>
          <h2 id="provider-settings-heading">Zugänge</h2>
          <p className="marketplace-muted">
            Write-only: Werte lassen sich setzen oder ersetzen, aber weder Browser noch API
            können einen gespeicherten Wert zurücklesen.
          </p>
        </div>
        <span className="badge">LAN/VPN · ohne Login</span>
      </div>
      <div className="marketplace-callout warning">
        <strong>Interne Vertrauensgrenze</strong>
        <p>
          Jede Person mit Zugriff auf diese interne Route kann Zugangsdaten ersetzen. Secret-Werte
          erscheinen nie wieder in der Oberfläche, in Exporten oder Pilot-Backups.
        </p>
      </div>
      {message && <p className="marketplace-status" role="status">{message}</p>}
      {!status?.storage_available && (
        <p className="marketplace-callout warning" role="alert">
          Der hostseitige Verschlüsselungsschlüssel ist noch nicht verfügbar.
        </p>
      )}
      <div className="provider-grid">
        <form onSubmit={saveOpenAi} className="provider-form">
          <div className="marketplace-preview-header">
            <h3>OpenAI · Pay per use</h3>
            <span className="badge">{status?.openai.configured ? "konfiguriert" : "fehlt"}</span>
          </div>
          <p className="marketplace-muted">
            {status?.openai.updated_at
              ? `Zuletzt ersetzt: ${formatDate(status.openai.updated_at)}`
              : "Noch kein API-Key gespeichert."}
          </p>
          <p className="marketplace-muted">
            Im eigenen OpenAI-API-Projekt einen Project Key erzeugen und dort
            Pay-per-use-Billing/Budget setzen. ChatGPT Pro ist davon getrennt.{" "}
            <a
              href="https://platform.openai.com/docs/quickstart/make-your-first-api-request"
              target="_blank"
              rel="noreferrer"
            >
              Offizielle Anleitung
            </a>
          </p>
          <label htmlFor="openai-api-key">
            Neuer Project API-Key
            <input
              id="openai-api-key"
              type="password"
              autoComplete="new-password"
              required
              value={openAiKey}
              onChange={(event) => setOpenAiKey(event.target.value)}
              placeholder="sk-proj-…"
            />
          </label>
          <label className="provider-checkbox">
            <input
              type="checkbox"
              checked={billingConfirmed}
              onChange={(event) => setBillingConfirmed(event.target.checked)}
            />
            Separates API-Pay-per-use-Budget ist eingerichtet.
          </label>
          <button
            type="submit"
            disabled={
              !status?.storage_available || !openAiKey || !billingConfirmed || saving !== null
            }
          >
            {saving === "openai" ? "Wird gespeichert …" : "OpenAI-Key setzen/ersetzen"}
          </button>
        </form>

        <form onSubmit={saveAmazon} className="provider-form">
          <div className="marketplace-preview-header">
            <h3>Amazon SP-API</h3>
            <span className="badge">{status?.amazon.configured ? "konfiguriert" : "fehlt"}</span>
          </div>
          <p className="marketplace-muted">
            {status?.amazon.updated_at
              ? `Zuletzt ersetzt: ${formatDate(status.amazon.updated_at)}`
              : "Noch keine LWA-Zugangsdaten gespeichert."}
          </p>
          <p className="marketplace-muted">
            In Seller Central eine private SP-API-App registrieren, LWA Client
            ID/Secret öffnen und die App für Mantle selbst autorisieren; dabei
            entsteht der Refresh Token.{" "}
            <a
              href="https://developer-docs.amazon.com/sp-api/docs/register-as-a-private-developer"
              target="_blank"
              rel="noreferrer"
            >
              Amazon-Anleitung
            </a>
          </p>
          <label htmlFor="amazon-client-id">
            LWA Client ID
            <input id="amazon-client-id" type="password" autoComplete="new-password" required value={amazon.lwaClientId} onChange={(event) => setAmazon({ ...amazon, lwaClientId: event.target.value })} />
          </label>
          <label htmlFor="amazon-client-secret">
            LWA Client Secret
            <input id="amazon-client-secret" type="password" autoComplete="new-password" required value={amazon.lwaClientSecret} onChange={(event) => setAmazon({ ...amazon, lwaClientSecret: event.target.value })} />
          </label>
          <label htmlFor="amazon-refresh-token">
            LWA Refresh Token
            <input id="amazon-refresh-token" type="password" autoComplete="new-password" required value={amazon.lwaRefreshToken} onChange={(event) => setAmazon({ ...amazon, lwaRefreshToken: event.target.value })} />
          </label>
          <label htmlFor="amazon-seller-id">
            Seller ID
            <input id="amazon-seller-id" type="password" autoComplete="new-password" required value={amazon.sellerId} onChange={(event) => setAmazon({ ...amazon, sellerId: event.target.value })} />
          </label>
          <label htmlFor="amazon-marketplace-id">
            Marketplace ID
            <input id="amazon-marketplace-id" type="password" autoComplete="new-password" required value={amazon.marketplaceId} onChange={(event) => setAmazon({ ...amazon, marketplaceId: event.target.value })} />
          </label>
          <label htmlFor="amazon-region">
            SP-API-Region
            <select id="amazon-region" value={amazon.region} onChange={(event) => setAmazon({ ...amazon, region: event.target.value })}>
              <option value="eu">EU</option>
              <option value="na">NA</option>
              <option value="fe">FE</option>
            </select>
          </label>
          <label className="provider-checkbox">
            <input type="checkbox" checked={amazon.authorized} onChange={(event) => setAmazon({ ...amazon, authorized: event.target.checked })} />
            Mantle hat diese private App selbst autorisiert.
          </label>
          <label className="provider-checkbox">
            <input type="checkbox" checked={amazon.readOnly} onChange={(event) => setAmazon({ ...amazon, readOnly: event.target.checked })} />
            Nur LWA und Reports API für Sales &amp; Traffic; keine Mutation.
          </label>
          <button
            type="submit"
            disabled={!status?.storage_available || !amazonComplete || saving !== null}
          >
            {saving === "amazon" ? "Wird gespeichert …" : "Amazon-Zugang setzen/ersetzen"}
          </button>
        </form>
      </div>
    </section>
  );
}
