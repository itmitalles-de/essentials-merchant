import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../api";
import { useLanguage } from "../contexts/LanguageContext";
import type { Article, ManualAdjustmentInput, StockMovement } from "../types";

const emptyMovement: ManualAdjustmentInput = {
  movement_type: "in",
  quantity: "",
  note: "",
};

export function ArticleDetail() {
  const { t } = useLanguage();
  const { id } = useParams();
  const [article, setArticle] = useState<Article | null>(null);
  const [movements, setMovements] = useState<StockMovement[]>([]);
  const [form, setForm] = useState<ManualAdjustmentInput>(emptyMovement);
  const [showForm, setShowForm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    if (!id) return;
    api.get<Article>(`/articles/${id}`).then(setArticle);
    api.get<StockMovement[]>(`/articles/${id}/stock-movements`).then(setMovements);
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  if (!article) return null;

  const isLowStock =
    article.min_stock_quantity !== null && Number(article.stock_quantity) <= Number(article.min_stock_quantity);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!id) return;

    setBusy(true);
    setError(null);
    try {
      await api.post(`/articles/${id}/stock-movements`, form);
      setForm(emptyMovement);
      setShowForm(false);
      load();
    } catch {
      setError(t("invoiceDetail.error"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1rem", maxWidth: 900 }}>
      <Link to="/articles">← {t("articles.back")}</Link>

      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "1rem" }}>
        <div>
          <h2 style={{ margin: 0 }}>{article.name}</h2>
          <div style={{ color: "var(--fg-muted)", marginTop: "0.35rem" }}>{article.sku}</div>
        </div>
        <button onClick={() => setShowForm((value) => !value)}>
          {showForm ? t("articles.cancel") : t("articles.adjustStock")}
        </button>
      </div>

      <div
        className="card"
        style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))", gap: "1rem" }}
      >
        <div>
          <div style={{ color: "var(--fg-muted)", fontSize: "0.85rem" }}>{t("articles.stock")}</div>
          <strong style={isLowStock ? { color: "var(--warning)" } : undefined}>
            {article.stock_quantity} {article.unit}
          </strong>
        </div>
        <div>
          <div style={{ color: "var(--fg-muted)", fontSize: "0.85rem" }}>{t("articles.salesPrice")}</div>
          <strong>{article.sales_price_net} €</strong>
        </div>
        {article.min_stock_quantity !== null && (
          <div>
            <div style={{ color: "var(--fg-muted)", fontSize: "0.85rem" }}>{t("articles.minStock")}</div>
            <strong>{article.min_stock_quantity} {article.unit}</strong>
          </div>
        )}
      </div>

      {showForm && (
        <form
          className="card"
          onSubmit={submit}
          style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.6rem" }}
        >
          <label>
            {t("articles.movementType")}
            <select
              value={form.movement_type}
              onChange={(event) =>
                setForm({ ...form, movement_type: event.target.value as ManualAdjustmentInput["movement_type"] })
              }
            >
              <option value="in">{t("articles.movement.in")}</option>
              <option value="out">{t("articles.movement.out")}</option>
              <option value="adjustment">{t("articles.movement.adjustment")}</option>
            </select>
          </label>
          <label>
            {t("articles.quantity")}
            <input
              required
              type="number"
              step="0.01"
              min={form.movement_type === "adjustment" ? undefined : "0.01"}
              value={form.quantity}
              onChange={(event) => setForm({ ...form, quantity: event.target.value })}
            />
          </label>
          <label style={{ gridColumn: "1 / -1" }}>
            {t("articles.note")}
            <input value={form.note} onChange={(event) => setForm({ ...form, note: event.target.value })} />
          </label>
          <div style={{ gridColumn: "1 / -1", display: "flex", gap: "0.5rem" }}>
            <button type="submit" disabled={busy}>
              {t("articles.saveMovement")}
            </button>
            <button type="button" className="secondary" onClick={() => setShowForm(false)}>
              {t("articles.cancel")}
            </button>
          </div>
        </form>
      )}

      {error && <div style={{ color: "var(--danger)" }}>{error}</div>}

      <div>
        <h3>{t("articles.movementHistory")}</h3>
        {movements.length === 0 ? (
          <div className="card">{t("articles.noMovements")}</div>
        ) : (
          <table className="card">
            <thead>
              <tr>
                <th>{t("articles.colDate")}</th>
                <th>{t("articles.colMovement")}</th>
                <th>{t("articles.colQuantity")}</th>
                <th>{t("articles.colNote")}</th>
              </tr>
            </thead>
            <tbody>
              {movements.map((movement) => (
                <tr key={movement.id}>
                  <td>{new Date(movement.created_at).toLocaleString()}</td>
                  <td>{t(`articles.movement.${movement.movement_type}`)}</td>
                  <td style={Number(movement.quantity) < 0 ? { color: "var(--danger)" } : { color: "var(--success)" }}>
                    {Number(movement.quantity) > 0 ? "+" : ""}
                    {movement.quantity} {article.unit}
                  </td>
                  <td>{movement.note || "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
