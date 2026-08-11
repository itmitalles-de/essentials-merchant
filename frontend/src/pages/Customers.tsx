import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { api } from "../api";
import { useLanguage } from "../contexts/LanguageContext";
import type { Customer, CustomerInput } from "../types";

const empty: CustomerInput = {
  name: "",
  contact_person: "",
  address_line1: "",
  address_line2: "",
  zip: "",
  city: "",
  country: "DE",
  email: "",
  phone: "",
  ust_id: "",
  default_payment_terms_days: null,
  notes: "",
  active: true,
};

export function Customers() {
  const { t } = useLanguage();
  const [customers, setCustomers] = useState<Customer[]>([]);
  const [form, setForm] = useState<CustomerInput>(empty);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);

  const load = () => api.get<Customer[]>("/customers").then(setCustomers);

  useEffect(() => {
    load();
  }, []);

  const startEdit = (c: Customer) => {
    setEditingId(c.id);
    setForm({
      name: c.name,
      contact_person: c.contact_person,
      address_line1: c.address_line1,
      address_line2: c.address_line2,
      zip: c.zip,
      city: c.city,
      country: c.country,
      email: c.email,
      phone: c.phone,
      ust_id: c.ust_id,
      default_payment_terms_days: c.default_payment_terms_days,
      notes: c.notes,
      active: c.active,
    });
    setShowForm(true);
  };

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (editingId) {
      await api.put(`/customers/${editingId}`, form);
    } else {
      await api.post("/customers", form);
    }
    setForm(empty);
    setEditingId(null);
    setShowForm(false);
    load();
  };

  const remove = async (id: string) => {
    if (!confirm(t("customers.confirmDelete"))) return;
    await api.delete(`/customers/${id}`);
    load();
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h2 style={{ margin: 0 }}>{t("customers.title")}</h2>
        <button
          onClick={() => {
            setForm(empty);
            setEditingId(null);
            setShowForm((v) => !v);
          }}
        >
          {showForm ? t("customers.cancel") : t("customers.new")}
        </button>
      </div>

      {showForm && (
        <form
          onSubmit={onSubmit}
          className="card"
          style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.6rem" }}
        >
          <input
            placeholder={t("customers.name")}
            required
            value={form.name}
            onChange={(e) => setForm({ ...form, name: e.target.value })}
          />
          <input
            placeholder={t("customers.contactPerson")}
            value={form.contact_person}
            onChange={(e) => setForm({ ...form, contact_person: e.target.value })}
          />
          <input
            placeholder={t("customers.address")}
            value={form.address_line1}
            onChange={(e) => setForm({ ...form, address_line1: e.target.value })}
          />
          <input
            placeholder={t("customers.addressLine2")}
            value={form.address_line2}
            onChange={(e) => setForm({ ...form, address_line2: e.target.value })}
          />
          <input
            placeholder={t("customers.zip")}
            value={form.zip}
            onChange={(e) => setForm({ ...form, zip: e.target.value })}
          />
          <input
            placeholder={t("customers.city")}
            value={form.city}
            onChange={(e) => setForm({ ...form, city: e.target.value })}
          />
          <input
            placeholder={t("customers.email")}
            type="email"
            value={form.email}
            onChange={(e) => setForm({ ...form, email: e.target.value })}
          />
          <input
            placeholder={t("customers.phone")}
            value={form.phone}
            onChange={(e) => setForm({ ...form, phone: e.target.value })}
          />
          <input
            placeholder={t("customers.ustId")}
            value={form.ust_id}
            onChange={(e) => setForm({ ...form, ust_id: e.target.value })}
          />
          <input
            placeholder={t("customers.paymentTermsDays")}
            type="number"
            min={0}
            value={form.default_payment_terms_days ?? ""}
            onChange={(e) =>
              setForm({
                ...form,
                default_payment_terms_days: e.target.value === "" ? null : Number(e.target.value),
              })
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
          <textarea
            placeholder={t("customers.notes")}
            style={{ gridColumn: "1 / -1" }}
            value={form.notes}
            onChange={(e) => setForm({ ...form, notes: e.target.value })}
          />
          <button type="submit" style={{ gridColumn: "1 / -1" }}>
            {editingId ? t("customers.save") : t("customers.create")}
          </button>
        </form>
      )}

      <table className="card">
        <thead>
          <tr>
            <th>#</th>
            <th>{t("customers.name")}</th>
            <th>{t("customers.colContact")}</th>
            <th>{t("customers.colEmail")}</th>
            <th>{t("customers.colStatus")}</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {customers.map((c) => (
            <tr key={c.id}>
              <td>{c.customer_number}</td>
              <td>{c.name}</td>
              <td>{c.contact_person}</td>
              <td>{c.email}</td>
              <td>{c.active ? t("customers.statusActive") : t("customers.statusInactive")}</td>
              <td style={{ display: "flex", gap: "0.4rem" }}>
                <button className="secondary" onClick={() => startEdit(c)}>
                  {t("customers.edit")}
                </button>
                <button className="danger" onClick={() => remove(c.id)}>
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
