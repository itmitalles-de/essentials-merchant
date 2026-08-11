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

export type InvoiceStatus = "draft" | "sent" | "overdue" | "paid" | "cancelled";

export interface InvoiceListItem {
  id: string;
  invoice_number: string | null;
  customer_id: string;
  customer_name: string;
  status: InvoiceStatus;
  issue_date: string | null;
  due_date: string | null;
  gross_total: string;
  created_at: string;
}

export interface CustomerSnapshot {
  name: string;
  contact_person: string;
  address_line1: string;
  address_line2: string;
  zip: string;
  city: string;
  country: string;
  ust_id: string;
}

export interface CompanySnapshot {
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
  invoice_footer_note: string;
}

export interface InvoiceLineItem {
  id: string;
  invoice_id: string;
  position: number;
  description: string;
  article_id: string | null;
  quantity: string;
  unit: string;
  unit_price_net: string;
  vat_rate_code: string;
  vat_rate_percent: string;
  net_amount: string;
  vat_amount: string;
  gross_amount: string;
}

export interface Invoice {
  id: string;
  invoice_number: string | null;
  customer_id: string;
  status: InvoiceStatus;
  issue_date: string | null;
  due_date: string | null;
  customer_snapshot: CustomerSnapshot | null;
  company_snapshot: CompanySnapshot | null;
  net_total: string;
  vat_total: string;
  gross_total: string;
  notes: string;
  pdf_path: string | null;
  sent_at: string | null;
  paid_at: string | null;
  cancelled_at: string | null;
  created_at: string;
  line_items: InvoiceLineItem[];
}

export interface InvoiceInput {
  customer_id: string;
  notes: string;
}

export interface LineItemInput {
  description: string;
  article_id: string | null;
  quantity: string;
  unit: string;
  unit_price_net: string;
  vat_rate_code: string;
}

export interface Article {
  id: string;
  sku: string;
  name: string;
  unit: string;
  sales_price_net: string;
  default_vat_rate_code: string;
  purchase_price_net: string | null;
  stock_quantity: string;
  min_stock_quantity: string | null;
  active: boolean;
  created_at: string;
}

export type ArticleInput = Omit<Article, "id" | "stock_quantity" | "created_at">;

export type StockMovementType = "in" | "out" | "adjustment";

export interface StockMovement {
  id: string;
  article_id: string;
  movement_type: StockMovementType;
  quantity: string;
  reference_type: "invoice" | "manual";
  reference_id: string | null;
  note: string;
  created_at: string;
}

export interface ManualAdjustmentInput {
  movement_type: StockMovementType;
  quantity: string;
  note: string;
}

export type SalesChannel = "manual" | "woocommerce" | "amazon" | "ebay";
export type ShippingCarrier = "dhl" | "hermes" | "dpd";

export interface SalesOrder {
  id: string;
  order_number: number;
  customer_id: string;
  customer_name: string;
  source: SalesChannel;
  external_order_id: string | null;
  status: "open" | "fulfilled" | "cancelled";
  shipping_carrier: ShippingCarrier | null;
  tracking_number: string;
  notes: string;
  created_at: string;
}

export interface SalesOrderItemInput {
  article_id: string | null;
  description: string;
  quantity: string;
  unit: string;
}

export interface CreateSalesOrderInput {
  customer_id: string;
  source: SalesChannel;
  external_order_id: string | null;
  shipping_carrier: ShippingCarrier | null;
  tracking_number: string;
  notes: string;
  items: SalesOrderItemInput[];
}
