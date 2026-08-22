import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";

import { api } from "../api";
import type {
  AmazonProductMapping,
  AmazonProductMappingView,
  ObservedAmazonProduct,
} from "../types";

type MappingForm = {
  brand: AmazonProductMapping["brand"];
  productFamily: string;
  variant: string;
  packSize: string;
  sku: string;
  evidenceSource: AmazonProductMapping["evidence_source"];
  enabled: boolean;
};

const emptyForm: MappingForm = {
  brand: "sphagnum",
  productFamily: "Sphagnum-Moos",
  variant: "",
  packSize: "",
  sku: "",
  evidenceSource: "operator_confirmed",
  enabled: true,
};

const brandLabels: Record<AmazonProductMapping["brand"], string> = {
  mantle: "Mantle",
  sphagnum: "Sphagnum",
  shared: "Mantle / Sphagnum",
  other: "Sonstiges",
};

const evidenceLabels: Record<AmazonProductMapping["evidence_source"], string> = {
  mantle_wiki: "Mantle-Wiki",
  seller_central: "Seller Central bestätigt",
  operator_confirmed: "Manuell bestätigt",
};

function mappingFor(
  mappings: AmazonProductMapping[],
  observed: ObservedAmazonProduct,
): AmazonProductMapping | undefined {
  return mappings.find((mapping) =>
    mapping.connection_id === observed.connection_id
      && mapping.marketplace_id === observed.marketplace_id
      && mapping.child_asin === observed.child_asin);
}

function formFor(mapping?: AmazonProductMapping): MappingForm {
  if (!mapping) return { ...emptyForm };
  return {
    brand: mapping.brand,
    productFamily: mapping.product_family,
    variant: mapping.variant,
    packSize: mapping.pack_size ?? "",
    sku: mapping.sku ?? "",
    evidenceSource: mapping.evidence_source,
    enabled: mapping.enabled,
  };
}

export function ProductMappingPanel() {
  const [view, setView] = useState<AmazonProductMappingView | null>(null);
  const [selectedKey, setSelectedKey] = useState("");
  const [form, setForm] = useState<MappingForm>({ ...emptyForm });
  const [confirmed, setConfirmed] = useState(false);
  const [saving, setSaving] = useState(false);
  const [search, setSearch] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  const load = async () => {
    const loaded = await api.get<AmazonProductMappingView>("/marketplace/product-mappings");
    setView(loaded);
    setSelectedKey((current) => current || (loaded.observed[0]
      ? `${loaded.observed[0].connection_id}|${loaded.observed[0].marketplace_id}|${loaded.observed[0].child_asin}`
      : ""));
  };

  useEffect(() => {
    void load().catch(() => setMessage("Produktzuordnungen konnten nicht geladen werden."));
  }, []);

  const selected = useMemo(() => {
    if (!view || !selectedKey) return undefined;
    return view.observed.find((observed) =>
      `${observed.connection_id}|${observed.marketplace_id}|${observed.child_asin}` === selectedKey);
  }, [selectedKey, view]);

  useEffect(() => {
    if (!view || !selected) return;
    setForm(formFor(mappingFor(view.mappings, selected)));
    setConfirmed(false);
  }, [selected, view]);

  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (!selected) return;
    setSaving(true);
    setMessage(null);
    try {
      await api.post("/marketplace/product-mappings", {
        connection_id: selected.connection_id,
        marketplace_id: selected.marketplace_id,
        child_asin: selected.child_asin,
        brand: form.brand,
        product_family: form.productFamily,
        variant: form.variant,
        pack_size: form.packSize || null,
        sku: form.sku || null,
        evidence_source: form.evidenceSource,
        enabled: form.enabled,
        confirmed_business_mapping: confirmed,
      });
      await load();
      setConfirmed(false);
      setMessage("Zuordnung gespeichert. ASIN und SKU bleiben innerhalb des internen Systems.");
    } catch (error) {
      const code = error instanceof Error ? error.message : "mapping_failed";
      setMessage(`Zuordnung konnte nicht gespeichert werden (${code}).`);
    } finally {
      setSaving(false);
    }
  };

  const filteredMappings = (view?.mappings ?? []).filter((mapping) => {
    const needle = search.trim().toLocaleLowerCase("de");
    return !needle || [
      mapping.child_asin,
      mapping.product_family,
      mapping.variant,
      mapping.pack_size ?? "",
      mapping.sku ?? "",
    ].some((value) => value.toLocaleLowerCase("de").includes(needle));
  });

  return (
    <section className="card provider-settings product-mapping" aria-labelledby="product-mapping-heading">
      <div className="marketplace-section-heading">
        <div>
          <h2 id="product-mapping-heading">Produktzuordnung</h2>
          <p className="marketplace-muted">
            Verknüpft beobachtete Child-ASINs mit internen Produktnamen. Die KI erhält nur Labels
            und Aggregate, niemals ASIN oder SKU.
          </p>
        </div>
        <span className="badge">
          {view ? `${view.coverage.enabled_mapped_products}/${view.coverage.observed_products} aktiv` : "lädt …"}
        </span>
      </div>

      {message && <p className="marketplace-status" role="status">{message}</p>}
      {view && view.observed.length > 0 && (
        <form onSubmit={save} className="provider-form product-mapping-form">
          <label htmlFor="mapping-asin">
            Beobachtete Child-ASIN
            <select
              id="mapping-asin"
              value={selectedKey}
              onChange={(event) => setSelectedKey(event.target.value)}
            >
              {view.observed.map((observed) => {
                const mapping = mappingFor(view.mappings, observed);
                const key = `${observed.connection_id}|${observed.marketplace_id}|${observed.child_asin}`;
                return (
                  <option key={key} value={key}>
                    {observed.child_asin} · {mapping?.variant ?? "noch nicht zugeordnet"}
                  </option>
                );
              })}
            </select>
          </label>
          {selected && (
            <a
              className="pilot-settings-back"
              href={`https://www.amazon.de/dp/${selected.child_asin}`}
              target="_blank"
              rel="noreferrer"
            >
              Produkt bei Amazon prüfen ↗
            </a>
          )}
          <div className="provider-grid product-mapping-fields">
            <label htmlFor="mapping-brand">
              Bereich
              <select id="mapping-brand" value={form.brand} onChange={(event) => setForm({ ...form, brand: event.target.value as MappingForm["brand"] })}>
                {Object.entries(brandLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
              </select>
            </label>
            <label htmlFor="mapping-family">
              Produktfamilie
              <input id="mapping-family" required maxLength={80} value={form.productFamily} onChange={(event) => setForm({ ...form, productFamily: event.target.value })} />
            </label>
            <label htmlFor="mapping-variant">
              Produkt / Variante
              <input id="mapping-variant" required maxLength={120} value={form.variant} onChange={(event) => setForm({ ...form, variant: event.target.value })} placeholder="z. B. Sphagnum Moos Chile 1 kg" />
            </label>
            <label htmlFor="mapping-pack-size">
              Packungsgröße
              <input id="mapping-pack-size" maxLength={40} value={form.packSize} onChange={(event) => setForm({ ...form, packSize: event.target.value })} placeholder="z. B. 1 kg" />
            </label>
            <label htmlFor="mapping-sku">
              Interne SKU
              <input id="mapping-sku" maxLength={64} value={form.sku} onChange={(event) => setForm({ ...form, sku: event.target.value })} />
            </label>
            <label htmlFor="mapping-evidence">
              Bestätigt durch
              <select id="mapping-evidence" value={form.evidenceSource} onChange={(event) => setForm({ ...form, evidenceSource: event.target.value as MappingForm["evidenceSource"] })}>
                {Object.entries(evidenceLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
              </select>
            </label>
          </div>
          <label className="provider-checkbox">
            <input type="checkbox" checked={form.enabled} onChange={(event) => setForm({ ...form, enabled: event.target.checked })} />
            In die identifierfreie Produktanalyse aufnehmen.
          </label>
          <label className="provider-checkbox">
            <input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} />
            Die Zuordnung wurde anhand interner Unterlagen oder Seller Central geprüft.
          </label>
          <button type="submit" disabled={!selected || !form.variant || !form.productFamily || !confirmed || saving}>
            {saving ? "Wird gespeichert …" : "Zuordnung speichern"}
          </button>
        </form>
      )}

      {view && (
        <div className="product-mapping-list">
          <div className="marketplace-preview-header">
            <h3>Gespeicherte Zuordnungen</h3>
            <label className="product-mapping-search" htmlFor="mapping-search">
              <span className="sr-only">Zuordnungen durchsuchen</span>
              <input id="mapping-search" type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="ASIN, Variante oder SKU suchen" />
            </label>
          </div>
          <div className="table-scroll">
            <table>
              <thead><tr><th>Child-ASIN</th><th>Bereich</th><th>Produkt</th><th>Packung</th><th>Quelle</th><th>Status</th></tr></thead>
              <tbody>
                {filteredMappings.map((mapping) => (
                  <tr key={mapping.id}>
                    <td><code>{mapping.child_asin}</code></td>
                    <td>{brandLabels[mapping.brand]}</td>
                    <td>{mapping.variant}<br /><span className="marketplace-muted">{mapping.product_family}</span></td>
                    <td>{mapping.pack_size ?? "–"}</td>
                    <td>{evidenceLabels[mapping.evidence_source]}</td>
                    <td>{mapping.enabled ? "aktiv" : "pausiert"} · Rev. {mapping.revision}</td>
                  </tr>
                ))}
                {filteredMappings.length === 0 && <tr><td colSpan={6}>Noch keine passende Zuordnung.</td></tr>}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </section>
  );
}
