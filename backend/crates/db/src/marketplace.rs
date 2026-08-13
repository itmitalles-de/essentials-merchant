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
        parser_version: Some("sales-traffic-json-v1"),
        supported_options: &[],
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
    pub seller_id: String,
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
        Self {
            id: connection.id,
            seller_id: connection.seller_id,
            region: connection.region,
            granted_roles: connection.granted_roles,
            marketplace_ids,
            mode: connection.mode,
            enabled: connection.enabled,
            credential_configured: !connection.secret_ref.trim().is_empty(),
            created_at: connection.created_at,
            updated_at: connection.updated_at,
        }
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
pub struct AmazonReportDocumentInfo {
    pub id: Uuid,
    pub run_id: Uuid,
    pub amazon_report_document_id: String,
    pub sha256: String,
    pub content_type: Option<String>,
    pub compression_algorithm: Option<String>,
    pub downloaded_at: DateTime<Utc>,
    pub parser_version: Option<String>,
    pub import_status: String,
    pub import_error: Option<String>,
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
    if input.marketplace_ids.iter().any(|id| id.trim().is_empty()) {
        return Err("marketplace identifiers cannot be empty".into());
    }
    Ok(())
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
    content: &[u8],
) -> Result<(), sqlx::Error> {
    let checksum = sha256(content);
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
                     (run_id, amazon_report_document_id, sha256, content_type, compression_algorithm, raw_content)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(run_id)
            .bind(document_id)
            .bind(&checksum)
            .bind(content_type)
            .bind(compression_algorithm)
            .bind(content)
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

pub async fn load_document_for_parsing(
    pool: &PgPool,
    run_id: Uuid,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT amazon_report_document_id, raw_content
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
           AND comparability_key = $4 AND id <> $5 AND created_at < $6
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(snapshot.connection_id)
    .bind(&snapshot.marketplace_id)
    .bind(&snapshot.report_type)
    .bind(&snapshot.comparability_key)
    .bind(snapshot.id)
    .bind(snapshot.created_at)
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
        "SELECT id, run_id, amazon_report_document_id, sha256, content_type, compression_algorithm,
                downloaded_at, parser_version, import_status, import_error
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
    Ok(Some(AmazonRunDetail {
        run,
        events,
        document,
        snapshot,
        metrics,
        analyses,
    }))
}

pub async fn raw_document(
    pool: &PgPool,
    run_id: Uuid,
) -> Result<Option<RawReportDocument>, sqlx::Error> {
    sqlx::query_as::<_, (Option<String>, Vec<u8>)>(
        "SELECT content_type, raw_content FROM amazon_report_documents WHERE run_id = $1",
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

pub async fn overview(pool: &PgPool) -> Result<MarketplaceOverview, sqlx::Error> {
    let connections = sqlx::query_as::<_, AmazonConnection>(
        "SELECT id, seller_id, region, secret_ref, granted_roles, mode, enabled, created_at, updated_at
         FROM amazon_connections ORDER BY created_at DESC",
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
        return false;
    };
    definition.regions.contains(&connection.region.as_str())
        && definition
            .required_roles
            .iter()
            .any(|required| connection.granted_roles.iter().any(|role| role == required))
}

pub fn report_options_are_supported(report_type: &str, options: &Value) -> bool {
    let Some(definition) = report_definition(report_type) else {
        return false;
    };
    let Some(object) = options.as_object() else {
        return false;
    };
    object
        .keys()
        .all(|option| definition.supported_options.contains(&option.as_str()))
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
        assert!(!report_options_are_supported(
            SALES_AND_TRAFFIC,
            &json!({ "unsafe": true })
        ));
    }
}
