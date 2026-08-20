//! Persistent Marketplace Intelligence records. Secrets deliberately do not
//! belong in this module or its database schema; `secret_ref` is only a logical
//! lookup key for the process environment.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub const SALES_AND_TRAFFIC: &str = "GET_SALES_AND_TRAFFIC_REPORT";
pub const INVENTORY_PLANNING: &str = "GET_FBA_INVENTORY_PLANNING_DATA";
pub const FBA_RETURNS: &str = "GET_FBA_FULFILLMENT_CUSTOMER_RETURNS_DATA";
pub const SETTLEMENT_V2: &str = "GET_V2_SETTLEMENT_REPORT_DATA_FLAT_FILE_V2";

#[derive(Debug, Clone, Serialize)]
pub struct ReportDefinition {
    pub report_type: &'static str,
    pub required_roles: &'static [&'static str],
    pub regions: &'static [&'static str],
    pub format: &'static str,
    pub parser_version: Option<&'static str>,
    pub supported_options: &'static [&'static str],
    pub pii_classification: &'static str,
    pub analysis_capable: bool,
    pub requires_rdt: bool,
    pub schedule_supported: bool,
    pub deprecation_status: &'static str,
}

const REPORT_DEFINITIONS: [ReportDefinition; 4] = [
    ReportDefinition {
        report_type: SALES_AND_TRAFFIC,
        required_roles: &["Brand Analytics"],
        regions: &["na", "eu", "fe"],
        format: "json",
        parser_version: Some("sales-traffic-json-v2"),
        supported_options: &["dateGranularity", "asinGranularity"],
        pii_classification: "aggregated",
        analysis_capable: true,
        requires_rdt: false,
        schedule_supported: true,
        deprecation_status: "active",
    },
    ReportDefinition {
        report_type: INVENTORY_PLANNING,
        required_roles: &["Amazon Fulfillment"],
        regions: &["na", "eu", "fe"],
        format: "tsv",
        parser_version: Some("inventory-planning-tsv-v1"),
        supported_options: &[],
        pii_classification: "aggregated",
        analysis_capable: true,
        requires_rdt: false,
        schedule_supported: false,
        deprecation_status: "active",
    },
    ReportDefinition {
        report_type: FBA_RETURNS,
        required_roles: &["Pricing", "Amazon Fulfillment"],
        regions: &["na", "eu", "fe"],
        format: "tsv",
        parser_version: None,
        supported_options: &[],
        pii_classification: "potential_pii",
        analysis_capable: false,
        requires_rdt: false,
        schedule_supported: false,
        deprecation_status: "active_raw_only",
    },
    ReportDefinition {
        report_type: SETTLEMENT_V2,
        required_roles: &["Finance and Accounting"],
        regions: &["na", "eu", "fe"],
        format: "tsv",
        parser_version: None,
        supported_options: &[],
        pii_classification: "financial_pseudonymous",
        analysis_capable: false,
        requires_rdt: false,
        schedule_supported: false,
        deprecation_status: "active_raw_only",
    },
];

pub fn report_definitions() -> &'static [ReportDefinition] {
    &REPORT_DEFINITIONS
}

pub fn report_definition(report_type: &str) -> Option<&'static ReportDefinition> {
    REPORT_DEFINITIONS
        .iter()
        .find(|definition| definition.report_type == report_type)
}

#[derive(Debug, Clone, Deserialize)]
pub struct AmazonConnectionInput {
    pub seller_id: String,
    pub region: String,
    pub secret_ref: String,
    #[serde(default)]
    pub granted_roles: Vec<String>,
    #[serde(default)]
    pub marketplace_ids: Vec<String>,
    #[serde(default = "default_connection_mode")]
    pub mode: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_connection_mode() -> String {
    "live".to_owned()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AmazonConnection {
    pub id: Uuid,
    pub seller_id: String,
    pub region: String,
    pub secret_ref: String,
    pub granted_roles: Vec<String>,
    pub mode: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmazonConnectionSummary {
    pub id: Uuid,
    pub seller_id_redacted: String,
    pub region: String,
    pub granted_roles: Vec<String>,
    pub marketplace_ids: Vec<String>,
    pub mode: String,
    pub enabled: bool,
    pub credential_configured: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AmazonConnectionSummary {
    fn from_connection(connection: AmazonConnection, marketplace_ids: Vec<String>) -> Self {
        let credential_configured = connection.mode == "fixture"
            || live_secret_reference_is_configured(&connection.secret_ref);
        Self {
            id: connection.id,
            seller_id_redacted: redact_identifier(&connection.seller_id),
            region: connection.region,
            granted_roles: connection.granted_roles,
            marketplace_ids,
            mode: connection.mode,
            enabled: connection.enabled,
            credential_configured,
            created_at: connection.created_at,
            updated_at: connection.updated_at,
        }
    }
}

pub fn live_secret_reference_is_configured(secret_ref: &str) -> bool {
    let Ok(raw) = std::env::var(secret_environment_key(secret_ref)) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return false;
    };
    ["refresh_token", "client_id", "client_secret"]
        .iter()
        .all(|field| {
            value
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        })
}

fn secret_environment_key(secret_ref: &str) -> String {
    let normalized = secret_ref
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("AMAZON_SECRET_{normalized}")
}

fn redact_identifier(identifier: &str) -> String {
    let tail = identifier.chars().rev().take(4).collect::<Vec<_>>();
    let tail = tail.into_iter().rev().collect::<String>();
    if identifier.chars().count() <= 4 {
        "****".to_owned()
    } else {
        format!("****{tail}")
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AmazonReportSchedule {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub marketplace_id: String,
    pub report_type: String,
    pub report_options: Value,
    pub interval_seconds: i32,
    pub enabled: bool,
    pub next_run_at: DateTime<Utc>,
    pub last_enqueued_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AmazonReportScheduleInput {
    pub marketplace_id: String,
    pub report_type: String,
    #[serde(default)]
    pub report_options: Value,
    pub interval_seconds: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AmazonReportRun {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub schedule_id: Option<Uuid>,
    pub marketplace_id: String,
    pub report_type: String,
    pub data_start_time: Option<DateTime<Utc>>,
    pub data_end_time: Option<DateTime<Utc>>,
    pub report_options: Value,
    pub trigger_source: String,
    pub status: String,
    pub attempts: i32,
    pub poll_attempts: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub amazon_report_id: Option<String>,
    pub amazon_report_document_id: Option<String>,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
    pub requested_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAmazonReportRunInput {
    pub marketplace_id: String,
    pub report_type: String,
    pub data_start_time: Option<DateTime<Utc>>,
    pub data_end_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub report_options: Value,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AmazonRunEvent {
    pub id: i64,
    pub run_id: Uuid,
    pub status: String,
    pub message: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MetricSnapshot {
    pub id: Uuid,
    pub run_id: Uuid,
    pub connection_id: Uuid,
    pub marketplace_id: String,
    pub report_type: String,
    pub parser_version: String,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
    pub granularity: String,
    pub comparability_key: String,
    pub summary: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct NormalizedMetric {
    pub id: i64,
    pub snapshot_id: Uuid,
    pub metric_name: String,
    pub dimension_type: String,
    pub dimension_key: String,
    pub value_numeric: Decimal,
    pub unit: String,
    pub currency_code: Option<String>,
    pub evidence: Value,
}

#[derive(Debug, Clone)]
pub struct ParsedMetric {
    pub metric_name: String,
    pub dimension_type: String,
    pub dimension_key: String,
    pub value_numeric: Decimal,
    pub unit: String,
    pub currency_code: Option<String>,
    pub evidence: Value,
}

#[derive(Debug, Clone)]
pub struct ParsedSnapshot {
    pub parser_version: String,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
    pub granularity: String,
    pub comparability_key: String,
    pub summary: Value,
    pub metrics: Vec<ParsedMetric>,
}

#[derive(Debug, Clone)]
pub struct ManualImportStoreInput<'a> {
    pub uploaded_by: Uuid,
    pub raw_sha256: &'a str,
    pub raw_content: &'a [u8],
    pub content_type: &'a str,
    pub detected_format: &'a str,
    pub marketplace_id: &'a str,
    pub report_type: &'a str,
    pub date_granularity: &'a str,
    pub source_timezone: &'a str,
    pub currency_code: Option<&'a str>,
    pub parsed: &'a ParsedSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManualImportStoreOutcome {
    pub run_id: Uuid,
    pub analysis_job_id: Uuid,
    pub comparison_generated: bool,
    pub imported: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ManualImportStoreError {
    #[error("manual import input is invalid: {0}")]
    InvalidInput(String),
    #[error("the same raw bytes were already archived with different confirmed metadata")]
    MetadataConflict,
    #[error("a different raw report already represents this exact comparison period")]
    DuplicatePeriod,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AnalysisJob {
    pub id: Uuid,
    pub run_id: Option<Uuid>,
    pub connection_id: Uuid,
    pub marketplace_id: String,
    pub report_type: Option<String>,
    pub analysis_type: String,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
    pub status: String,
    pub attempts: i32,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AnalysisResult {
    pub id: Uuid,
    pub job_id: Uuid,
    pub strategy: String,
    pub model_name: Option<String>,
    pub prompt_version: String,
    pub payload_sha256: String,
    pub result: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AiStrategyAssessment {
    pub id: Uuid,
    pub analysis_id: Uuid,
    pub payload_sha256: String,
    pub model_name: String,
    pub prompt_version: String,
    pub result: Value,
    pub provider_request_id_redacted: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

pub struct StoreAiStrategyAssessment<'a> {
    pub analysis_id: Uuid,
    pub payload_sha256: &'a str,
    pub model_name: &'a str,
    pub prompt_version: &'a str,
    pub result: &'a Value,
    pub provider_request_id_redacted: Option<&'a str>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub created_by: Uuid,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AmazonReportDocumentInfo {
    pub id: Uuid,
    pub run_id: Uuid,
    pub amazon_report_document_id: String,
    pub sha256: String,
    pub decoded_sha256: String,
    pub content_type: Option<String>,
    pub compression_algorithm: Option<String>,
    pub downloaded_at: DateTime<Utc>,
    pub parser_version: Option<String>,
    pub import_status: String,
    pub import_error: Option<String>,
    pub transport_bytes: i64,
    pub decoded_bytes: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AmazonTransportObservation {
    pub id: i64,
    pub run_id: Uuid,
    pub operation: String,
    pub request_id_redacted: Option<String>,
    pub rate_limit_limit: Option<String>,
    pub retry_after_seconds: Option<i64>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RawReportDocument {
    pub content_type: Option<String>,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmazonRunDetail {
    pub run: AmazonReportRun,
    pub events: Vec<AmazonRunEvent>,
    pub document: Option<AmazonReportDocumentInfo>,
    pub snapshot: Option<MetricSnapshot>,
    pub metrics: Vec<NormalizedMetric>,
    pub analyses: Vec<AnalysisResult>,
    pub transport: Vec<AmazonTransportObservation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketplaceOverview {
    pub connections: Vec<AmazonConnectionSummary>,
    pub schedules: Vec<AmazonReportSchedule>,
    pub recent_runs: Vec<AmazonReportRun>,
    pub analyses: Vec<AnalysisResult>,
    pub report_types: Vec<ReportDefinition>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClaimedReportRun {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub schedule_id: Option<Uuid>,
    pub marketplace_id: String,
    pub report_type: String,
    pub data_start_time: Option<DateTime<Utc>>,
    pub data_end_time: Option<DateTime<Utc>>,
    pub report_options: Value,
    pub trigger_source: String,
    pub status: String,
    pub attempts: i32,
    pub poll_attempts: i32,
    pub amazon_report_id: Option<String>,
    pub amazon_report_document_id: Option<String>,
    pub seller_id: String,
    pub region: String,
    pub secret_ref: String,
    pub granted_roles: Vec<String>,
    pub mode: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClaimedAnalysisJob {
    pub id: Uuid,
    pub run_id: Option<Uuid>,
    pub connection_id: Uuid,
    pub marketplace_id: String,
    pub report_type: Option<String>,
    pub analysis_type: String,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
    pub attempts: i32,
}

pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let ordered: BTreeMap<_, _> = map.iter().collect();
            let values = ordered
                .into_iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap(),
                        canonical_json(value)
                    )
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", values.join(","))
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => value.to_string(),
    }
}

pub fn sha256(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn validate_connection_input(input: &AmazonConnectionInput) -> Result<(), String> {
    if input.seller_id.trim().is_empty()
        || input.secret_ref.trim().is_empty()
        || input.marketplace_ids.is_empty()
        || !matches!(input.region.as_str(), "na" | "eu" | "fe")
        || !matches!(input.mode.as_str(), "live" | "fixture")
    {
        return Err(
            "seller, region, secret reference, mode, and a marketplace are required".into(),
        );
    }
    if !transport_mode_is_consistent(&input.mode, &input.secret_ref) {
        return Err("fixture connections require a fixture: secret reference, and live connections must not use one".into());
    }
    if input.marketplace_ids.iter().any(|id| id.trim().is_empty()) {
        return Err("marketplace identifiers cannot be empty".into());
    }
    Ok(())
}

/// Keep the persisted connection mode and transport selector inseparable. The
/// worker repeats this check so legacy or directly inserted rows fail closed.
pub fn transport_mode_is_consistent(mode: &str, secret_ref: &str) -> bool {
    matches!(
        (mode, secret_ref.starts_with("fixture:")),
        ("fixture", true) | ("live", false)
    )
}

pub async fn upsert_connection(
    pool: &PgPool,
    input: &AmazonConnectionInput,
) -> Result<AmazonConnectionSummary, sqlx::Error> {
    validate_connection_input(input).map_err(sqlx::Error::Protocol)?;
    let mut tx = pool.begin().await?;
    let connection = sqlx::query_as::<_, AmazonConnection>(
        "INSERT INTO amazon_connections
             (seller_id, region, secret_ref, granted_roles, mode, enabled)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (seller_id, region, secret_ref) DO UPDATE
         SET granted_roles = EXCLUDED.granted_roles, mode = EXCLUDED.mode,
             enabled = EXCLUDED.enabled, updated_at = now()
         RETURNING id, seller_id, region, secret_ref, granted_roles, mode, enabled, created_at, updated_at",
    )
    .bind(&input.seller_id)
    .bind(&input.region)
    .bind(&input.secret_ref)
    .bind(&input.granted_roles)
    .bind(&input.mode)
    .bind(input.enabled)
    .fetch_one(&mut *tx)
    .await?;
    for marketplace_id in &input.marketplace_ids {
        sqlx::query(
            "INSERT INTO amazon_marketplaces (connection_id, marketplace_id)
             VALUES ($1, $2) ON CONFLICT (connection_id, marketplace_id) DO NOTHING",
        )
        .bind(connection.id)
        .bind(marketplace_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let marketplace_ids = list_marketplace_ids(pool, connection.id).await?;
    Ok(AmazonConnectionSummary::from_connection(
        connection,
        marketplace_ids,
    ))
}

pub async fn create_demo_connection(pool: &PgPool) -> Result<AmazonConnectionSummary, sqlx::Error> {
    upsert_connection(
        pool,
        &AmazonConnectionInput {
            seller_id: "DEMO-SELLER".to_owned(),
            region: "eu".to_owned(),
            secret_ref: "fixture:demo".to_owned(),
            granted_roles: vec![
                "Brand Analytics".to_owned(),
                "Amazon Fulfillment".to_owned(),
            ],
            marketplace_ids: vec!["A1PA6795UKMFR9".to_owned()],
            mode: "fixture".to_owned(),
            enabled: true,
        },
    )
    .await
}

pub async fn get_connection(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<AmazonConnection>, sqlx::Error> {
    sqlx::query_as::<_, AmazonConnection>(
        "SELECT id, seller_id, region, secret_ref, granted_roles, mode, enabled, created_at, updated_at
         FROM amazon_connections WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

async fn list_marketplace_ids(
    pool: &PgPool,
    connection_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT marketplace_id FROM amazon_marketplaces
         WHERE connection_id = $1 AND enabled = true ORDER BY marketplace_id",
    )
    .bind(connection_id)
    .fetch_all(pool)
    .await
}

pub async fn marketplace_exists(
    pool: &PgPool,
    connection_id: Uuid,
    marketplace_id: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM amazon_marketplaces
             WHERE connection_id = $1 AND marketplace_id = $2 AND enabled
         )",
    )
    .bind(connection_id)
    .bind(marketplace_id)
    .fetch_one(pool)
    .await
}

pub async fn upsert_schedule(
    pool: &PgPool,
    connection_id: Uuid,
    input: &AmazonReportScheduleInput,
) -> Result<AmazonReportSchedule, sqlx::Error> {
    sqlx::query_as::<_, AmazonReportSchedule>(
        "INSERT INTO amazon_report_schedules
             (connection_id, marketplace_id, report_type, report_options, interval_seconds, enabled)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (connection_id, marketplace_id, report_type) DO UPDATE
         SET report_options = EXCLUDED.report_options, interval_seconds = EXCLUDED.interval_seconds,
             enabled = EXCLUDED.enabled, next_run_at = CASE
                 WHEN EXCLUDED.enabled AND NOT amazon_report_schedules.enabled THEN now()
                 ELSE amazon_report_schedules.next_run_at END,
             updated_at = now()
         RETURNING id, connection_id, marketplace_id, report_type, report_options, interval_seconds,
                   enabled, next_run_at, last_enqueued_at, created_at, updated_at",
    )
    .bind(connection_id)
    .bind(&input.marketplace_id)
    .bind(&input.report_type)
    .bind(&input.report_options)
    .bind(input.interval_seconds)
    .bind(input.enabled)
    .fetch_one(pool)
    .await
}

fn idempotency_key(
    connection_id: Uuid,
    marketplace_id: &str,
    report_type: &str,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    options: &Value,
    trigger_discriminator: &str,
) -> String {
    let input = format!(
        "{connection_id}|{marketplace_id}|{report_type}|{}|{}|{}|{trigger_discriminator}",
        start.map(|value| value.to_rfc3339()).unwrap_or_default(),
        end.map(|value| value.to_rfc3339()).unwrap_or_default(),
        canonical_json(options),
    );
    format!("amazon-report:{}", sha256(input.as_bytes()))
}

#[allow(clippy::too_many_arguments)]
async fn insert_run(
    tx: &mut Transaction<'_, Postgres>,
    connection_id: Uuid,
    schedule_id: Option<Uuid>,
    marketplace_id: &str,
    report_type: &str,
    data_start_time: Option<DateTime<Utc>>,
    data_end_time: Option<DateTime<Utc>>,
    report_options: &Value,
    trigger_source: &str,
    trigger_discriminator: &str,
) -> Result<AmazonReportRun, sqlx::Error> {
    let key = idempotency_key(
        connection_id,
        marketplace_id,
        report_type,
        data_start_time,
        data_end_time,
        report_options,
        trigger_discriminator,
    );
    let inserted = sqlx::query_as::<_, AmazonReportRun>(
        "INSERT INTO amazon_report_runs
             (connection_id, schedule_id, marketplace_id, report_type, data_start_time, data_end_time,
              report_options, trigger_source, idempotency_key, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'queued')
         ON CONFLICT (idempotency_key) DO NOTHING
         RETURNING id, connection_id, schedule_id, marketplace_id, report_type, data_start_time,
                   data_end_time, report_options, trigger_source, status, attempts, poll_attempts,
                   next_attempt_at, amazon_report_id, amazon_report_document_id, failure_code,
                   failure_message, requested_at, completed_at, created_at, updated_at",
    )
    .bind(connection_id)
    .bind(schedule_id)
    .bind(marketplace_id)
    .bind(report_type)
    .bind(data_start_time)
    .bind(data_end_time)
    .bind(report_options)
    .bind(trigger_source)
    .bind(&key)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(run) = inserted {
        sqlx::query(
            "INSERT INTO amazon_report_run_events (run_id, status, message)
             VALUES ($1, 'queued', $2)",
        )
        .bind(run.id)
        .bind(format!("{trigger_source} request queued"))
        .execute(&mut **tx)
        .await?;
        Ok(run)
    } else {
        sqlx::query_as::<_, AmazonReportRun>(
            "SELECT id, connection_id, schedule_id, marketplace_id, report_type, data_start_time,
                    data_end_time, report_options, trigger_source, status, attempts, poll_attempts,
                    next_attempt_at, amazon_report_id, amazon_report_document_id, failure_code,
                    failure_message, requested_at, completed_at, created_at, updated_at
             FROM amazon_report_runs WHERE idempotency_key = $1",
        )
        .bind(key)
        .fetch_one(&mut **tx)
        .await
    }
}

pub async fn create_manual_run(
    pool: &PgPool,
    connection_id: Uuid,
    input: &CreateAmazonReportRunInput,
) -> Result<AmazonReportRun, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // A double click joins a queued/in-flight equivalent job. Once terminal, a
    // deliberate new request may create another historical run for comparison.
    if let Some(existing) = sqlx::query_as::<_, AmazonReportRun>(
        "SELECT id, connection_id, schedule_id, marketplace_id, report_type, data_start_time,
                data_end_time, report_options, trigger_source, status, attempts, poll_attempts,
                next_attempt_at, amazon_report_id, amazon_report_document_id, failure_code,
                failure_message, requested_at, completed_at, created_at, updated_at
         FROM amazon_report_runs
         WHERE connection_id = $1 AND marketplace_id = $2 AND report_type = $3
           AND data_start_time IS NOT DISTINCT FROM $4
           AND data_end_time IS NOT DISTINCT FROM $5
           AND report_options = $6
           AND trigger_source = 'manual'
           AND status IN ('queued', 'requesting', 'polling', 'downloading', 'parsing', 'analysing')
         ORDER BY created_at DESC LIMIT 1 FOR UPDATE",
    )
    .bind(connection_id)
    .bind(&input.marketplace_id)
    .bind(&input.report_type)
    .bind(input.data_start_time)
    .bind(input.data_end_time)
    .bind(&input.report_options)
    .fetch_optional(&mut *tx)
    .await?
    {
        tx.commit().await?;
        return Ok(existing);
    }
    let run = insert_run(
        &mut tx,
        connection_id,
        None,
        &input.marketplace_id,
        &input.report_type,
        input.data_start_time,
        input.data_end_time,
        &input.report_options,
        "manual",
        &format!("manual:{}", Uuid::new_v4()),
    )
    .await?;
    tx.commit().await?;
    Ok(run)
}

pub async fn enqueue_due_schedules(pool: &PgPool, limit: i64) -> Result<usize, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let schedules = sqlx::query_as::<_, AmazonReportSchedule>(
        "SELECT id, connection_id, marketplace_id, report_type, report_options, interval_seconds,
                enabled, next_run_at, last_enqueued_at, created_at, updated_at
         FROM amazon_report_schedules
         WHERE enabled AND next_run_at <= now()
         ORDER BY next_run_at FOR UPDATE SKIP LOCKED LIMIT $1",
    )
    .bind(limit.clamp(1, 100))
    .fetch_all(&mut *tx)
    .await?;
    let mut count = 0;
    let now = Utc::now();
    for schedule in schedules {
        let period_end = now;
        let period_start = now - Duration::seconds(i64::from(schedule.interval_seconds));
        let discriminator = format!(
            "schedule:{}:{}",
            schedule.id,
            schedule.next_run_at.to_rfc3339()
        );
        insert_run(
            &mut tx,
            schedule.connection_id,
            Some(schedule.id),
            &schedule.marketplace_id,
            &schedule.report_type,
            Some(period_start),
            Some(period_end),
            &schedule.report_options,
            "scheduled",
            &discriminator,
        )
        .await?;
        sqlx::query(
            "UPDATE amazon_report_schedules
             SET last_enqueued_at = now(), next_run_at = now() + make_interval(secs => $2), updated_at = now()
             WHERE id = $1",
        )
        .bind(schedule.id)
        .bind(f64::from(schedule.interval_seconds))
        .execute(&mut *tx)
        .await?;
        count += 1;
    }
    tx.commit().await?;
    Ok(count)
}

pub async fn claim_due_runs(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<ClaimedReportRun>, sqlx::Error> {
    sqlx::query(
        "UPDATE amazon_report_runs
         SET locked_at = NULL, next_attempt_at = now(), updated_at = now()
         WHERE locked_at < now() - interval '5 minutes'
           AND status IN ('queued', 'requesting', 'polling', 'downloading', 'parsing', 'analysing')",
    )
    .execute(pool)
    .await?;
    sqlx::query_as::<_, ClaimedReportRun>(
        "WITH selected AS (
             SELECT id FROM amazon_report_runs
             WHERE status IN ('queued', 'requesting', 'polling', 'downloading', 'parsing', 'analysing')
               AND locked_at IS NULL AND next_attempt_at <= now()
             ORDER BY next_attempt_at, created_at
             FOR UPDATE SKIP LOCKED LIMIT $1
         ), claimed AS (
             UPDATE amazon_report_runs run
             SET locked_at = now(), attempts = run.attempts + 1, updated_at = now()
             FROM selected WHERE run.id = selected.id
             RETURNING run.*
         )
         SELECT claimed.id, claimed.connection_id, claimed.schedule_id, claimed.marketplace_id,
                claimed.report_type, claimed.data_start_time, claimed.data_end_time,
                claimed.report_options, claimed.trigger_source, claimed.status, claimed.attempts,
                claimed.poll_attempts, claimed.amazon_report_id, claimed.amazon_report_document_id,
                connection.seller_id, connection.region, connection.secret_ref, connection.granted_roles,
                connection.mode
         FROM claimed JOIN amazon_connections connection ON connection.id = claimed.connection_id",
    )
    .bind(limit.clamp(1, 25))
    .fetch_all(pool)
    .await
}

async fn append_run_event(
    tx: &mut Transaction<'_, Postgres>,
    run_id: Uuid,
    status: &str,
    message: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO amazon_report_run_events (run_id, status, message) VALUES ($1, $2, $3)",
    )
    .bind(run_id)
    .bind(status)
    .bind(message)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn set_run_request_created(
    pool: &PgPool,
    run_id: Uuid,
    amazon_report_id: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE amazon_report_runs
         SET status = 'polling', amazon_report_id = $2, requested_at = COALESCE(requested_at, now()),
             next_attempt_at = now(), locked_at = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(run_id)
    .bind(amazon_report_id)
    .execute(&mut *tx)
    .await?;
    append_run_event(
        &mut tx,
        run_id,
        "polling",
        Some("Amazon report request accepted"),
    )
    .await?;
    tx.commit().await
}

pub async fn mark_run_requesting(pool: &PgPool, run_id: Uuid) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE amazon_report_runs
         SET status = 'requesting', requested_at = COALESCE(requested_at, now()), updated_at = now()
         WHERE id = $1",
    )
    .bind(run_id)
    .execute(&mut *tx)
    .await?;
    append_run_event(
        &mut tx,
        run_id,
        "requesting",
        Some("Submitting createReport request"),
    )
    .await?;
    tx.commit().await
}

pub async fn set_run_poll_pending(
    pool: &PgPool,
    run_id: Uuid,
    delay_seconds: i64,
    message: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE amazon_report_runs
         SET status = 'polling', poll_attempts = poll_attempts + 1,
             next_attempt_at = now() + make_interval(secs => $2), locked_at = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(run_id)
    .bind(delay_seconds as f64)
    .execute(&mut *tx)
    .await?;
    append_run_event(&mut tx, run_id, "polling", Some(message)).await?;
    tx.commit().await
}

pub async fn set_run_document_ready(
    pool: &PgPool,
    run_id: Uuid,
    document_id: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE amazon_report_runs
         SET status = 'downloading', amazon_report_document_id = $2,
             next_attempt_at = now(), locked_at = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(run_id)
    .bind(document_id)
    .execute(&mut *tx)
    .await?;
    append_run_event(
        &mut tx,
        run_id,
        "downloading",
        Some("Amazon report document is ready"),
    )
    .await?;
    tx.commit().await
}

pub async fn archive_document(
    pool: &PgPool,
    run_id: Uuid,
    document_id: &str,
    content_type: Option<&str>,
    compression_algorithm: Option<&str>,
    raw_content: &[u8],
    decoded_content: &[u8],
) -> Result<(), sqlx::Error> {
    let checksum = sha256(raw_content);
    let decoded_checksum = sha256(decoded_content);
    let mut tx = pool.begin().await?;
    let existing = sqlx::query_scalar::<_, String>(
        "SELECT sha256 FROM amazon_report_documents WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_optional(&mut *tx)
    .await?;
    match existing {
        Some(existing_checksum) if existing_checksum != checksum => {
            return Err(sqlx::Error::Protocol(
                "downloaded report content differs from immutable archive".into(),
            ));
        }
        Some(_) => {}
        None => {
            sqlx::query(
                "INSERT INTO amazon_report_documents
                     (run_id, amazon_report_document_id, sha256, content_type,
                      compression_algorithm, raw_content, decoded_content, decoded_sha256)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(run_id)
            .bind(document_id)
            .bind(&checksum)
            .bind(content_type)
            .bind(compression_algorithm)
            .bind(raw_content)
            .bind(decoded_content)
            .bind(&decoded_checksum)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query(
        "UPDATE amazon_report_runs
         SET status = 'parsing', next_attempt_at = now(), locked_at = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(run_id)
    .execute(&mut *tx)
    .await?;
    append_run_event(
        &mut tx,
        run_id,
        "parsing",
        Some("Raw report archived with SHA-256 checksum"),
    )
    .await?;
    tx.commit().await
}

pub async fn record_transport_observation(
    pool: &PgPool,
    run_id: Uuid,
    operation: &str,
    request_id_redacted: Option<&str>,
    rate_limit_limit: Option<&str>,
    retry_after_seconds: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO amazon_transport_observations
             (run_id, operation, request_id_redacted, rate_limit_limit, retry_after_seconds)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(run_id)
    .bind(operation)
    .bind(request_id_redacted)
    .bind(rate_limit_limit)
    .bind(retry_after_seconds)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_document_for_parsing(
    pool: &PgPool,
    run_id: Uuid,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT amazon_report_document_id, decoded_content
         FROM amazon_report_documents WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
}

pub async fn mark_document_import(
    pool: &PgPool,
    run_id: Uuid,
    parser_version: Option<&str>,
    import_status: &str,
    import_error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE amazon_report_documents
         SET parser_version = $2, import_status = $3, import_error = $4
         WHERE run_id = $1",
    )
    .bind(run_id)
    .bind(parser_version)
    .bind(import_status)
    .bind(import_error)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn store_snapshot(
    pool: &PgPool,
    run: &ClaimedReportRun,
    parsed: &ParsedSnapshot,
) -> Result<MetricSnapshot, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let snapshot = sqlx::query_as::<_, MetricSnapshot>(
        "INSERT INTO amazon_metric_snapshots
             (run_id, connection_id, marketplace_id, report_type, parser_version,
              period_start, period_end, granularity, comparability_key, summary)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (run_id) DO NOTHING
         RETURNING id, run_id, connection_id, marketplace_id, report_type, parser_version,
                   period_start, period_end, granularity, comparability_key, summary, created_at",
    )
    .bind(run.id)
    .bind(run.connection_id)
    .bind(&run.marketplace_id)
    .bind(&run.report_type)
    .bind(&parsed.parser_version)
    .bind(parsed.period_start.or(run.data_start_time))
    .bind(parsed.period_end.or(run.data_end_time))
    .bind(&parsed.granularity)
    .bind(&parsed.comparability_key)
    .bind(&parsed.summary)
    .fetch_optional(&mut *tx)
    .await?;
    let snapshot = if let Some(snapshot) = snapshot {
        for metric in &parsed.metrics {
            sqlx::query(
                "INSERT INTO amazon_normalized_metrics
                     (snapshot_id, metric_name, dimension_type, dimension_key, value_numeric,
                      unit, currency_code, evidence)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(snapshot.id)
            .bind(&metric.metric_name)
            .bind(&metric.dimension_type)
            .bind(&metric.dimension_key)
            .bind(metric.value_numeric)
            .bind(&metric.unit)
            .bind(&metric.currency_code)
            .bind(&metric.evidence)
            .execute(&mut *tx)
            .await?;
        }
        snapshot
    } else {
        sqlx::query_as::<_, MetricSnapshot>(
            "SELECT id, run_id, connection_id, marketplace_id, report_type, parser_version,
                    period_start, period_end, granularity, comparability_key, summary, created_at
             FROM amazon_metric_snapshots WHERE run_id = $1",
        )
        .bind(run.id)
        .fetch_one(&mut *tx)
        .await?
    };
    mark_document_import_tx(&mut tx, run.id, &parsed.parser_version, "parsed", None).await?;
    sqlx::query(
        "UPDATE amazon_report_runs
         SET status = 'analysing', next_attempt_at = now(), locked_at = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(run.id)
    .execute(&mut *tx)
    .await?;
    append_run_event(
        &mut tx,
        run.id,
        "analysing",
        Some("Normalized metric snapshot stored"),
    )
    .await?;
    enqueue_delta_analysis_tx(&mut tx, run, &snapshot).await?;
    tx.commit().await?;
    Ok(snapshot)
}

/// Stores a fully validated manual report in one transaction. Parsing happens
/// before this boundary, so a schema error cannot leave a run, raw archive, or
/// partial metric set behind. A transaction-scoped advisory lock serializes
/// concurrent uploads of the same bytes and makes retries idempotent.
pub async fn store_manual_import(
    pool: &PgPool,
    input: &ManualImportStoreInput<'_>,
) -> Result<ManualImportStoreOutcome, ManualImportStoreError> {
    if input.report_type != SALES_AND_TRAFFIC
        || !matches!(input.detected_format, "json" | "csv" | "tsv")
        || !matches!(input.date_granularity, "DAY" | "WEEK" | "MONTH" | "PERIOD")
        || input.marketplace_id.len() < 2
        || input.marketplace_id.len() > 64
        || !input
            .marketplace_id
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
        || !input
            .marketplace_id
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        || input.raw_sha256 != sha256(input.raw_content)
        || input.currency_code.is_none()
        || input.parsed.period_start.is_none()
        || input.parsed.period_end.is_none()
        || input.parsed.comparability_key.trim().is_empty()
    {
        return Err(ManualImportStoreError::InvalidInput(
            "metadata did not pass the storage boundary".to_owned(),
        ));
    }
    let period_start = input.parsed.period_start.expect("checked above");
    let period_end = input.parsed.period_end.expect("checked above");
    if period_start > period_end {
        return Err(ManualImportStoreError::InvalidInput(
            "report period is inverted".to_owned(),
        ));
    }

    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(input.raw_sha256)
        .execute(&mut *tx)
        .await?;
    if let Some(existing) = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            String,
            DateTime<Utc>,
            DateTime<Utc>,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
        ),
    >(
        "SELECT imported.run_id, imported.analysis_job_id, imported.report_type,
                imported.marketplace_id, imported.period_start, imported.period_end,
                imported.granularity, imported.source_timezone, imported.currency_code,
                imported.parser_version, imported.comparability_key, job.analysis_type
         FROM amazon_manual_report_imports imported
         JOIN amazon_analysis_jobs job ON job.id = imported.analysis_job_id
         WHERE imported.raw_sha256 = $1",
    )
    .bind(input.raw_sha256)
    .fetch_optional(&mut *tx)
    .await?
    {
        let metadata_matches = existing.2 == input.report_type
            && existing.3 == input.marketplace_id
            && existing.4 == period_start
            && existing.5 == period_end
            && existing.6 == input.date_granularity
            && existing.7 == input.source_timezone
            && existing.8.as_deref() == input.currency_code
            && existing.9 == input.parsed.parser_version
            && existing.10 == input.parsed.comparability_key;
        if !metadata_matches {
            return Err(ManualImportStoreError::MetadataConflict);
        }
        tx.commit().await?;
        return Ok(ManualImportStoreOutcome {
            run_id: existing.0,
            analysis_job_id: existing.1,
            comparison_generated: existing.11 == "manual_comparison",
            imported: false,
        });
    }

    let semantic_key = format!(
        "{}:{}:{}:{}:{}:{}",
        input.marketplace_id,
        input.report_type,
        period_start,
        period_end,
        input.parsed.comparability_key,
        input.parsed.parser_version,
    );
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 1))")
        .bind(&semantic_key)
        .execute(&mut *tx)
        .await?;
    let duplicate_period = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
           SELECT 1 FROM amazon_manual_report_imports
           WHERE marketplace_id = $1 AND report_type = $2
             AND period_start = $3 AND period_end = $4
             AND comparability_key = $5 AND parser_version = $6
         )",
    )
    .bind(input.marketplace_id)
    .bind(input.report_type)
    .bind(period_start)
    .bind(period_end)
    .bind(&input.parsed.comparability_key)
    .bind(&input.parsed.parser_version)
    .fetch_one(&mut *tx)
    .await?;
    if duplicate_period {
        return Err(ManualImportStoreError::DuplicatePeriod);
    }

    let connection_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM amazon_connections
         WHERE seller_id = 'manual-report-import' AND region = 'eu'
           AND secret_ref = 'fixture:manual-report-import' AND mode = 'fixture'",
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO amazon_marketplaces (connection_id, marketplace_id)
         VALUES ($1, $2)
         ON CONFLICT (connection_id, marketplace_id) DO UPDATE SET enabled = true",
    )
    .bind(connection_id)
    .bind(input.marketplace_id)
    .execute(&mut *tx)
    .await?;

    let run_id = Uuid::new_v4();
    let run_options = json!({
        "source": "manual_upload",
        "detectedFormat": input.detected_format,
        "dateGranularity": input.date_granularity,
        "sourceTimezone": input.source_timezone,
        "parserVersion": input.parsed.parser_version,
    });
    sqlx::query(
        "INSERT INTO amazon_report_runs
             (id, connection_id, marketplace_id, report_type, data_start_time, data_end_time,
              report_options, trigger_source, idempotency_key, status,
              amazon_report_document_id, requested_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'manual', $8, 'analysing', $9, now())",
    )
    .bind(run_id)
    .bind(connection_id)
    .bind(input.marketplace_id)
    .bind(input.report_type)
    .bind(period_start)
    .bind(period_end)
    .bind(&run_options)
    .bind(format!("amazon-manual-report:{}", input.raw_sha256))
    .bind(format!("manual-upload:{}", &input.raw_sha256[..16]))
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO amazon_report_documents
             (run_id, amazon_report_document_id, sha256, content_type, compression_algorithm,
              raw_content, decoded_content, decoded_sha256, parser_version, import_status)
         VALUES ($1, $2, $3, $4, 'NONE', $5, $5, $3, $6, 'parsed')",
    )
    .bind(run_id)
    .bind(format!("manual-upload:{}", &input.raw_sha256[..16]))
    .bind(input.raw_sha256)
    .bind(input.content_type)
    .bind(input.raw_content)
    .bind(&input.parsed.parser_version)
    .execute(&mut *tx)
    .await?;

    let snapshot_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO amazon_metric_snapshots
             (id, run_id, connection_id, marketplace_id, report_type, parser_version,
              period_start, period_end, granularity, comparability_key, summary)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(snapshot_id)
    .bind(run_id)
    .bind(connection_id)
    .bind(input.marketplace_id)
    .bind(input.report_type)
    .bind(&input.parsed.parser_version)
    .bind(period_start)
    .bind(period_end)
    .bind(input.date_granularity)
    .bind(&input.parsed.comparability_key)
    .bind(&input.parsed.summary)
    .execute(&mut *tx)
    .await?;
    for metric in &input.parsed.metrics {
        sqlx::query(
            "INSERT INTO amazon_normalized_metrics
                 (snapshot_id, metric_name, dimension_type, dimension_key, value_numeric,
                  unit, currency_code, evidence)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(snapshot_id)
        .bind(&metric.metric_name)
        .bind(&metric.dimension_type)
        .bind(&metric.dimension_key)
        .bind(metric.value_numeric)
        .bind(&metric.unit)
        .bind(&metric.currency_code)
        .bind(&metric.evidence)
        .execute(&mut *tx)
        .await?;
    }

    let comparison = sqlx::query_as::<_, (Uuid, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT run_id, period_start, period_end
         FROM amazon_metric_snapshots
         WHERE connection_id = $1 AND marketplace_id = $2 AND report_type = $3
           AND comparability_key = $4 AND parser_version = $5 AND id <> $6
           AND (period_end < $7 OR period_start > $8)
         ORDER BY CASE WHEN period_end < $7 THEN $7 - period_end
                       ELSE period_start - $8 END,
                  created_at DESC
         LIMIT 1",
    )
    .bind(connection_id)
    .bind(input.marketplace_id)
    .bind(input.report_type)
    .bind(&input.parsed.comparability_key)
    .bind(&input.parsed.parser_version)
    .bind(snapshot_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_optional(&mut *tx)
    .await?;
    let analysis_job_id = Uuid::new_v4();
    let (analysis_type, analysis_start, analysis_end) = comparison
        .map(|(_, other_start, other_end)| {
            (
                "manual_comparison",
                period_start.min(other_start),
                period_end.max(other_end),
            )
        })
        .unwrap_or(("delta", period_start, period_end));
    sqlx::query(
        "INSERT INTO amazon_analysis_jobs
             (id, run_id, connection_id, marketplace_id, report_type, analysis_type,
              period_start, period_end)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(analysis_job_id)
    .bind(run_id)
    .bind(connection_id)
    .bind(input.marketplace_id)
    .bind(input.report_type)
    .bind(analysis_type)
    .bind(analysis_start)
    .bind(analysis_end)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO amazon_manual_report_imports
             (run_id, analysis_job_id, raw_sha256, detected_format, report_type, marketplace_id,
              period_start, period_end, granularity, source_timezone, currency_code,
              parser_version, comparability_key, uploaded_by, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
    )
    .bind(run_id)
    .bind(analysis_job_id)
    .bind(input.raw_sha256)
    .bind(input.detected_format)
    .bind(input.report_type)
    .bind(input.marketplace_id)
    .bind(period_start)
    .bind(period_end)
    .bind(input.date_granularity)
    .bind(input.source_timezone)
    .bind(input.currency_code)
    .bind(&input.parsed.parser_version)
    .bind(&input.parsed.comparability_key)
    .bind(input.uploaded_by)
    .bind(json!({
        "archive": "postgresql_immutable",
        "raw_bytes": input.raw_content.len(),
        "partial_imports": "transactionally_prevented",
    }))
    .execute(&mut *tx)
    .await?;
    append_run_event(
        &mut tx,
        run_id,
        "archived",
        Some("Manual raw report archived with SHA-256 checksum"),
    )
    .await?;
    append_run_event(
        &mut tx,
        run_id,
        "analysing",
        Some("Manual report validated and normalized atomically"),
    )
    .await?;
    tx.commit().await?;
    Ok(ManualImportStoreOutcome {
        run_id,
        analysis_job_id,
        comparison_generated: analysis_type == "manual_comparison",
        imported: true,
    })
}

async fn mark_document_import_tx(
    tx: &mut Transaction<'_, Postgres>,
    run_id: Uuid,
    parser_version: &str,
    import_status: &str,
    import_error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE amazon_report_documents
         SET parser_version = $2, import_status = $3, import_error = $4 WHERE run_id = $1",
    )
    .bind(run_id)
    .bind(parser_version)
    .bind(import_status)
    .bind(import_error)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn enqueue_delta_analysis_tx(
    tx: &mut Transaction<'_, Postgres>,
    run: &ClaimedReportRun,
    _snapshot: &MetricSnapshot,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO amazon_analysis_jobs
             (run_id, connection_id, marketplace_id, report_type, analysis_type, period_start, period_end)
         VALUES ($1, $2, $3, $4, 'delta', $5, $6)
         ON CONFLICT DO NOTHING",
    )
    .bind(run.id)
    .bind(run.connection_id)
    .bind(&run.marketplace_id)
    .bind(&run.report_type)
    .bind(run.data_start_time)
    .bind(run.data_end_time)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn mark_run_archived(
    pool: &PgPool,
    run_id: Uuid,
    parser_version: Option<&str>,
    message: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    mark_document_import_tx(
        &mut tx,
        run_id,
        parser_version.unwrap_or("raw-only"),
        "unsupported",
        Some(message),
    )
    .await?;
    sqlx::query(
        "UPDATE amazon_report_runs
         SET status = 'archived', completed_at = now(), locked_at = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(run_id)
    .execute(&mut *tx)
    .await?;
    append_run_event(&mut tx, run_id, "archived", Some(message)).await?;
    tx.commit().await
}

pub async fn mark_run_failure(
    pool: &PgPool,
    run_id: Uuid,
    code: &str,
    message: &str,
    retry_after_seconds: Option<i64>,
) -> Result<(), sqlx::Error> {
    let retry = retry_after_seconds.unwrap_or(0);
    let retryable = retry_after_seconds.is_some();
    let status = if retryable { "polling" } else { "failed" };
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE amazon_report_runs
         SET status = $2, failure_code = $3, failure_message = $4,
             next_attempt_at = now() + make_interval(secs => $5), locked_at = NULL,
             completed_at = CASE WHEN $2 = 'failed' THEN now() ELSE NULL END, updated_at = now()
         WHERE id = $1",
    )
    .bind(run_id)
    .bind(status)
    .bind(code)
    .bind(message)
    .bind(retry as f64)
    .execute(&mut *tx)
    .await?;
    append_run_event(&mut tx, run_id, status, Some(message)).await?;
    tx.commit().await
}

pub async fn retry_run(
    pool: &PgPool,
    run_id: Uuid,
    status: &str,
    code: &str,
    message: &str,
    delay_seconds: i64,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE amazon_report_runs
         SET status = $2, failure_code = $3, failure_message = $4,
             next_attempt_at = now() + make_interval(secs => $5), locked_at = NULL,
             updated_at = now()
         WHERE id = $1",
    )
    .bind(run_id)
    .bind(status)
    .bind(code)
    .bind(message)
    .bind(delay_seconds as f64)
    .execute(&mut *tx)
    .await?;
    append_run_event(&mut tx, run_id, status, Some(message)).await?;
    tx.commit().await
}

pub async fn mark_run_terminal(
    pool: &PgPool,
    run_id: Uuid,
    status: &str,
    code: &str,
    message: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE amazon_report_runs
         SET status = $2, failure_code = $3, failure_message = $4, completed_at = now(),
             locked_at = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(run_id)
    .bind(status)
    .bind(code)
    .bind(message)
    .execute(&mut *tx)
    .await?;
    append_run_event(&mut tx, run_id, status, Some(message)).await?;
    tx.commit().await
}

pub async fn mark_parse_failure(
    pool: &PgPool,
    run_id: Uuid,
    parser_version: &str,
    message: &str,
) -> Result<(), sqlx::Error> {
    mark_document_import(pool, run_id, Some(parser_version), "failed", Some(message)).await?;
    mark_run_failure(pool, run_id, "parser_error", message, None).await
}

pub async fn create_total_analysis(
    pool: &PgPool,
    connection_id: Uuid,
    marketplace_id: &str,
    report_type: &str,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<AnalysisJob, sqlx::Error> {
    sqlx::query_as::<_, AnalysisJob>(
        "INSERT INTO amazon_analysis_jobs
             (connection_id, marketplace_id, report_type, analysis_type, period_start, period_end)
         VALUES ($1, $2, $3, 'total', $4, $5)
         RETURNING id, run_id, connection_id, marketplace_id, report_type, analysis_type,
                   period_start, period_end, status, attempts, error_message, created_at, completed_at",
    )
    .bind(connection_id)
    .bind(marketplace_id)
    .bind(report_type)
    .bind(period_start)
    .bind(period_end)
    .fetch_one(pool)
    .await
}

pub async fn claim_analysis_jobs(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<ClaimedAnalysisJob>, sqlx::Error> {
    sqlx::query(
        "UPDATE amazon_analysis_jobs
         SET status = 'queued', locked_at = NULL, next_attempt_at = now()
         WHERE status = 'processing' AND locked_at < now() - interval '5 minutes'",
    )
    .execute(pool)
    .await?;
    sqlx::query_as::<_, ClaimedAnalysisJob>(
        "WITH selected AS (
             SELECT id FROM amazon_analysis_jobs
             WHERE status = 'queued' AND next_attempt_at <= now()
             ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT $1
         )
         UPDATE amazon_analysis_jobs job SET status = 'processing', locked_at = now(),
             attempts = job.attempts + 1
         FROM selected WHERE job.id = selected.id
         RETURNING job.id, job.run_id, job.connection_id, job.marketplace_id, job.report_type,
                   job.analysis_type, job.period_start, job.period_end, job.attempts",
    )
    .bind(limit.clamp(1, 25))
    .fetch_all(pool)
    .await
}

pub async fn snapshot_for_run(
    pool: &PgPool,
    run_id: Uuid,
) -> Result<Option<MetricSnapshot>, sqlx::Error> {
    sqlx::query_as::<_, MetricSnapshot>(
        "SELECT id, run_id, connection_id, marketplace_id, report_type, parser_version,
                period_start, period_end, granularity, comparability_key, summary, created_at
         FROM amazon_metric_snapshots WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
}

pub async fn metrics_for_snapshot(
    pool: &PgPool,
    snapshot_id: Uuid,
) -> Result<Vec<NormalizedMetric>, sqlx::Error> {
    sqlx::query_as::<_, NormalizedMetric>(
        "SELECT id, snapshot_id, metric_name, dimension_type, dimension_key, value_numeric,
                unit, currency_code, evidence
         FROM amazon_normalized_metrics WHERE snapshot_id = $1
         ORDER BY metric_name, dimension_type, dimension_key",
    )
    .bind(snapshot_id)
    .fetch_all(pool)
    .await
}

pub async fn previous_compatible_snapshot(
    pool: &PgPool,
    snapshot: &MetricSnapshot,
) -> Result<Option<MetricSnapshot>, sqlx::Error> {
    sqlx::query_as::<_, MetricSnapshot>(
        "SELECT id, run_id, connection_id, marketplace_id, report_type, parser_version,
                period_start, period_end, granularity, comparability_key, summary, created_at
         FROM amazon_metric_snapshots
         WHERE connection_id = $1 AND marketplace_id = $2 AND report_type = $3
           AND comparability_key = $4 AND parser_version = $5
           AND id <> $6 AND period_end < $7
         ORDER BY period_end DESC, period_start DESC, created_at DESC LIMIT 1",
    )
    .bind(snapshot.connection_id)
    .bind(&snapshot.marketplace_id)
    .bind(&snapshot.report_type)
    .bind(&snapshot.comparability_key)
    .bind(&snapshot.parser_version)
    .bind(snapshot.id)
    .bind(snapshot.period_start)
    .fetch_optional(pool)
    .await
}

pub async fn snapshots_for_window(
    pool: &PgPool,
    job: &ClaimedAnalysisJob,
) -> Result<Vec<MetricSnapshot>, sqlx::Error> {
    sqlx::query_as::<_, MetricSnapshot>(
        "SELECT id, run_id, connection_id, marketplace_id, report_type, parser_version,
                period_start, period_end, granularity, comparability_key, summary, created_at
         FROM amazon_metric_snapshots
         WHERE connection_id = $1 AND marketplace_id = $2
           AND ($3::text IS NULL OR report_type = $3)
           AND ($4::timestamptz IS NULL OR period_start >= $4)
           AND ($5::timestamptz IS NULL OR period_end <= $5)
         ORDER BY period_start, created_at",
    )
    .bind(job.connection_id)
    .bind(&job.marketplace_id)
    .bind(&job.report_type)
    .bind(job.period_start)
    .bind(job.period_end)
    .fetch_all(pool)
    .await
}

pub async fn complete_analysis(
    pool: &PgPool,
    job: &ClaimedAnalysisJob,
    strategy: &str,
    model_name: Option<&str>,
    prompt_version: &str,
    payload_hash: &str,
    result: &Value,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO amazon_analysis_results
             (job_id, strategy, model_name, prompt_version, payload_sha256, result)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (job_id) DO NOTHING",
    )
    .bind(job.id)
    .bind(strategy)
    .bind(model_name)
    .bind(prompt_version)
    .bind(payload_hash)
    .bind(result)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE amazon_analysis_jobs
         SET status = 'completed', locked_at = NULL, completed_at = now(), error_message = NULL
         WHERE id = $1",
    )
    .bind(job.id)
    .execute(&mut *tx)
    .await?;
    if let Some(run_id) = job.run_id {
        sqlx::query(
            "UPDATE amazon_report_runs
             SET status = 'succeeded', completed_at = now(), locked_at = NULL, updated_at = now()
             WHERE id = $1 AND status = 'analysing'",
        )
        .bind(run_id)
        .execute(&mut *tx)
        .await?;
        append_run_event(
            &mut tx,
            run_id,
            "succeeded",
            Some("Deterministic analysis completed"),
        )
        .await?;
    }
    tx.commit().await
}

pub async fn analysis_result_for_job(
    pool: &PgPool,
    job_id: Uuid,
) -> Result<Option<AnalysisResult>, sqlx::Error> {
    sqlx::query_as::<_, AnalysisResult>(
        "SELECT id, job_id, strategy, model_name, prompt_version, payload_sha256,
                result, created_at
         FROM amazon_analysis_results WHERE job_id = $1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
}

pub async fn fail_analysis(pool: &PgPool, job_id: Uuid, message: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE amazon_analysis_jobs
         SET status = 'failed', locked_at = NULL, error_message = $2, completed_at = now()
         WHERE id = $1",
    )
    .bind(job_id)
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_run_detail(
    pool: &PgPool,
    run_id: Uuid,
) -> Result<Option<AmazonRunDetail>, sqlx::Error> {
    let Some(run) = sqlx::query_as::<_, AmazonReportRun>(
        "SELECT id, connection_id, schedule_id, marketplace_id, report_type, data_start_time,
                data_end_time, report_options, trigger_source, status, attempts, poll_attempts,
                next_attempt_at, amazon_report_id, amazon_report_document_id, failure_code,
                failure_message, requested_at, completed_at, created_at, updated_at
         FROM amazon_report_runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let events = sqlx::query_as::<_, AmazonRunEvent>(
        "SELECT id, run_id, status, message, created_at FROM amazon_report_run_events
         WHERE run_id = $1 ORDER BY id",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;
    let document = sqlx::query_as::<_, AmazonReportDocumentInfo>(
        "SELECT id, run_id, amazon_report_document_id, sha256, decoded_sha256,
                content_type, compression_algorithm,
                downloaded_at, parser_version, import_status, import_error,
                octet_length(raw_content)::bigint AS transport_bytes,
                octet_length(decoded_content)::bigint AS decoded_bytes
         FROM amazon_report_documents WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await?;
    let snapshot = snapshot_for_run(pool, run_id).await?;
    let metrics = match &snapshot {
        Some(snapshot) => metrics_for_snapshot(pool, snapshot.id).await?,
        None => Vec::new(),
    };
    let analyses = sqlx::query_as::<_, AnalysisResult>(
        "SELECT result.id, result.job_id, result.strategy, result.model_name, result.prompt_version,
                result.payload_sha256, result.result, result.created_at
         FROM amazon_analysis_results result
         JOIN amazon_analysis_jobs job ON job.id = result.job_id
         WHERE job.run_id = $1 ORDER BY result.created_at DESC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;
    let transport = sqlx::query_as::<_, AmazonTransportObservation>(
        "SELECT id, run_id, operation, request_id_redacted, rate_limit_limit,
                retry_after_seconds, observed_at
         FROM amazon_transport_observations WHERE run_id = $1 ORDER BY id",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;
    Ok(Some(AmazonRunDetail {
        run,
        events,
        document,
        snapshot,
        metrics,
        analyses,
        transport,
    }))
}

pub async fn raw_document(
    pool: &PgPool,
    run_id: Uuid,
) -> Result<Option<RawReportDocument>, sqlx::Error> {
    sqlx::query_as::<_, (Option<String>, Vec<u8>)>(
        "SELECT CASE WHEN compression_algorithm = 'GZIP' THEN 'application/gzip'
                     ELSE content_type END,
                raw_content
         FROM amazon_report_documents WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.map(|(content_type, content)| RawReportDocument {
            content_type,
            content,
        })
    })
}

pub async fn analysis_result(
    pool: &PgPool,
    analysis_id: Uuid,
) -> Result<Option<AnalysisResult>, sqlx::Error> {
    sqlx::query_as::<_, AnalysisResult>(
        "SELECT id, job_id, strategy, model_name, prompt_version, payload_sha256, result, created_at
         FROM amazon_analysis_results WHERE id = $1",
    )
    .bind(analysis_id)
    .fetch_optional(pool)
    .await
}

pub async fn ai_strategy_assessment(
    pool: &PgPool,
    analysis_id: Uuid,
    payload_sha256: &str,
    model_name: &str,
    prompt_version: &str,
) -> Result<Option<AiStrategyAssessment>, sqlx::Error> {
    sqlx::query_as::<_, AiStrategyAssessment>(
        "SELECT id, analysis_id, payload_sha256, model_name, prompt_version, result,
                provider_request_id_redacted, input_tokens, output_tokens, created_by, created_at
         FROM amazon_ai_strategy_assessments
         WHERE analysis_id = $1 AND payload_sha256 = $2
           AND model_name = $3 AND prompt_version = $4",
    )
    .bind(analysis_id)
    .bind(payload_sha256)
    .bind(model_name)
    .bind(prompt_version)
    .fetch_optional(pool)
    .await
}

/// Persist only the validated structured model result. The provider prompt,
/// aggregate input document, authorization secret and raw provider response are
/// intentionally absent from both this table and the administrative audit log.
pub async fn store_ai_strategy_assessment(
    pool: &PgPool,
    input: &StoreAiStrategyAssessment<'_>,
) -> Result<(AiStrategyAssessment, bool), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let inserted = sqlx::query_as::<_, AiStrategyAssessment>(
        "INSERT INTO amazon_ai_strategy_assessments
             (analysis_id, payload_sha256, model_name, prompt_version, result,
              provider_request_id_redacted, input_tokens, output_tokens, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (analysis_id, payload_sha256, model_name, prompt_version) DO NOTHING
         RETURNING id, analysis_id, payload_sha256, model_name, prompt_version, result,
                   provider_request_id_redacted, input_tokens, output_tokens, created_by, created_at",
    )
    .bind(input.analysis_id)
    .bind(input.payload_sha256)
    .bind(input.model_name)
    .bind(input.prompt_version)
    .bind(input.result)
    .bind(input.provider_request_id_redacted)
    .bind(input.input_tokens)
    .bind(input.output_tokens)
    .bind(input.created_by)
    .fetch_optional(&mut *tx)
    .await?;
    let was_inserted = inserted.is_some();
    let assessment = match inserted {
        Some(assessment) => assessment,
        None => sqlx::query_as::<_, AiStrategyAssessment>(
            "SELECT id, analysis_id, payload_sha256, model_name, prompt_version, result,
                    provider_request_id_redacted, input_tokens, output_tokens, created_by, created_at
             FROM amazon_ai_strategy_assessments
             WHERE analysis_id = $1 AND payload_sha256 = $2
               AND model_name = $3 AND prompt_version = $4",
        )
        .bind(input.analysis_id)
        .bind(input.payload_sha256)
        .bind(input.model_name)
        .bind(input.prompt_version)
        .fetch_one(&mut *tx)
        .await?,
    };
    if was_inserted {
        let idempotency_key = format!(
            "amazon-ai-strategy:{}:{}:{}:{}",
            input.analysis_id, input.payload_sha256, input.model_name, input.prompt_version
        );
        sqlx::query(
            "INSERT INTO administrative_audit_log
                 (actor_user_id, action, target_type, target_id, idempotency_key, details)
             VALUES ($1, 'amazon.ai_strategy_assessed', 'amazon_analysis', $2, $3, $4)
             ON CONFLICT (action, idempotency_key) DO NOTHING",
        )
        .bind(input.created_by)
        .bind(input.analysis_id.to_string())
        .bind(idempotency_key)
        .bind(json!({
            "payload_sha256": input.payload_sha256,
            "model_name": input.model_name,
            "prompt_version": input.prompt_version,
            "aggregate_only": true,
            "response_storage": "store_false",
            "amazon_mutation": false,
        }))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((assessment, was_inserted))
}

pub async fn overview(pool: &PgPool) -> Result<MarketplaceOverview, sqlx::Error> {
    let connections = sqlx::query_as::<_, AmazonConnection>(
        "SELECT id, seller_id, region, secret_ref, granted_roles, mode, enabled, created_at, updated_at
         FROM amazon_connections
         WHERE seller_id <> 'manual-report-import'
         ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    let mut connection_summaries = Vec::with_capacity(connections.len());
    for connection in connections {
        let marketplaces = list_marketplace_ids(pool, connection.id).await?;
        connection_summaries.push(AmazonConnectionSummary::from_connection(
            connection,
            marketplaces,
        ));
    }
    let schedules = sqlx::query_as::<_, AmazonReportSchedule>(
        "SELECT id, connection_id, marketplace_id, report_type, report_options, interval_seconds,
                enabled, next_run_at, last_enqueued_at, created_at, updated_at
         FROM amazon_report_schedules ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    let recent_runs = sqlx::query_as::<_, AmazonReportRun>(
        "SELECT id, connection_id, schedule_id, marketplace_id, report_type, data_start_time,
                data_end_time, report_options, trigger_source, status, attempts, poll_attempts,
                next_attempt_at, amazon_report_id, amazon_report_document_id, failure_code,
                failure_message, requested_at, completed_at, created_at, updated_at
         FROM amazon_report_runs ORDER BY created_at DESC LIMIT 30",
    )
    .fetch_all(pool)
    .await?;
    let analyses = sqlx::query_as::<_, AnalysisResult>(
        "SELECT id, job_id, strategy, model_name, prompt_version, payload_sha256, result, created_at
         FROM amazon_analysis_results ORDER BY created_at DESC LIMIT 20",
    )
    .fetch_all(pool)
    .await?;
    Ok(MarketplaceOverview {
        connections: connection_summaries,
        schedules,
        recent_runs,
        analyses,
        report_types: report_definitions().to_vec(),
    })
}

pub fn report_type_is_allowed_for_connection(
    connection: &AmazonConnection,
    report_type: &str,
) -> bool {
    let Some(definition) = report_definition(report_type) else {
        return connection.mode == "fixture" && report_type.starts_with("GET_");
    };
    definition.regions.contains(&connection.region.as_str())
        && definition
            .required_roles
            .iter()
            .any(|required| connection.granted_roles.iter().any(|role| role == required))
}

pub fn report_options_are_supported(report_type: &str, options: &Value) -> bool {
    let Some(definition) = report_definition(report_type) else {
        return options.as_object().is_some_and(serde_json::Map::is_empty);
    };
    let Some(object) = options.as_object() else {
        return false;
    };
    if !object
        .keys()
        .all(|option| definition.supported_options.contains(&option.as_str()))
    {
        return false;
    }
    if report_type != SALES_AND_TRAFFIC {
        return object.is_empty();
    }
    object.iter().all(|(name, value)| {
        let Some(value) = value.as_str() else {
            return false;
        };
        match name.as_str() {
            "dateGranularity" => matches!(value, "DAY" | "WEEK" | "MONTH"),
            "asinGranularity" => matches!(value, "PARENT" | "CHILD" | "SKU"),
            _ => false,
        }
    })
}

pub fn default_analysis_payload(job: &ClaimedAnalysisJob, snapshots: &[MetricSnapshot]) -> Value {
    json!({
        "analysis_type": job.analysis_type,
        "connection_id": job.connection_id,
        "marketplace_id": job.marketplace_id,
        "report_type": job.report_type,
        "period_start": job.period_start,
        "period_end": job.period_end,
        "snapshot_ids": snapshots.iter().map(|snapshot| snapshot.id).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_makes_option_hashes_order_independent() {
        let left = serde_json::json!({ "first": 1, "second": [true, "x"] });
        let right = serde_json::json!({ "second": [true, "x"], "first": 1 });
        assert_eq!(canonical_json(&left), canonical_json(&right));
    }

    #[test]
    fn only_registered_options_are_accepted() {
        assert!(report_options_are_supported(SALES_AND_TRAFFIC, &json!({})));
        assert!(report_options_are_supported(
            SALES_AND_TRAFFIC,
            &json!({ "dateGranularity": "DAY", "asinGranularity": "CHILD" })
        ));
        assert!(!report_options_are_supported(
            SALES_AND_TRAFFIC,
            &json!({ "dateGranularity": "HOUR" })
        ));
        assert!(!report_options_are_supported(
            SALES_AND_TRAFFIC,
            &json!({ "unsafe": true })
        ));
        assert!(report_options_are_supported(
            "GET_SYNTHETIC_UNKNOWN_REPORT",
            &json!({})
        ));
    }

    #[test]
    fn connection_mode_and_transport_selector_cannot_diverge() {
        assert!(transport_mode_is_consistent("fixture", "fixture:demo"));
        assert!(transport_mode_is_consistent("live", "pilot_seller"));
        assert!(!transport_mode_is_consistent("fixture", "pilot_seller"));
        assert!(!transport_mode_is_consistent("live", "fixture:demo"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn previous_snapshot_is_selected_by_report_period_not_import_order(pool: PgPool) {
        let connection = create_demo_connection(&pool).await.unwrap();
        let mut snapshots = Vec::new();
        for (key, period_start, period_end, created_at) in [
            (
                "older-period-imported-later",
                "2026-07-01T00:00:00Z",
                "2026-07-07T23:59:59Z",
                "2026-08-20T12:00:00Z",
            ),
            (
                "current-period-imported-first",
                "2026-07-08T00:00:00Z",
                "2026-07-14T23:59:59Z",
                "2026-08-20T11:00:00Z",
            ),
        ] {
            let run_id: Uuid = sqlx::query_scalar(
                "INSERT INTO amazon_report_runs (
                     connection_id, marketplace_id, report_type, trigger_source,
                     idempotency_key, status, completed_at
                 ) VALUES ($1, 'A1PA6795UKMFR9', $2, 'manual', $3, 'succeeded', now())
                 RETURNING id",
            )
            .bind(connection.id)
            .bind(SALES_AND_TRAFFIC)
            .bind(key)
            .fetch_one(&pool)
            .await
            .unwrap();
            let snapshot = sqlx::query_as::<_, MetricSnapshot>(
                "INSERT INTO amazon_metric_snapshots (
                     run_id, connection_id, marketplace_id, report_type, parser_version,
                     period_start, period_end, granularity, comparability_key, summary, created_at
                 ) VALUES ($1, $2, 'A1PA6795UKMFR9', $3, 'parser-v1',
                     $4::timestamptz, $5::timestamptz, 'day_child',
                     'sales-traffic:day_child:7d', '{}', $6::timestamptz)
                 RETURNING id, run_id, connection_id, marketplace_id, report_type,
                     parser_version, period_start, period_end, granularity,
                     comparability_key, summary, created_at",
            )
            .bind(run_id)
            .bind(connection.id)
            .bind(SALES_AND_TRAFFIC)
            .bind(period_start)
            .bind(period_end)
            .bind(created_at)
            .fetch_one(&pool)
            .await
            .unwrap();
            snapshots.push(snapshot);
        }

        let previous = previous_compatible_snapshot(&pool, &snapshots[1])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(previous.id, snapshots[0].id);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn raw_archive_bytes_and_rows_are_immutable(pool: PgPool) {
        let connection = create_demo_connection(&pool).await.unwrap();
        let run = create_manual_run(
            &pool,
            connection.id,
            &CreateAmazonReportRunInput {
                marketplace_id: connection.marketplace_ids[0].clone(),
                report_type: SALES_AND_TRAFFIC.to_owned(),
                data_start_time: None,
                data_end_time: None,
                report_options: json!({}),
            },
        )
        .await
        .unwrap();
        archive_document(
            &pool,
            run.id,
            "synthetic-document",
            Some("application/json"),
            None,
            b"synthetic immutable transport bytes",
            b"synthetic immutable transport bytes",
        )
        .await
        .unwrap();
        assert!(sqlx::query(
            "UPDATE amazon_report_documents SET raw_content = 'changed'::bytea WHERE run_id = $1",
        )
        .bind(run.id)
        .execute(&pool)
        .await
        .is_err());
        assert!(
            sqlx::query("DELETE FROM amazon_report_documents WHERE run_id = $1")
                .bind(run.id)
                .execute(&pool)
                .await
                .is_err()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn manual_import_is_atomic_immutable_and_idempotent(pool: PgPool) {
        let uploaded_by: Uuid = sqlx::query_scalar(
            "INSERT INTO users (username, password_hash, role)
             VALUES ('synthetic-manual-import-admin', 'synthetic-not-a-secret', 'administrator')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let raw = b"SYNTHETIC TEST DATA - valid bytes are parsed before this boundary";
        let raw_sha256 = sha256(raw);
        let start = "2026-07-01T00:00:00Z".parse().unwrap();
        let end = "2026-07-07T23:59:59Z".parse().unwrap();
        let parsed = ParsedSnapshot {
            parser_version: "manual-sales-traffic-v1".to_owned(),
            period_start: Some(start),
            period_end: Some(end),
            granularity: "day_child".to_owned(),
            comparability_key: "sales-traffic:DAY:EUR:Europe/Berlin:7d".to_owned(),
            summary: json!({
                "data_freshness": "2026-07-07",
                "missing_fields": [],
                "timezone": "Europe/Berlin",
                "currency_code": "EUR",
            }),
            metrics: vec![ParsedMetric {
                metric_name: "ordered_product_sales".to_owned(),
                dimension_type: "catalog".to_owned(),
                dimension_key: String::new(),
                value_numeric: Decimal::from(123),
                unit: "currency".to_owned(),
                currency_code: Some("EUR".to_owned()),
                evidence: json!({ "source": "synthetic_test" }),
            }],
        };
        let input = ManualImportStoreInput {
            uploaded_by,
            raw_sha256: &raw_sha256,
            raw_content: raw,
            content_type: "application/json",
            detected_format: "json",
            marketplace_id: "SYNTHETIC-MARKETPLACE",
            report_type: SALES_AND_TRAFFIC,
            date_granularity: "DAY",
            source_timezone: "Europe/Berlin",
            currency_code: Some("EUR"),
            parsed: &parsed,
        };

        let first = store_manual_import(&pool, &input).await.unwrap();
        let second = store_manual_import(&pool, &input).await.unwrap();
        assert!(first.imported);
        assert!(!second.imported);
        assert_eq!(first.run_id, second.run_id);
        let mut conflicting = input.clone();
        conflicting.source_timezone = "UTC";
        assert!(matches!(
            store_manual_import(&pool, &conflicting).await,
            Err(ManualImportStoreError::MetadataConflict)
        ));
        let different_raw = b"SYNTHETIC TEST DATA - same semantic period, different bytes";
        let mut duplicate_period = input.clone();
        let different_hash = sha256(different_raw);
        duplicate_period.raw_content = different_raw;
        duplicate_period.raw_sha256 = &different_hash;
        assert!(matches!(
            store_manual_import(&pool, &duplicate_period).await,
            Err(ManualImportStoreError::DuplicatePeriod)
        ));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM amazon_manual_report_imports WHERE raw_sha256 = $1",
            )
            .bind(&raw_sha256)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        let archived = raw_document(&pool, first.run_id).await.unwrap().unwrap();
        assert_eq!(archived.content, raw);
        assert!(
            sqlx::query("DELETE FROM amazon_manual_report_imports WHERE run_id = $1")
                .bind(first.run_id)
                .execute(&pool)
                .await
                .is_err()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn manual_import_compares_periods_independent_of_upload_order(pool: PgPool) {
        let uploaded_by: Uuid = sqlx::query_scalar(
            "INSERT INTO users (username, password_hash, role)
             VALUES ('synthetic-order-admin', 'synthetic-not-a-secret', 'administrator')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let make_snapshot = |start: &str, end: &str, revenue: i64| ParsedSnapshot {
            parser_version: "manual-sales-traffic-v1".to_owned(),
            period_start: Some(start.parse().unwrap()),
            period_end: Some(end.parse().unwrap()),
            granularity: "day_child".to_owned(),
            comparability_key: "sales-traffic:DAY:EUR:Europe/Berlin:7d".to_owned(),
            summary: json!({
                "data_freshness": end,
                "missing_fields": [],
                "timezone": "Europe/Berlin",
                "currency_code": "EUR",
            }),
            metrics: vec![ParsedMetric {
                metric_name: "ordered_product_sales".to_owned(),
                dimension_type: "catalog".to_owned(),
                dimension_key: String::new(),
                value_numeric: Decimal::from(revenue),
                unit: "currency".to_owned(),
                currency_code: Some("EUR".to_owned()),
                evidence: json!({ "source": "synthetic_test" }),
            }],
        };
        let newer_raw = b"SYNTHETIC NEWER REPORT";
        let newer_hash = sha256(newer_raw);
        let newer = make_snapshot("2026-08-08T00:00:00Z", "2026-08-14T23:59:59Z", 140);
        let newer_input = ManualImportStoreInput {
            uploaded_by,
            raw_sha256: &newer_hash,
            raw_content: newer_raw,
            content_type: "application/json",
            detected_format: "json",
            marketplace_id: "SYNTHETIC-MARKETPLACE",
            report_type: SALES_AND_TRAFFIC,
            date_granularity: "DAY",
            source_timezone: "Europe/Berlin",
            currency_code: Some("EUR"),
            parsed: &newer,
        };
        let newer_outcome = store_manual_import(&pool, &newer_input).await.unwrap();

        let older_raw = b"SYNTHETIC OLDER REPORT";
        let older_hash = sha256(older_raw);
        let older = make_snapshot("2026-08-01T00:00:00Z", "2026-08-07T23:59:59Z", 100);
        let older_input = ManualImportStoreInput {
            uploaded_by,
            raw_sha256: &older_hash,
            raw_content: older_raw,
            content_type: "application/json",
            detected_format: "json",
            marketplace_id: "SYNTHETIC-MARKETPLACE",
            report_type: SALES_AND_TRAFFIC,
            date_granularity: "DAY",
            source_timezone: "Europe/Berlin",
            currency_code: Some("EUR"),
            parsed: &older,
        };
        let older_outcome = store_manual_import(&pool, &older_input).await.unwrap();
        let duplicate = store_manual_import(&pool, &older_input).await.unwrap();

        assert_ne!(newer_outcome.run_id, older_outcome.run_id);
        assert_eq!(duplicate.analysis_job_id, older_outcome.analysis_job_id);
        assert!(!duplicate.imported);
        let comparison: (String, Uuid, DateTime<Utc>, DateTime<Utc>) = sqlx::query_as(
            "SELECT analysis_type, run_id, period_start, period_end
             FROM amazon_analysis_jobs WHERE id = $1",
        )
        .bind(older_outcome.analysis_job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(comparison.0, "manual_comparison");
        assert_eq!(comparison.1, older_outcome.run_id);
        assert_eq!(comparison.2, older.period_start.unwrap());
        assert_eq!(comparison.3, newer.period_end.unwrap());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn ai_strategy_assessment_is_idempotent_immutable_and_metadata_only(pool: PgPool) {
        let created_by: Uuid = sqlx::query_scalar(
            "INSERT INTO users (username, password_hash, role)
             VALUES ('synthetic-ai-admin', 'synthetic-not-a-secret', 'administrator')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let connection_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM amazon_connections WHERE seller_id = 'manual-report-import'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO amazon_analysis_jobs
                 (connection_id, marketplace_id, report_type, analysis_type, status, completed_at)
             VALUES ($1, 'SYNTHETIC-MARKETPLACE', $2, 'delta', 'completed', now())
             RETURNING id",
        )
        .bind(connection_id)
        .bind(SALES_AND_TRAFFIC)
        .fetch_one(&pool)
        .await
        .unwrap();
        let analysis_id: Uuid = sqlx::query_scalar(
            "INSERT INTO amazon_analysis_results
                 (job_id, strategy, model_name, prompt_version, payload_sha256, result)
             VALUES ($1, 'deterministic_rules', NULL, 'rules-v1', $2, $3)
             RETURNING id",
        )
        .bind(job_id)
        .bind("0".repeat(64))
        .bind(json!({"facts": [{"metric": "sessions", "value": "20"}]}))
        .fetch_one(&pool)
        .await
        .unwrap();
        let result = json!({
            "executive_summary": "Synthetic aggregate assessment",
            "recommended_actions": [],
        });
        let payload_sha256 = "a".repeat(64);
        let input = StoreAiStrategyAssessment {
            analysis_id,
            payload_sha256: &payload_sha256,
            model_name: "gpt-5.6",
            prompt_version: "mantle-amazon-strategy-v1",
            result: &result,
            provider_request_id_redacted: Some("0123456789ab"),
            input_tokens: Some(100),
            output_tokens: Some(50),
            created_by,
        };
        let (first, first_inserted) = store_ai_strategy_assessment(&pool, &input).await.unwrap();
        let (second, second_inserted) = store_ai_strategy_assessment(&pool, &input).await.unwrap();
        assert!(first_inserted);
        assert!(!second_inserted);
        assert_eq!(first.id, second.id);
        assert_eq!(first.result, result);
        assert!(
            sqlx::query("DELETE FROM amazon_ai_strategy_assessments WHERE id = $1")
                .bind(first.id)
                .execute(&pool)
                .await
                .is_err()
        );
        let audit: Value = sqlx::query_scalar(
            "SELECT details FROM administrative_audit_log
             WHERE action = 'amazon.ai_strategy_assessed' AND target_id = $1",
        )
        .bind(analysis_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        let audit_text = audit.to_string();
        assert!(!audit_text.contains("executive_summary"));
        assert!(!audit_text.contains("Synthetic aggregate assessment"));
    }
}
