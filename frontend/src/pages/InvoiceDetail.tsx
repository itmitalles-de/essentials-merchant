import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { api, openInvoicePdf } from "../api";
import { useLanguage } from "../contexts/LanguageContext";
import { invoiceStatusLabel } from "../invoiceStatus";
import type { Article, Customer, Invoice, InvoiceLineItem, LineItemInput, VatRate } from "../types";

const emptyLineItem: LineItemInput = {
  description: "",
  article_id: null,
  quantity: "1",
  unit: "Stk",
  unit_price_net: "0",
  vat_rate_code: "STANDARD",
};

interface VatBreakdownRow {
  rate_percent: string;
  net_total: number;
  vat_total: number;
  gross_total: number;
}

function computeBreakdown(lineItems: InvoiceLineItem[]): VatBreakdownRow[] {
  const rows = new Map<string, VatBreakdownRow>();
  for (const li of lineItems) {
    const existing = rows.get(li.vat_rate_percent);
    const net = Number(li.net_amount);
    const vat = Number(li.vat_amount);
    const gross = Number(li.gross_amount);
    if (existing) {
      existing.net_total += net;
      existing.vat_total += vat;
      existing.gross_total += gross;
    } else {
      rows.set(li.vat_rate_percent, {
        rate_percent: li.vat_rate_percent,
        net_total: net,
        vat_total: vat,
        gross_total: gross,
      });
    }
  }
  return Array.from(rows.values()).sort((a, b) => Number(b.rate_percent) - Number(a.rate_percent));
}

export function InvoiceDetail() {
  const { t } = useLanguage();
  const { id } = useParams();
  const navigate = useNavigate();
  const [invoice, setInvoice] = useState<Invoice | null>(null);
  const [customers, setCustomers] = useState<Customer[]>([]);
  const [articles, setArticles] = useState<Article[]>([]);
  const [vatRates, setVatRates] = useState<VatRate[]>([]);
  const [showLineItemForm, setShowLineItemForm] = useState(false);
  const [lineItemForm, setLineItemForm] = useState<LineItemInput>(emptyLineItem);
  const [editingLineItemId, setEditingLineItemId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    if (!id) return;
    api.get<Invoice>(`/invoices/${id}`).then(setInvoice);
  };

  useEffect(() => {
    load();
    api.get<VatRate[]>("/vat-rates").then(setVatRates);
    api.get<Customer[]>("/customers").then(setCustomers);
    api.get<Article[]>("/articles").then(setArticles);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  if (!invoice) return null;

  const isDraft = invoice.status === "draft";

  const runAction = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch {
      setError(t("invoiceDetail.error"));
    } finally {
      setBusy(false);
    }
  };

  const transition = (status: string) =>
    runAction(async () => {
      await api.post(`/invoices/${invoice.id}/status`, { status });
      load();
    });

  const removeInvoice = () =>
    runAction(async () => {
      await api.delete(`/invoices/${invoice.id}`);
      navigate("/invoices");
    });

  const startEditLineItem = (li: InvoiceLineItem) => {
    setEditingLineItemId(li.id);
    setLineItemForm({
      description: li.description,
      article_id: li.article_id,
      quantity: li.quantity,
      unit: li.unit,
      unit_price_net: li.unit_price_net,
      vat_rate_code: li.vat_rate_code,
    });
    setShowLineItemForm(true);
  };

  const selectArticle = (articleId: string) => {
    if (!articleId) {
      setLineItemForm({ ...lineItemForm, article_id: null });
      return;
    }

    const article = articles.find((candidate) => candidate.id === articleId);
    if (!article) return;

    setLineItemForm({
      ...lineItemForm,
      article_id: article.id,
      description: article.name,
      unit: article.unit,
      unit_price_net: article.sales_price_net,
      vat_rate_code: article.default_vat_rate_code,
    });
  };

  const submitLineItem = (e: FormEvent) => {
    e.preventDefault();
    return runAction(async () => {
      if (editingLineItemId) {
        await api.put(`/invoices/${invoice.id}/line-items/${editingLineItemId}`, lineItemForm);
      } else {
        await api.post(`/invoices/${invoice.id}/line-items`, lineItemForm);
      }
      setShowLineItemForm(false);
      setEditingLineItemId(null);
      setLineItemForm(emptyLineItem);
      load();
    });
  };

  const removeLineItem = (lineItemId: string) =>
    runAction(async () => {
      await api.delete(`/invoices/${invoice.id}/line-items/${lineItemId}`);
      load();
    });

  const breakdown = computeBreakdown(invoice.line_items);
  const customerName =
    invoice.customer_snapshot?.name ??
    customers.find((c) => c.id === invoice.customer_id)?.name ??
    "";

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1rem", maxWidth: 800 }}>
      <h2 style={{ margin: 0 }}>
        {t("invoiceDetail.title")} {invoice.invoice_number ?? t("invoices.draft")}
      </h2>

      <div className="card" style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
        <div>
          <strong>{t("invoiceDetail.customer")}</strong> {customerName}
        </div>
        {invoice.issue_date && (
          <div>
            <strong>{t("invoiceDetail.date")}</strong> {invoice.issue_date} — <strong>{t("invoiceDetail.due")}</strong>{" "}
            {invoice.due_date}
          </div>
        )}
        <div>
          <strong>{t("invoiceDetail.status")}</strong>{" "}
          <span className={`badge ${invoice.status}`}>{invoiceStatusLabel(t, invoice.status)}</span>
        </div>
        <div>
          <strong>{t("invoiceDetail.total")}</strong> {invoice.gross_total} €
        </div>
      </div>

      <table className="card">
        <thead>
          <tr>
            <th>{t("invoiceDetail.colDescription")}</th>
            <th>{t("invoiceDetail.colQuantity")}</th>
            <th>{t("invoiceDetail.colUnitPrice")}</th>
            <th>{t("invoiceDetail.colVatRate")}</th>
            <th>{t("invoiceDetail.colNet")}</th>
            <th>{t("invoiceDetail.colGross")}</th>
            {isDraft && <th></th>}
          </tr>
        </thead>
        <tbody>
          {invoice.line_items.map((li) => (
            <tr key={li.id}>
              <td>{li.description}</td>
              <td>
                {li.quantity} {li.unit}
              </td>
              <td>{li.unit_price_net} €</td>
              <td>{li.vat_rate_percent} %</td>
              <td>{li.net_amount} €</td>
              <td>{li.gross_amount} €</td>
              {isDraft && (
                <td style={{ display: "flex", gap: "0.4rem" }}>
                  <button className="secondary" onClick={() => startEditLineItem(li)}>
                    {t("customers.edit")}
                  </button>
                  <button className="danger" onClick={() => removeLineItem(li.id)}>
                    {t("customers.delete")}
                  </button>
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>

      {isDraft && (
        <div className="card" style={{ display: "flex", flexDirection: "column", gap: "0.6rem" }}>
          {!showLineItemForm ? (
            <button
              onClick={() => {
                setEditingLineItemId(null);
                setLineItemForm(emptyLineItem);
                setShowLineItemForm(true);
              }}
            >
              {t("invoiceDetail.addLineItem")}
            </button>
          ) : (
            <form onSubmit={submitLineItem} style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.6rem" }}>
              <select
                aria-label={t("invoiceDetail.article")}
                style={{ gridColumn: "1 / -1" }}
                value={lineItemForm.article_id ?? ""}
                onChange={(e) => selectArticle(e.target.value)}
              >
                <option value="">{t("invoiceDetail.customLineItem")}</option>
                {articles
                  .filter((article) => article.active || article.id === lineItemForm.article_id)
                  .map((article) => (
                    <option key={article.id} value={article.id}>
                      {article.sku} — {article.name}
                    </option>
                  ))}
              </select>
              <input
                placeholder={t("invoiceDetail.colDescription")}
                required
                style={{ gridColumn: "1 / -1" }}
                value={lineItemForm.description}
                onChange={(e) => setLineItemForm({ ...lineItemForm, description: e.target.value })}
              />
              <input
                placeholder={t("invoiceDetail.colQuantity")}
                type="number"
                step="0.01"
                value={lineItemForm.quantity}
                onChange={(e) => setLineItemForm({ ...lineItemForm, quantity: e.target.value })}
              />
              <input
                placeholder={t("invoiceDetail.unit")}
                value={lineItemForm.unit}
                onChange={(e) => setLineItemForm({ ...lineItemForm, unit: e.target.value })}
              />
              <input
                placeholder={t("invoiceDetail.colUnitPrice")}
                type="number"
                step="0.01"
                value={lineItemForm.unit_price_net}
                onChange={(e) => setLineItemForm({ ...lineItemForm, unit_price_net: e.target.value })}
              />
              <select
                value={lineItemForm.vat_rate_code}
                onChange={(e) => setLineItemForm({ ...lineItemForm, vat_rate_code: e.target.value })}
              >
                {vatRates.map((r) => (
                  <option key={r.code} value={r.code}>
                    {r.rate_percent} %
                  </option>
                ))}
              </select>
              <div style={{ gridColumn: "1 / -1", display: "flex", gap: "0.5rem" }}>
                <button type="submit" disabled={busy}>
                  {editingLineItemId ? t("customers.save") : t("invoiceDetail.addLineItem")}
                </button>
                <button type="button" className="secondary" onClick={() => setShowLineItemForm(false)}>
                  {t("customers.cancel")}
                </button>
              </div>
            </form>
          )}
        </div>
      )}

      {breakdown.length > 0 && (
        <table className="card">
          <thead>
            <tr>
              <th>{t("invoiceDetail.colVatRate")}</th>
              <th>{t("invoiceDetail.colNet")}</th>
              <th>{t("invoiceDetail.colVatAmount")}</th>
              <th>{t("invoiceDetail.colGross")}</th>
            </tr>
          </thead>
          <tbody>
            {breakdown.map((row) => (
              <tr key={row.rate_percent}>
                <td>{row.rate_percent} %</td>
                <td>{row.net_total.toFixed(2)} €</td>
                <td>{row.vat_total.toFixed(2)} €</td>
                <td>{row.gross_total.toFixed(2)} €</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {error && <div style={{ color: "var(--danger)" }}>{error}</div>}

      <div style={{ display: "flex", gap: "0.6rem" }}>
        {invoice.status !== "draft" && (
          <button
            className="secondary"
            onClick={() => openInvoicePdf(invoice.id, invoice.invoice_number ?? invoice.id)}
          >
            {t("invoiceDetail.downloadPdf")}
          </button>
        )}
        {invoice.status === "draft" && (
          <button onClick={() => transition("sent")} disabled={busy || invoice.line_items.length === 0}>
            {t("invoiceDetail.send")}
          </button>
        )}
        {(invoice.status === "sent" || invoice.status === "overdue") && (
          <button onClick={() => transition("paid")} disabled={busy}>
            {t("invoiceDetail.markPaid")}
          </button>
        )}
        {(invoice.status === "draft" || invoice.status === "sent" || invoice.status === "overdue") && (
          <button className="secondary" onClick={() => transition("cancelled")} disabled={busy}>
            {t("invoiceDetail.cancel")}
          </button>
        )}
        {invoice.status === "draft" && (
          <button className="danger" onClick={removeInvoice} disabled={busy}>
            {t("customers.delete")}
          </button>
        )}
      </div>
    </div>
  );
}
