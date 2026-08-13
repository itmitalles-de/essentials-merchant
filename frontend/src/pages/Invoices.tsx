import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { api } from "../api";
import { useLanguage } from "../contexts/LanguageContext";
import { invoiceStatusLabel } from "../invoiceStatus";
import type { Customer, Invoice, InvoiceListItem } from "../types";

export function Invoices() {
  const { t } = useLanguage();
  const navigate = useNavigate();
  const [invoices, setInvoices] = useState<InvoiceListItem[]>([]);
  const [customers, setCustomers] = useState<Customer[]>([]);
  const [showCreate, setShowCreate] = useState(false);
  const [selectedCustomerId, setSelectedCustomerId] = useState("");
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    api.get<InvoiceListItem[]>("/invoices").then(setInvoices);
    api.get<Customer[]>("/customers").then(setCustomers);
  }, []);

  const createInvoice = async () => {
    if (!selectedCustomerId) return;
    setCreating(true);
    try {
      const invoice = await api.post<Invoice>("/invoices", {
        customer_id: selectedCustomerId,
        notes: "",
      });
      navigate(`/invoices/${invoice.id}`);
    } finally {
      setCreating(false);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h2 style={{ margin: 0 }}>{t("invoices.title")}</h2>
        <button
          onClick={() => {
            setSelectedCustomerId("");
            setShowCreate((v) => !v);
          }}
        >
          {showCreate ? t("invoices.cancel") : t("invoices.new")}
        </button>
      </div>

      {showCreate && (
        <div className="card" style={{ display: "flex", gap: "0.6rem" }}>
          <select
            value={selectedCustomerId}
            onChange={(e) => setSelectedCustomerId(e.target.value)}
            style={{ flex: 1 }}
          >
            <option value="">{t("invoices.chooseCustomer")}</option>
            {customers.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
          <button onClick={createInvoice} disabled={!selectedCustomerId || creating}>
            {t("invoices.create")}
          </button>
        </div>
      )}

      <table className="card">
        <thead>
          <tr>
            <th>{t("invoices.colNumber")}</th>
            <th>{t("invoices.colCustomer")}</th>
            <th>{t("invoices.colIssueDate")}</th>
            <th>{t("invoices.colDueDate")}</th>
            <th>{t("invoices.colAmount")}</th>
            <th>{t("invoices.colStatus")}</th>
          </tr>
        </thead>
        <tbody>
          {invoices.map((inv) => (
            <tr key={inv.id}>
              <td>
                <Link to={`/invoices/${inv.id}`} className="btn btn-sm">
                  {inv.invoice_number ?? t("invoices.draft")}
                </Link>
                {inv.document_type === "correction" && <div style={{ fontSize: "0.75rem" }}>Korrektur zu {inv.corrected_invoice_number}</div>}
              </td>
              <td>{inv.customer_name}</td>
              <td>{inv.issue_date ?? "—"}</td>
              <td>{inv.due_date ?? "—"}</td>
              <td>{inv.gross_total} €</td>
              <td>
                <span className={`badge ${inv.status}`}>{invoiceStatusLabel(t, inv.status)}</span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
