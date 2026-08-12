import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../api";
import { useLanguage } from "../contexts/LanguageContext";
import type { FulfillSalesOrderInput, SalesOrderWithItems, ShippingCarrier } from "../types";

const emptyFulfillment: FulfillSalesOrderInput = {
  shipping_carrier: null,
  tracking_number: "",
};

export function SalesOrderDetail() {
  const { t } = useLanguage();
  const { id } = useParams();
  const [order, setOrder] = useState<SalesOrderWithItems | null>(null);
  const [fulfillment, setFulfillment] = useState<FulfillSalesOrderInput>(emptyFulfillment);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    if (!id) return;
    api.get<SalesOrderWithItems>(`/sales-orders/${id}`).then((loaded) => {
      setOrder(loaded);
      setFulfillment({
        shipping_carrier: loaded.shipping_carrier,
        tracking_number: loaded.tracking_number,
      });
    });
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  if (!order) return null;

  const fulfill = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.post(`/sales-orders/${order.id}/fulfill`, fulfillment);
      load();
    } catch {
      setError(t("orders.fulfillError"));
    } finally {
      setBusy(false);
    }
  };

  const isOpen = order.status === "open";
  const hasCarrier = fulfillment.shipping_carrier !== null;
  const fulfillmentValid = hasCarrier === Boolean(fulfillment.tracking_number.trim());

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1rem", maxWidth: 800 }}>
      <Link to="/sales-orders">← {t("orders.back")}</Link>
      <h2 style={{ margin: 0 }}>{t("orders.detailTitle")} {order.order_number}</h2>
      <div className="card" style={{ display: "grid", gridTemplateColumns: "auto 1fr", gap: "0.6rem 1rem" }}>
        <strong>{t("orders.customer")}</strong><span>{order.customer_name}</span>
        <strong>{t("orders.channel")}</strong><span>{t(`orders.${order.source}`)}</span>
        {order.external_order_id && <><strong>{t("orders.externalId")}</strong><span>{order.external_order_id}</span></>}
        <strong>{t("orders.status")}</strong><span>{t(`orders.status.${order.status}`)}</span>
        {order.fulfilled_at && <><strong>{t("orders.fulfilledAt")}</strong><span>{new Date(order.fulfilled_at).toLocaleString()}</span></>}
      </div>
      <table className="card">
        <thead><tr><th>{t("orders.item")}</th><th>{t("orders.quantity")}</th></tr></thead>
        <tbody>{order.items.map((item) => <tr key={item.id}><td>{item.description}</td><td>{item.quantity} {item.unit}</td></tr>)}</tbody>
      </table>
      {isOpen ? (
        <div className="card" style={{ display: "flex", flexDirection: "column", gap: "0.6rem" }}>
          <h3 style={{ margin: 0 }}>{t("orders.fulfill")}</h3>
          <p style={{ margin: 0, color: "var(--fg-muted)" }}>{t("orders.fulfillHint")}</p>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.6rem" }}>
            <select value={fulfillment.shipping_carrier ?? ""} onChange={(event) => setFulfillment({ ...fulfillment, shipping_carrier: (event.target.value || null) as ShippingCarrier | null })}>
              <option value="">{t("orders.noCarrier")}</option>
              {(["dhl", "hermes", "dpd"] as ShippingCarrier[]).map((carrier) => <option key={carrier} value={carrier}>{carrier.toUpperCase()}</option>)}
            </select>
            <input placeholder={t("orders.tracking")} value={fulfillment.tracking_number} onChange={(event) => setFulfillment({ ...fulfillment, tracking_number: event.target.value })} />
          </div>
          {!fulfillmentValid && <div style={{ color: "var(--danger)" }}>{t("orders.fulfillValidation")}</div>}
          {error && <div style={{ color: "var(--danger)" }}>{error}</div>}
          <button onClick={fulfill} disabled={busy || !fulfillmentValid}>{t("orders.confirmFulfill")}</button>
        </div>
      ) : (
        <div className="card"><strong>{t("orders.carrier")}:</strong> {order.shipping_carrier?.toUpperCase() ?? "—"}<br /><strong>{t("orders.tracking")}:</strong> {order.tracking_number || "—"}</div>
      )}
    </div>
  );
}
