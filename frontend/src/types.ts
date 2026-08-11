export interface CompanySettings {
  company_name: string;
  owner_name: string;
  address_line1: string;
  address_line2: string;
  zip: string;
  city: string;
  country: string;
  email: string;
  phone: string;
  tax_id: string;
  vat_id: string;
  iban: string;
  bic: string;
  bank_name: string;
  invoice_number_prefix: string;
  next_invoice_number: number;
  next_customer_number: number;
  invoice_footer_note: string;
  default_payment_terms_days: number;
  skr: string;
  datev_berater_nr: string;
  datev_mandant_nr: string;
}

export type CompanySettingsUpdate = Omit<
  CompanySettings,
  "next_invoice_number" | "next_customer_number"
>;

export interface Customer {
  id: string;
  customer_number: number;
  name: string;
  contact_person: string;
  address_line1: string;
  address_line2: string;
  zip: string;
  city: string;
  country: string;
  email: string;
  phone: string;
  ust_id: string;
  default_payment_terms_days: number | null;
  notes: string;
  active: boolean;
  created_at: string;
}

export type CustomerInput = Omit<Customer, "id" | "customer_number" | "created_at">;

export interface VatRate {
  code: string;
  rate_percent: string;
}
