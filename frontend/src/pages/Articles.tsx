import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Link } from "react-router-dom";
import { api } from "../api";
import { useLanguage } from "../contexts/LanguageContext";
import type { Article, ArticleInput, VatRate } from "../types";

const empty: ArticleInput = {
  sku: "",
  name: "",
  unit: "Stk",
  sales_price_net: "0",
  default_vat_rate_code: "STANDARD",
  purchase_price_net: null,
  min_stock_quantity: null,
  active: true,
};

export function Articles() {
  const { t } = useLanguage();
  const [articles, setArticles] = useState<Article[]>([]);
  const [vatRates, setVatRates] = useState<VatRate[]>([]);
  const [form, setForm] = useState<ArticleInput>(empty);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);

  const load = () => api.get<Article[]>("/articles").then(setArticles);

  useEffect(() => {
    load();
    api.get<VatRate[]>("/vat-rates").then(setVatRates);
  }, []);

  const startEdit = (a: Article) => {
    setEditingId(a.id);
    setForm({
      sku: a.sku,
      name: a.name,
      unit: a.unit,
      sales_price_net: a.sales_price_net,
      default_vat_rate_code: a.default_vat_rate_code,
      purchase_price_net: a.purchase_price_net,
      min_stock_quantity: a.min_stock_quantity,
      active: a.active,
    });
    setShowForm(true);
  };

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (editingId) {
      await api.put(`/articles/${editingId}`, form);
    } else {
      await api.post("/articles", form);
    }
    setForm(empty);
    setEditingId(null);
    setShowForm(false);
    load();
  };

  const remove = async (id: string) => {
    if (!confirm(t("articles.confirmDelete"))) return;
    await api.delete(`/articles/${id}`);
    load();
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h2 style={{ margin: 0 }}>{t("articles.title")}</h2>
        <button
          onClick={() => {
            setForm(empty);
            setEditingId(null);
            setShowForm((v) => !v);
          }}
        >
          {showForm ? t("articles.cancel") : t("articles.new")}
        </button>
      </div>

      {showForm && (
        <form
          onSubmit={onSubmit}
          className="card"
          style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.6rem" }}
        >
          <input
            placeholder={t("articles.sku")}
            required
            value={form.sku}
            onChange={(e) => setForm({ ...form, sku: e.target.value })}
          />
          <input
            placeholder={t("articles.name")}
            required
            value={form.name}
            onChange={(e) => setForm({ ...form, name: e.target.value })}
          />
          <input
            placeholder={t("articles.unit")}
            value={form.unit}
            onChange={(e) => setForm({ ...form, unit: e.target.value })}
          />
          <input
            placeholder={t("articles.salesPrice")}
            type="number"
            step="0.01"
            value={form.sales_price_net}
            onChange={(e) => setForm({ ...form, sales_price_net: e.target.value })}
          />
          <select
            value={form.default_vat_rate_code}
            onChange={(e) => setForm({ ...form, default_vat_rate_code: e.target.value })}
          >
            {vatRates.map((r) => (
              <option key={r.code} value={r.code}>
                {r.rate_percent} %
              </option>
            ))}
          </select>
          <input
            placeholder={t("articles.purchasePrice")}
            type="number"
            step="0.01"
            value={form.purchase_price_net ?? ""}
            onChange={(e) =>
              setForm({ ...form, purchase_price_net: e.target.value === "" ? null : e.target.value })
            }
          />
          <input
            placeholder={t("articles.minStock")}
            type="number"
            step="0.01"
            value={form.min_stock_quantity ?? ""}
            onChange={(e) =>
              setForm({ ...form, min_stock_quantity: e.target.value === "" ? null : e.target.value })
            }
          />
          <label style={{ display: "flex", alignItems: "center", gap: "0.4rem" }}>
            <input
              type="checkbox"
              checked={form.active}
              onChange={(e) => setForm({ ...form, active: e.target.checked })}
            />
            {t("customers.active")}
          </label>
          <button type="submit" style={{ gridColumn: "1 / -1" }}>
            {editingId ? t("customers.save") : t("customers.create")}
          </button>
        </form>
      )}

      <table className="card">
        <thead>
          <tr>
            <th>{t("articles.sku")}</th>
            <th>{t("articles.name")}</th>
            <th>{t("articles.salesPrice")}</th>
            <th>{t("articles.stock")}</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {articles.map((a) => (
            <tr key={a.id}>
              <td>
                <Link to={`/articles/${a.id}`} className="btn btn-sm">
                  {a.sku}
                </Link>
              </td>
              <td>{a.name}</td>
              <td>
                {a.sales_price_net} € / {a.unit}
              </td>
              <td
                style={
                  a.min_stock_quantity && Number(a.stock_quantity) <= Number(a.min_stock_quantity)
                    ? { color: "var(--warning)", fontWeight: 600 }
                    : undefined
                }
              >
                {a.stock_quantity} {a.unit}
              </td>
              <td style={{ display: "flex", gap: "0.4rem" }}>
                <button className="secondary" onClick={() => startEdit(a)}>
                  {t("customers.edit")}
                </button>
                <button className="danger" onClick={() => remove(a.id)}>
                  {t("customers.delete")}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
