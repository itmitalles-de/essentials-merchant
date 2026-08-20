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
  document_type: "invoice" | "correction";
  corrects_invoice_id: string | null;
  corrected_invoice_number: string | null;
  correction_reason: string | null;
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
  document_type: "invoice" | "correction";
  corrects_invoice_id: string | null;
  correction_reason: string | null;
  correction_idempotency_key: string | null;
  line_items: InvoiceLineItem[];
  correction: { id: string; invoice_number: string | null } | null;
  corrected_invoice_number: string | null;
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
  reference_type: "invoice" | "sales_order" | "manual";
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
  fulfilled_at: string | null;
  created_at: string;
}

export interface SalesOrderItem {
  id: string;
  sales_order_id: string;
  position: number;
  article_id: string | null;
  description: string;
  quantity: string;
  unit: string;
}

export interface SalesOrderWithItems extends SalesOrder {
  items: SalesOrderItem[];
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

export interface FulfillSalesOrderInput {
  shipping_carrier: ShippingCarrier | null;
  tracking_number: string;
}

export interface EssentialsModule {
  module_key: string;
  module_id: string;
  module_group: string;
  display_name: string;
  module_kind: "core" | "optional" | "connector";
  version: string;
  state: "not_installed" | "needs_configuration" | "disabled" | "enabled" | "degraded";
  enabled: boolean;
  required: boolean;
  dependencies: string[];
  conflicts: string[];
  compatibility: Record<string, unknown>;
  configuration_requirements: unknown[];
  secret_requirements: unknown[];
  api_boundaries: string[];
  navigation_boundaries: string[];
  jobs: string[];
  webhooks: string[];
  healthcheck: Record<string, unknown>;
  data_ownership: string;
  backup_restore: Record<string, unknown>;
  updated_at: string;
}

export interface ConnectorHealth {
  module_key: string;
  module_id: string;
  configuration_valid: boolean;
  health_status: "not_configured" | "healthy" | "degraded" | "failed";
  checked_at: string | null;
  message: string | null;
}

export interface IntegrationQueueSummary {
  pending: number;
  processing: number;
  delivered: number;
  dead: number;
  oldest_open_at: string | null;
  last_success_at: string | null;
  last_error: string | null;
}

export interface IntegrationDiagnosticEvent {
  source: "core" | "vendure";
  event_id: string;
  event_type: string;
  status: "pending" | "processing" | "delivered" | "dead";
  attempts: number;
  available_at: string | null;
  locked_at: string | null;
  last_error: string | null;
  created_at: string;
  delivered_at: string | null;
}

export interface IntegrationDiagnostics {
  core_outbox: IntegrationQueueSummary;
  core_inbox: { completed: number; failed: number; last_processed_at: string | null };
  vendure_outbox: IntegrationQueueSummary;
  events: IntegrationDiagnosticEvent[];
  mappings: { entity_type: string; count: number; last_updated_at: string | null }[];
  audit: {
    id: string;
    actor_user_id: string;
    action: string;
    target_type: string;
    target_id: string;
    idempotency_key: string;
    details: Record<string, unknown>;
    created_at: string;
  }[];
  core_database_ready: boolean;
  vendure_health: string;
  vendure_observed_at: string | null;
}

export interface AmazonReportDefinition {
  report_type: string;
  required_roles: string[];
  regions: string[];
  format: string;
  parser_version: string | null;
  supported_options: string[];
  pii_classification: string;
  analysis_capable: boolean;
  requires_rdt: boolean;
  schedule_supported: boolean;
  deprecation_status: string;
}

export interface AmazonConnectionSummary {
  id: string;
  seller_id_redacted: string;
  region: string;
  granted_roles: string[];
  marketplace_ids: string[];
  mode: "live" | "fixture";
  enabled: boolean;
  credential_configured: boolean;
  created_at: string;
  updated_at: string;
}

export interface AmazonReportSchedule {
  id: string;
  connection_id: string;
  marketplace_id: string;
  report_type: string;
  report_options: Record<string, unknown>;
  interval_seconds: number;
  enabled: boolean;
  next_run_at: string;
  last_enqueued_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface AmazonReportRun {
  id: string;
  connection_id: string;
  schedule_id: string | null;
  marketplace_id: string;
  report_type: string;
  data_start_time: string | null;
  data_end_time: string | null;
  report_options: Record<string, unknown>;
  trigger_source: "manual" | "scheduled";
  status: string;
  attempts: number;
  poll_attempts: number;
  next_attempt_at: string;
  amazon_report_id: string | null;
  amazon_report_document_id: string | null;
  failure_code: string | null;
  failure_message: string | null;
  requested_at: string | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface MarketplaceImportMetric {
  metric_name: string;
  dimension_type: string;
  dimension_key: string;
  value_numeric: string | number;
  unit: string;
  currency_code: string | null;
}

export interface MarketplaceImportPreview {
  sha256: string;
  raw_bytes: number;
  detected_format: string;
  report_type: string;
  parser_version: string;
  marketplace_id: string;
  period_start: string;
  period_end: string;
  granularity: string;
  timezone: string;
  currency_code: string;
  data_freshness: string | null;
  confirmation_required: boolean;
  operator_confirmed: string[];
  metadata_provenance: Record<string, "report" | "operator_confirmed" | "missing">;
  missing_fields: string[];
  warnings: string[];
  metrics: MarketplaceImportMetric[];
}

export interface MarketplaceImportResult {
  outcome: "imported" | "already_imported";
  run_id: string;
  analysis_id: string | null;
  comparison_generated: boolean;
  preview: MarketplaceImportPreview;
}

export interface MarketplaceAnalysisSummary {
  id: string;
  job_id: string;
  strategy: string;
  model_name: string | null;
  prompt_version: string;
  payload_sha256: string;
  result: Record<string, unknown>;
  created_at: string;
}

export interface MarketplaceStrategyStatus {
  available: boolean;
  reason: "feature_disabled" | "api_key_missing" | null;
  provider: "openai";
  model: string;
  prompt_version: string;
  response_storage: "store_false";
  input_boundary: "aggregate_analysis_only";
  automatic_execution: false;
  mutation_capability: false;
}

export interface MarketplaceStrategyFinding {
  title: string;
  rationale: string;
  confidence: "low" | "medium" | "high";
  evidence_refs: string[];
}

export interface MarketplaceStrategyHypothesis {
  statement: string;
  rationale: string;
  confidence: "low" | "medium" | "high";
  evidence_needed: string[];
  evidence_refs: string[];
}

export interface MarketplaceStrategyAction {
  title: string;
  rationale: string;
  priority: "now" | "next" | "later";
  expected_signal: string;
  risks: string[];
  evidence_refs: string[];
}

export interface MarketplaceStrategyAssessment {
  executive_summary: string;
  assessment: string;
  opportunities: MarketplaceStrategyFinding[];
  risks: MarketplaceStrategyFinding[];
  hypotheses: MarketplaceStrategyHypothesis[];
  recommended_actions: MarketplaceStrategyAction[];
  open_questions: string[];
  limitations: string[];
}

export interface MarketplaceStrategyView {
  analysis_id: string;
  payload_sha256: string;
  status: MarketplaceStrategyStatus;
  cached: boolean;
  assessment: MarketplaceStrategyAssessment | null;
  provider_request_id_redacted: string | null;
  input_tokens: number | null;
  output_tokens: number | null;
  created_at: string | null;
}

export interface MarketplaceOverview {
  connections: AmazonConnectionSummary[];
  schedules: AmazonReportSchedule[];
  recent_runs: AmazonReportRun[];
  analyses: MarketplaceAnalysisSummary[];
  report_types: AmazonReportDefinition[];
}

export interface MarketplaceRunDetail {
  run: AmazonReportRun;
  events: Array<{ id: number; status: string; message: string | null; created_at: string }>;
  document: { sha256: string; decoded_sha256: string; import_status: string; import_error: string | null; parser_version: string | null; downloaded_at: string; transport_bytes: number; decoded_bytes: number } | null;
  snapshot: { id: string; period_start: string | null; period_end: string | null; granularity: string; comparability_key: string; summary: Record<string, unknown> } | null;
  metrics: Array<{ id: number; metric_name: string; dimension_type: string; dimension_key: string; value_numeric: string; unit: string; currency_code: string | null }>;
  analyses: Array<{ id: string; result: Record<string, unknown>; created_at: string }>;
  transport: Array<{ id: number; operation: string; request_id_redacted: string | null; rate_limit_limit: string | null; retry_after_seconds: number | null; observed_at: string }>;
}

export interface AmazonPilotStatus {
  profile: "amazon-read-only";
  title: string;
  enabled: boolean;
  compliant: boolean;
  active_modules: string[];
  disabled_modules: string[];
  mutation_modules: string[];
  unexpected_active_modules: string[];
  missing_required_modules: string[];
  automatic_schedules_enabled: number;
  last_backup_verification: {
    outcome: "passed" | "failed";
    manifest_sha256: string;
    repository_revision: string;
    details: Record<string, unknown>;
    verified_at: string;
  } | null;
}
