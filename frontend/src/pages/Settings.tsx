import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { api } from "../api";
import { useLanguage } from "../contexts/LanguageContext";
import type { CompanySettings, CompanySettingsUpdate } from "../types";

export function Settings() {
  const { t } = useLanguage();
  const [form, setForm] = useState<CompanySettingsUpdate | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    api.get<CompanySettings>("/company-settings").then(setForm);
  }, []);

  if (!form) return null;

  const field = (key: keyof CompanySettingsUpdate) => ({
    value: form[key],
    onChange: (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      setSaved(false);
      setForm({
        ...form,
        [key]: e.target.type === "number" ? Number(e.target.value) : e.target.value,
      });
    },
  });

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setSaving(true);
    try {
      await api.put<CompanySettings>("/company-settings", form);
      setSaved(true);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="card" style={{ maxWidth: 640 }}>
      <h1>{t("settings.title")}</h1>
      <form onSubmit={onSubmit} style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
        <label>
          {t("settings.companyName")}
          <input {...field("company_name")} />
        </label>
        <label>
          {t("settings.ownerName")}
          <input {...field("owner_name")} />
        </label>
        <label>
          {t("settings.addressLine1")}
          <input {...field("address_line1")} />
        </label>
        <label>
          {t("settings.addressLine2")}
          <input {...field("address_line2")} />
        </label>
        <div style={{ display: "flex", gap: "0.5rem" }}>
          <label style={{ flex: "0 0 120px" }}>
            {t("settings.zip")}
            <input {...field("zip")} />
          </label>
          <label style={{ flex: 1 }}>
            {t("settings.city")}
            <input {...field("city")} />
          </label>
        </div>
        <label>
          {t("settings.email")}
          <input type="email" {...field("email")} />
        </label>
        <label>
          {t("settings.phone")}
          <input {...field("phone")} />
        </label>
        <label>
          {t("settings.taxId")}
          <input {...field("tax_id")} />
        </label>
        <label>
          {t("settings.vatId")}
          <input {...field("vat_id")} />
        </label>
        <label>
          {t("settings.iban")}
          <input {...field("iban")} />
        </label>
        <label>
          {t("settings.bic")}
          <input {...field("bic")} />
        </label>
        <label>
          {t("settings.bankName")}
          <input {...field("bank_name")} />
        </label>
        <label>
          {t("settings.invoiceNumberPrefix")}
          <input {...field("invoice_number_prefix")} />
        </label>
        <label>
          {t("settings.invoiceFooterNote")}
          <textarea rows={2} {...field("invoice_footer_note")} />
        </label>
        <label>
          {t("settings.paymentTermsDays")}
          <input type="number" min={0} {...field("default_payment_terms_days")} />
        </label>
        <label>
          {t("settings.skr")}
          <input {...field("skr")} />
        </label>
        <label>
          {t("settings.datevBeraterNr")}
          <input {...field("datev_berater_nr")} />
        </label>
        <label>
          {t("settings.datevMandantNr")}
          <input {...field("datev_mandant_nr")} />
        </label>
        <div style={{ display: "flex", alignItems: "center", gap: "0.75rem" }}>
          <button type="submit" disabled={saving}>
            {t("settings.save")}
          </button>
          {saved && <span style={{ color: "var(--success)" }}>{t("settings.saved")}</span>}
        </div>
      </form>
    </div>
  );
}
