import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { api } from "../api";
import { useLanguage } from "../contexts/LanguageContext";
import type { Article, CreateSalesOrderInput, Customer, SalesChannel, SalesOrder, ShippingCarrier } from "../types";

const empty: CreateSalesOrderInput = {
  customer_id: "",
  source: "manual",
  external_order_id: null,
  shipping_carrier: null,
  tracking_number: "",
  notes: "",
  items: [{ article_id: null, description: "", quantity: "1", unit: "Stk" }],
};

export function SalesOrders() {
  const { t } = useLanguage();
  const [orders, setOrders] = useState<SalesOrder[]>([]);
  const [customers, setCustomers] = useState<Customer[]>([]);
  const [articles, setArticles] = useState<Article[]>([]);
  const [form, setForm] = useState<CreateSalesOrderInput>(empty);
  const [showForm, setShowForm] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = () => api.get<SalesOrder[]>("/sales-orders").then(setOrders);
  useEffect(() => {
    load();
    api.get<Customer[]>("/customers").then(setCustomers);
    api.get<Article[]>("/articles").then(setArticles);
  }, []);

  const selectArticle = (id: string) => {
    const article = articles.find((candidate) => candidate.id === id);
    setForm({
      ...form,
      items: [{ ...form.items[0], article_id: article?.id ?? null, description: article?.name ?? "", unit: article?.unit ?? "Stk" }],
    });
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    try {
      await api.post("/sales-orders", form);
      setForm(empty);
      setShowForm(false);
      load();
    } finally {
      setBusy(false);
    }
  };

  const channelLabel = (source: SalesChannel) => t(`orders.${source}`);

  return <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
      <h2 style={{ margin: 0 }}>{t("orders.title")}</h2>
      <button onClick={() => { setForm(empty); setShowForm((value) => !value); }}>{showForm ? t("orders.cancel") : t("orders.new")}</button>
    </div>
    {showForm && <form className="card" onSubmit={submit} style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.6rem" }}>
      <select required value={form.customer_id} onChange={(event) => setForm({ ...form, customer_id: event.target.value })}>
        <option value="">{t("orders.customer")}</option>{customers.filter((customer) => customer.active).map((customer) => <option key={customer.id} value={customer.id}>{customer.name}</option>)}
      </select>
      <select value={form.source} onChange={(event) => setForm({ ...form, source: event.target.value as SalesChannel })}>
        {(["manual", "woocommerce", "amazon", "ebay"] as SalesChannel[]).map((source) => <option key={source} value={source}>{channelLabel(source)}</option>)}
      </select>
      <input placeholder={t("orders.externalId")} value={form.external_order_id ?? ""} onChange={(event) => setForm({ ...form, external_order_id: event.target.value || null })} />
      <select value={form.shipping_carrier ?? ""} onChange={(event) => setForm({ ...form, shipping_carrier: (event.target.value || null) as ShippingCarrier | null })}>
        <option value="">{t("orders.noCarrier")}</option>{(["dhl", "hermes", "dpd"] as ShippingCarrier[]).map((carrier) => <option key={carrier} value={carrier}>{carrier.toUpperCase()}</option>)}
      </select>
      <select value={form.items[0].article_id ?? ""} onChange={(event) => selectArticle(event.target.value)}>
        <option value="">{t("orders.item")}</option>{articles.filter((article) => article.active).map((article) => <option key={article.id} value={article.id}>{article.sku} — {article.name}</option>)}
      </select>
      <input required placeholder={t("orders.item")} value={form.items[0].description} onChange={(event) => setForm({ ...form, items: [{ ...form.items[0], description: event.target.value }] })} />
      <input required type="number" min="0.01" step="0.01" placeholder={t("orders.quantity")} value={form.items[0].quantity} onChange={(event) => setForm({ ...form, items: [{ ...form.items[0], quantity: event.target.value }] })} />
      <input placeholder={t("orders.tracking")} value={form.tracking_number} onChange={(event) => setForm({ ...form, tracking_number: event.target.value })} />
      <button type="submit" disabled={busy} style={{ gridColumn: "1 / -1" }}>{t("orders.create")}</button>
    </form>}
    <table className="card"><thead><tr><th>{t("orders.colNumber")}</th><th>{t("orders.colCustomer")}</th><th>{t("orders.colSource")}</th><th>{t("orders.colCarrier")}</th><th>{t("orders.colStatus")}</th></tr></thead>
      <tbody>{orders.map((order) => <tr key={order.id}><td>{order.order_number}</td><td>{order.customer_name}</td><td>{channelLabel(order.source)}</td><td>{order.shipping_carrier?.toUpperCase() ?? "—"}</td><td>{order.status}</td></tr>)}</tbody>
    </table>
  </div>;
}
