use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct OutboxEvent {
    pub id: Uuid,
    pub sequence: i64,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub idempotency_key: String,
    pub payload: Value,
    pub attempts: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct OutboxPolicy {
    pub lease_seconds: i32,
    pub retry_base_seconds: i32,
    pub retry_max_seconds: i32,
    pub max_attempts: i32,
}

impl Default for OutboxPolicy {
    fn default() -> Self {
        Self {
            lease_seconds: 300,
            retry_base_seconds: 2,
            retry_max_seconds: 3_600,
            max_attempts: 20,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VendureCustomer {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    #[serde(default)]
    pub phone: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VendureAddress {
    #[serde(default)]
    pub street_line1: String,
    #[serde(default)]
    pub street_line2: String,
    #[serde(default)]
    pub postal_code: String,
    #[serde(default)]
    pub city: String,
    #[serde(default = "default_country")]
    pub country_code: String,
}

fn default_country() -> String {
    "DE".to_owned()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VendureOrderLine {
    pub id: String,
    pub sku: String,
    pub description: String,
    pub quantity: Decimal,
    pub unit_price_gross_cents: i64,
    pub vat_rate_percent: Decimal,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VendureOrderEvent {
    pub event_id: String,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub order_id: String,
    pub order_code: String,
    pub order_state: String,
    pub currency_code: String,
    pub customer: VendureCustomer,
    pub shipping_address: VendureAddress,
    pub lines: Vec<VendureOrderLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub sales_order_id: Uuid,
    pub duplicate: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("Vendure order {0} contains no lines")]
    EmptyOrder(String),
    #[error("unknown Essentials+ Merchant SKU: {0}")]
    UnknownSku(String),
    #[error("idempotency record exists without its imported order")]
    InconsistentInbox,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub async fn claim_outbox(pool: &PgPool, limit: i64) -> Result<Vec<OutboxEvent>, sqlx::Error> {
    claim_outbox_with_policy(pool, limit, OutboxPolicy::default()).await
}

pub async fn claim_outbox_with_policy(
    pool: &PgPool,
    limit: i64,
    policy: OutboxPolicy,
) -> Result<Vec<OutboxEvent>, sqlx::Error> {
    sqlx::query(
        "UPDATE integration_outbox
         SET status = 'pending', locked_at = NULL,
             last_error = COALESCE(last_error, 'worker lease expired')
         WHERE status = 'processing'
           AND locked_at < now() - make_interval(secs => $1)",
    )
    .bind(f64::from(policy.lease_seconds.clamp(1, 86_400)))
    .execute(pool)
    .await?;

    sqlx::query_as::<_, OutboxEvent>(
        "WITH selected AS (
             SELECT id FROM integration_outbox
             WHERE status = 'pending' AND available_at <= now()
             ORDER BY created_at
             FOR UPDATE SKIP LOCKED
             LIMIT $1
         )
         UPDATE integration_outbox event
         SET status = 'processing', locked_at = now(), attempts = event.attempts + 1
         FROM selected
         WHERE event.id = selected.id
         RETURNING event.id, event.sequence, event.event_type, event.aggregate_type, event.aggregate_id,
                   event.idempotency_key, event.payload, event.attempts, event.created_at",
    )
    .bind(limit.clamp(1, 100))
    .fetch_all(pool)
    .await
}

pub async fn acknowledge_outbox(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE integration_outbox
         SET status = 'delivered', delivered_at = now(), locked_at = NULL, last_error = NULL
         WHERE id = $1 AND status = 'processing'",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn retry_outbox(pool: &PgPool, id: Uuid, error: &str) -> Result<bool, sqlx::Error> {
    retry_outbox_with_policy(pool, id, error, OutboxPolicy::default()).await
}

pub async fn retry_outbox_with_policy(
    pool: &PgPool,
    id: Uuid,
    error: &str,
    policy: OutboxPolicy,
) -> Result<bool, sqlx::Error> {
    let attempts = sqlx::query_scalar::<_, i32>(
        "SELECT attempts FROM integration_outbox WHERE id = $1 AND status = 'processing'",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(attempts) = attempts else {
        return Ok(false);
    };
    let exponent = attempts.saturating_sub(1).clamp(0, 20) as u32;
    let delay_seconds = policy
        .retry_base_seconds
        .clamp(1, 3_600)
        .saturating_mul(2_i32.saturating_pow(exponent))
        .min(policy.retry_max_seconds.clamp(1, 86_400));
    let status = if attempts >= policy.max_attempts.clamp(1, 100) {
        "dead"
    } else {
        "pending"
    };
    let safe_error = sanitize_integration_error(error);
    let result = sqlx::query(
        "UPDATE integration_outbox
         SET status = $2, available_at = now() + make_interval(secs => $3),
             locked_at = NULL, last_error = $4
         WHERE id = $1 AND status = 'processing'",
    )
    .bind(id)
    .bind(status)
    .bind(f64::from(delay_seconds))
    .bind(safe_error)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub fn sanitize_integration_error(error: &str) -> String {
    let normalized = error
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    normalized.chars().take(512).collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueSummary {
    pub pending: i64,
    pub processing: i64,
    pub delivered: i64,
    pub dead: i64,
    pub oldest_open_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboxSummary {
    pub completed: i64,
    pub failed: i64,
    pub last_processed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DiagnosticEvent {
    pub source: String,
    pub event_id: String,
    pub event_type: String,
    pub status: String,
    pub attempts: i32,
    pub available_at: Option<DateTime<Utc>>,
    pub locked_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MappingSummary {
    pub entity_type: String,
    pub count: i64,
    pub last_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: Uuid,
    pub actor_user_id: Uuid,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub idempotency_key: String,
    pub details: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationDiagnostics {
    pub core_outbox: QueueSummary,
    pub core_inbox: InboxSummary,
    pub vendure_outbox: QueueSummary,
    pub events: Vec<DiagnosticEvent>,
    pub mappings: Vec<MappingSummary>,
    pub audit: Vec<AuditEntry>,
    pub core_database_ready: bool,
    pub vendure_health: String,
    pub vendure_observed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteDiagnosticEvent {
    pub event_id: String,
    pub event_type: String,
    pub status: String,
    pub attempts: i32,
    pub available_at: Option<DateTime<Utc>>,
    pub locked_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteDiagnosticsReport {
    pub health_status: String,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub observed_at: DateTime<Utc>,
    #[serde(default)]
    pub events: Vec<RemoteDiagnosticEvent>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct IntegrationAdminCommand {
    pub id: Uuid,
    pub provider: String,
    pub action: String,
    pub target_id: String,
    pub attempts: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequeueResult {
    pub accepted: bool,
    pub duplicate: bool,
    pub command_id: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct QueueCountsRow {
    pending: i64,
    processing: i64,
    delivered: i64,
    dead: i64,
    oldest_open_at: Option<DateTime<Utc>>,
    last_success_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct RemoteStatusRow {
    health_status: String,
    last_success_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
    observed_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum RequeueError {
    #[error("event not found")]
    NotFound,
    #[error("only dead events can be manually requeued")]
    NotDead,
    #[error("unsupported integration source")]
    UnsupportedSource,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub async fn integration_diagnostics(pool: &PgPool) -> Result<IntegrationDiagnostics, sqlx::Error> {
    let core_counts = sqlx::query_as::<_, QueueCountsRow>(
        "SELECT
                count(*) FILTER (WHERE status = 'pending') AS pending,
                count(*) FILTER (WHERE status = 'processing') AS processing,
                count(*) FILTER (WHERE status = 'delivered') AS delivered,
                count(*) FILTER (WHERE status = 'dead') AS dead,
                min(created_at) FILTER (WHERE status IN ('pending', 'processing')) AS oldest_open_at,
                max(delivered_at) AS last_success_at
             FROM integration_outbox",
    )
    .fetch_one(pool)
    .await?;
    let core_last_error: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT last_error FROM integration_outbox
         WHERE last_error IS NOT NULL ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .map(|error| sanitize_integration_error(&error));

    let inbox: (i64, i64, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE status = 'completed'),
                count(*) FILTER (WHERE status = 'failed'), max(processed_at)
         FROM integration_inbox",
    )
    .fetch_one(pool)
    .await?;

    let remote_counts = sqlx::query_as::<_, QueueCountsRow>(
        "SELECT
                count(*) FILTER (WHERE status = 'pending') AS pending,
                count(*) FILTER (WHERE status = 'processing') AS processing,
                count(*) FILTER (WHERE status = 'delivered') AS delivered,
                count(*) FILTER (WHERE status = 'dead') AS dead,
                min(created_at) FILTER (WHERE status IN ('pending', 'processing')) AS oldest_open_at,
                max(delivered_at) AS last_success_at
             FROM integration_remote_events WHERE provider = 'vendure'",
    )
    .fetch_one(pool)
    .await?;
    let remote_status = sqlx::query_as::<_, RemoteStatusRow>(
        "SELECT health_status, last_success_at, last_error, observed_at
             FROM integration_remote_status WHERE provider = 'vendure'",
    )
    .fetch_optional(pool)
    .await?;

    let mut events = sqlx::query_as::<_, DiagnosticEvent>(
        "SELECT 'core'::text AS source, id::text AS event_id, event_type, status, attempts,
                available_at, locked_at, last_error, created_at, delivered_at
         FROM integration_outbox ORDER BY created_at DESC LIMIT 50",
    )
    .fetch_all(pool)
    .await?;
    events.extend(
        sqlx::query_as::<_, DiagnosticEvent>(
            "SELECT provider AS source, external_event_id AS event_id, event_type, status,
                    attempts, available_at, locked_at, last_error, created_at, delivered_at
             FROM integration_remote_events WHERE provider = 'vendure'
             ORDER BY created_at DESC LIMIT 50",
        )
        .fetch_all(pool)
        .await?,
    );
    for event in &mut events {
        event.last_error = event.last_error.as_deref().map(sanitize_integration_error);
    }
    events.sort_by_key(|event| std::cmp::Reverse(event.created_at));
    events.truncate(75);

    let mappings = sqlx::query_as::<_, MappingSummary>(
        "SELECT entity_type, count(*) AS count, max(updated_at) AS last_updated_at
         FROM external_entity_mappings WHERE provider = 'vendure'
         GROUP BY entity_type ORDER BY entity_type",
    )
    .fetch_all(pool)
    .await?;
    let audit = sqlx::query_as::<_, AuditEntry>(
        "SELECT id, actor_user_id, action, target_type, target_id, idempotency_key,
                details, created_at
         FROM administrative_audit_log ORDER BY created_at DESC LIMIT 50",
    )
    .fetch_all(pool)
    .await?;

    Ok(IntegrationDiagnostics {
        core_outbox: QueueSummary {
            pending: core_counts.pending,
            processing: core_counts.processing,
            delivered: core_counts.delivered,
            dead: core_counts.dead,
            oldest_open_at: core_counts.oldest_open_at,
            last_success_at: core_counts.last_success_at,
            last_error: core_last_error,
        },
        core_inbox: InboxSummary {
            completed: inbox.0,
            failed: inbox.1,
            last_processed_at: inbox.2,
        },
        vendure_outbox: QueueSummary {
            pending: remote_counts.pending,
            processing: remote_counts.processing,
            delivered: remote_counts.delivered,
            dead: remote_counts.dead,
            oldest_open_at: remote_counts.oldest_open_at,
            last_success_at: remote_status
                .as_ref()
                .and_then(|status| status.last_success_at)
                .or(remote_counts.last_success_at),
            last_error: remote_status
                .as_ref()
                .and_then(|status| status.last_error.as_deref())
                .map(sanitize_integration_error),
        },
        events,
        mappings,
        audit,
        core_database_ready: true,
        vendure_health: remote_status
            .as_ref()
            .map(|status| status.health_status.clone())
            .unwrap_or_else(|| "unknown".into()),
        vendure_observed_at: remote_status.map(|status| status.observed_at),
    })
}

pub async fn record_remote_diagnostics(
    pool: &PgPool,
    provider: &str,
    report: &RemoteDiagnosticsReport,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let health = match report.health_status.as_str() {
        "healthy" | "degraded" | "failed" => report.health_status.as_str(),
        _ => "degraded",
    };
    sqlx::query(
        "INSERT INTO integration_remote_status
             (provider, health_status, last_success_at, last_error, observed_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (provider) DO UPDATE SET
             health_status = EXCLUDED.health_status,
             last_success_at = EXCLUDED.last_success_at,
             last_error = EXCLUDED.last_error,
             observed_at = EXCLUDED.observed_at,
             updated_at = now()",
    )
    .bind(provider)
    .bind(health)
    .bind(report.last_success_at)
    .bind(report.last_error.as_deref().map(sanitize_integration_error))
    .bind(report.observed_at)
    .execute(&mut *tx)
    .await?;
    for event in report.events.iter().take(100) {
        sqlx::query(
            "INSERT INTO integration_remote_events
                 (provider, external_event_id, event_type, status, attempts, available_at,
                  locked_at, last_error, created_at, delivered_at, observed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (provider, external_event_id) DO UPDATE SET
                 event_type = EXCLUDED.event_type, status = EXCLUDED.status,
                 attempts = EXCLUDED.attempts, available_at = EXCLUDED.available_at,
                 locked_at = EXCLUDED.locked_at, last_error = EXCLUDED.last_error,
                 delivered_at = EXCLUDED.delivered_at, observed_at = EXCLUDED.observed_at",
        )
        .bind(provider)
        .bind(&event.event_id)
        .bind(&event.event_type)
        .bind(&event.status)
        .bind(event.attempts)
        .bind(event.available_at)
        .bind(event.locked_at)
        .bind(event.last_error.as_deref().map(sanitize_integration_error))
        .bind(event.created_at)
        .bind(event.delivered_at)
        .bind(report.observed_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

pub async fn manually_requeue(
    pool: &PgPool,
    actor_user_id: Uuid,
    source: &str,
    event_id: &str,
    idempotency_key: &str,
) -> Result<RequeueResult, RequeueError> {
    if idempotency_key.trim().is_empty() || idempotency_key.len() > 128 {
        return Err(RequeueError::UnsupportedSource);
    }
    let mut tx = pool.begin().await?;
    let audit_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO administrative_audit_log
             (actor_user_id, action, target_type, target_id, idempotency_key, details)
         VALUES ($1, 'integration.requeue', $2, $3, $4, '{\"outcome\":\"requested\"}'::jsonb)
         ON CONFLICT (action, idempotency_key) DO NOTHING
         RETURNING id",
    )
    .bind(actor_user_id)
    .bind(format!("{source}.outbox"))
    .bind(event_id)
    .bind(idempotency_key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(audit_id) = audit_id else {
        tx.rollback().await?;
        return Ok(RequeueResult {
            accepted: true,
            duplicate: true,
            command_id: None,
        });
    };

    let result = match source {
        "core" => {
            let parsed_id = Uuid::parse_str(event_id).map_err(|_| RequeueError::NotFound)?;
            let status = sqlx::query_scalar::<_, String>(
                "SELECT status FROM integration_outbox WHERE id = $1 FOR UPDATE",
            )
            .bind(parsed_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(RequeueError::NotFound)?;
            if status != "dead" {
                return Err(RequeueError::NotDead);
            }
            sqlx::query(
                "UPDATE integration_outbox SET status = 'pending', available_at = now(),
                    locked_at = NULL, last_error = 'manually requeued',
                    requeue_count = requeue_count + 1, requeued_at = now()
                 WHERE id = $1",
            )
            .bind(parsed_id)
            .execute(&mut *tx)
            .await?;
            RequeueResult {
                accepted: true,
                duplicate: false,
                command_id: None,
            }
        }
        "vendure" => {
            let status = sqlx::query_scalar::<_, String>(
                "SELECT status FROM integration_remote_events
                 WHERE provider = 'vendure' AND external_event_id = $1",
            )
            .bind(event_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(RequeueError::NotFound)?;
            if status != "dead" {
                return Err(RequeueError::NotDead);
            }
            let command_id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO integration_admin_commands
                     (provider, action, target_id, idempotency_key, actor_user_id)
                 VALUES ('vendure', 'requeue', $1, $2, $3) RETURNING id",
            )
            .bind(event_id)
            .bind(idempotency_key)
            .bind(actor_user_id)
            .fetch_one(&mut *tx)
            .await?;
            RequeueResult {
                accepted: true,
                duplicate: false,
                command_id: Some(command_id),
            }
        }
        _ => return Err(RequeueError::UnsupportedSource),
    };
    let audit_details = serde_json::json!({
        "outcome": "accepted",
        "command_id": result.command_id,
    });
    sqlx::query("UPDATE administrative_audit_log SET details = $2 WHERE id = $1")
        .bind(audit_id)
        .bind(audit_details)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn claim_admin_commands(
    pool: &PgPool,
    provider: &str,
    limit: i64,
    lease_seconds: i32,
) -> Result<Vec<IntegrationAdminCommand>, sqlx::Error> {
    sqlx::query(
        "UPDATE integration_admin_commands SET status = 'pending', locked_at = NULL,
             last_error = COALESCE(last_error, 'worker lease expired')
         WHERE provider = $1 AND status = 'processing'
           AND locked_at < now() - make_interval(secs => $2)",
    )
    .bind(provider)
    .bind(f64::from(lease_seconds.clamp(1, 86_400)))
    .execute(pool)
    .await?;
    sqlx::query_as::<_, IntegrationAdminCommand>(
        "WITH selected AS (
             SELECT id FROM integration_admin_commands
             WHERE provider = $1 AND status = 'pending'
             ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT $2
         )
         UPDATE integration_admin_commands command
         SET status = 'processing', locked_at = now(), attempts = command.attempts + 1
         FROM selected WHERE command.id = selected.id
         RETURNING command.id, command.provider, command.action, command.target_id,
                   command.attempts",
    )
    .bind(provider)
    .bind(limit.clamp(1, 20))
    .fetch_all(pool)
    .await
}

pub async fn complete_admin_command(
    pool: &PgPool,
    command_id: Uuid,
    error: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let safe_error = error.map(sanitize_integration_error);
    let result = sqlx::query(
        "UPDATE integration_admin_commands SET status = $2, locked_at = NULL,
                last_error = $3, completed_at = now()
         WHERE id = $1 AND status = 'processing'",
    )
    .bind(command_id)
    .bind(if error.is_some() {
        "failed"
    } else {
        "completed"
    })
    .bind(safe_error)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn record_mapping(
    pool: &PgPool,
    entity_type: &str,
    internal_id: Uuid,
    external_id: &str,
    metadata: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO external_entity_mappings
             (provider, entity_type, internal_id, external_id, metadata)
         VALUES ('vendure', $1, $2, $3, $4)
         ON CONFLICT (provider, entity_type, internal_id) DO UPDATE
         SET external_id = EXCLUDED.external_id, metadata = EXCLUDED.metadata, updated_at = now()",
    )
    .bind(entity_type)
    .bind(internal_id)
    .bind(external_id)
    .bind(metadata)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn import_vendure_order(
    pool: &PgPool,
    event: &VendureOrderEvent,
) -> Result<ImportResult, ImportError> {
    if event.lines.is_empty() {
        return Err(ImportError::EmptyOrder(event.order_code.clone()));
    }

    let mut tx = pool.begin().await?;
    let payload = serde_json::to_value(event).expect("VendureOrderEvent is serializable");
    let inbox_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO integration_inbox
             (source, event_id, event_type, payload, status, processed_at)
         VALUES ('vendure', $1, $2, $3, 'completed', now())
         ON CONFLICT (source, event_id) DO NOTHING
         RETURNING id",
    )
    .bind(&event.event_id)
    .bind(&event.event_type)
    .bind(payload)
    .fetch_optional(&mut *tx)
    .await?;

    if inbox_id.is_none() {
        let existing = find_order_by_external_id(&mut tx, &event.order_id).await?;
        tx.commit().await?;
        return existing
            .map(|sales_order_id| ImportResult {
                sales_order_id,
                duplicate: true,
            })
            .ok_or(ImportError::InconsistentInbox);
    }

    if let Some(sales_order_id) = find_order_by_external_id(&mut tx, &event.order_id).await? {
        tx.commit().await?;
        return Ok(ImportResult {
            sales_order_id,
            duplicate: true,
        });
    }

    let customer_id = find_or_create_customer(&mut tx, event).await?;
    let sales_order_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO sales_orders
             (customer_id, source, external_order_id, external_status, notes)
         VALUES ($1, 'vendure', $2, $3, $4)
         RETURNING id",
    )
    .bind(customer_id)
    .bind(&event.order_id)
    .bind(&event.order_state)
    .bind(format!(
        "Vendure order {} ({})",
        event.order_code, event.currency_code
    ))
    .fetch_one(&mut *tx)
    .await?;

    let mut stock_by_article = HashMap::<Uuid, Decimal>::new();
    for (index, line) in event.lines.iter().enumerate() {
        let article = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, unit FROM articles WHERE sku = $1 AND active = true FOR UPDATE",
        )
        .bind(&line.sku)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ImportError::UnknownSku(line.sku.clone()))?;

        let unit_price_gross =
            Decimal::from_i128_with_scale(line.unit_price_gross_cents as i128, 2);
        let divisor = Decimal::ONE + line.vat_rate_percent / Decimal::from(100);
        let unit_price_net = (unit_price_gross / divisor).round_dp(2);
        let gross_amount = (unit_price_gross * line.quantity).round_dp(2);

        sqlx::query(
            "INSERT INTO sales_order_items
                 (sales_order_id, position, article_id, external_line_id, description, quantity,
                  unit, unit_price_net, vat_rate_percent, gross_amount)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(sales_order_id)
        .bind((index + 1) as i32)
        .bind(article.0)
        .bind(&line.id)
        .bind(&line.description)
        .bind(line.quantity)
        .bind(article.1)
        .bind(unit_price_net)
        .bind(line.vat_rate_percent)
        .bind(gross_amount)
        .execute(&mut *tx)
        .await?;

        *stock_by_article.entry(article.0).or_default() += line.quantity;
    }

    for (article_id, quantity) in stock_by_article {
        sqlx::query(
            "INSERT INTO stock_movements
                 (article_id, movement_type, quantity, reference_type, reference_id, note)
             VALUES ($1, 'out', $2, 'sales_order', $3, $4)",
        )
        .bind(article_id)
        .bind(-quantity.abs())
        .bind(sales_order_id)
        .bind(format!("Vendure order {}", event.order_code))
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("UPDATE sales_orders SET stock_booked_at = now() WHERE id = $1")
        .bind(sales_order_id)
        .execute(&mut *tx)
        .await?;
    upsert_mapping(
        &mut tx,
        "sales_order",
        sales_order_id,
        &event.order_id,
        serde_json::json!({ "code": event.order_code }),
    )
    .await?;
    tx.commit().await?;

    Ok(ImportResult {
        sales_order_id,
        duplicate: false,
    })
}

async fn find_order_by_external_id(
    tx: &mut Transaction<'_, Postgres>,
    external_order_id: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM sales_orders WHERE source = 'vendure' AND external_order_id = $1",
    )
    .bind(external_order_id)
    .fetch_optional(&mut **tx)
    .await
}

async fn find_or_create_customer(
    tx: &mut Transaction<'_, Postgres>,
    event: &VendureOrderEvent,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&event.customer.id)
        .execute(&mut **tx)
        .await?;
    if let Some(customer_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT internal_id FROM external_entity_mappings
         WHERE provider = 'vendure' AND entity_type = 'customer' AND external_id = $1",
    )
    .bind(&event.customer.id)
    .fetch_optional(&mut **tx)
    .await?
    {
        return Ok(customer_id);
    }

    let customer_number = sqlx::query_scalar::<_, i32>(
        "UPDATE company_settings SET next_customer_number = next_customer_number + 1
         WHERE id = 1 RETURNING next_customer_number - 1",
    )
    .fetch_one(&mut **tx)
    .await?;
    let name = format!("{} {}", event.customer.first_name, event.customer.last_name)
        .trim()
        .to_owned();
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers
             (customer_number, name, address_line1, address_line2, zip, city, country,
              email, phone, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING id",
    )
    .bind(customer_number)
    .bind(if name.is_empty() {
        &event.customer.email
    } else {
        &name
    })
    .bind(&event.shipping_address.street_line1)
    .bind(&event.shipping_address.street_line2)
    .bind(&event.shipping_address.postal_code)
    .bind(&event.shipping_address.city)
    .bind(&event.shipping_address.country_code)
    .bind(&event.customer.email)
    .bind(&event.customer.phone)
    .bind("Imported from Vendure")
    .fetch_one(&mut **tx)
    .await?;
    upsert_mapping(
        tx,
        "customer",
        customer_id,
        &event.customer.id,
        serde_json::json!({ "email": event.customer.email }),
    )
    .await?;
    Ok(customer_id)
}

async fn upsert_mapping(
    tx: &mut Transaction<'_, Postgres>,
    entity_type: &str,
    internal_id: Uuid,
    external_id: &str,
    metadata: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO external_entity_mappings
             (provider, entity_type, internal_id, external_id, metadata)
         VALUES ('vendure', $1, $2, $3, $4)
         ON CONFLICT (provider, entity_type, internal_id) DO UPDATE
         SET external_id = EXCLUDED.external_id, metadata = EXCLUDED.metadata, updated_at = now()",
    )
    .bind(entity_type)
    .bind(internal_id)
    .bind(external_id)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use sqlx::PgPool;

    use super::*;

    async fn insert_article(pool: &PgPool) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO articles
                 (sku, name, sales_price_net, default_vat_rate_code, stock_quantity)
             VALUES ('TEST-001', 'Test product', 10.00, 'STANDARD', 10)
             RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn order_event(event_id: &str, state: &str) -> VendureOrderEvent {
        VendureOrderEvent {
            event_id: event_id.to_owned(),
            event_type: "vendure.order.payment".to_owned(),
            occurred_at: Utc::now(),
            order_id: "42".to_owned(),
            order_code: "V-TEST".to_owned(),
            order_state: state.to_owned(),
            currency_code: "EUR".to_owned(),
            customer: VendureCustomer {
                id: "customer-1".to_owned(),
                first_name: "Erika".to_owned(),
                last_name: "Musterfrau".to_owned(),
                email: "erika@example.test".to_owned(),
                phone: String::new(),
            },
            shipping_address: VendureAddress {
                street_line1: "Testweg 1".to_owned(),
                street_line2: String::new(),
                postal_code: "10115".to_owned(),
                city: "Berlin".to_owned(),
                country_code: "DE".to_owned(),
            },
            lines: vec![VendureOrderLine {
                id: "line-1".to_owned(),
                sku: "TEST-001".to_owned(),
                description: "Test product".to_owned(),
                quantity: Decimal::from(2),
                unit_price_gross_cents: 1190,
                vat_rate_percent: Decimal::from(19),
            }],
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn duplicate_and_late_payment_events_book_stock_once(pool: PgPool) {
        let article_id = insert_article(&pool).await;
        let first = import_vendure_order(&pool, &order_event("payment-1", "PaymentAuthorized"))
            .await
            .unwrap();
        let duplicate = import_vendure_order(&pool, &order_event("payment-1", "PaymentAuthorized"))
            .await
            .unwrap();
        let late = import_vendure_order(&pool, &order_event("payment-2", "PaymentSettled"))
            .await
            .unwrap();

        assert!(!first.duplicate);
        assert!(duplicate.duplicate);
        assert!(late.duplicate);
        assert_eq!(first.sales_order_id, late.sales_order_id);
        let stock: Decimal =
            sqlx::query_scalar("SELECT stock_quantity FROM articles WHERE id = $1")
                .bind(article_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let movements: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM stock_movements
             WHERE reference_type = 'sales_order' AND reference_id = $1",
        )
        .bind(first.sales_order_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stock, Decimal::from(8));
        assert_eq!(movements, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn expired_worker_lease_is_claimed_after_restart(pool: PgPool) {
        insert_article(&pool).await;
        let first = claim_outbox(&pool, 1).await.unwrap();
        assert_eq!(first.len(), 1);
        sqlx::query("UPDATE integration_outbox SET locked_at = $2 WHERE id = $1")
            .bind(first[0].id)
            .bind(Utc::now() - Duration::minutes(10))
            .execute(&pool)
            .await
            .unwrap();
        let reclaimed = claim_outbox(&pool, 1).await.unwrap();
        assert_eq!(reclaimed[0].id, first[0].id);
        assert_eq!(reclaimed[0].attempts, 2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn temporary_failure_schedules_retry(pool: PgPool) {
        insert_article(&pool).await;
        let event = claim_outbox(&pool, 1).await.unwrap().remove(0);
        assert!(retry_outbox(&pool, event.id, "connection refused")
            .await
            .unwrap());
        let state: (String, i32, String) = sqlx::query_as(
            "SELECT status, attempts, last_error FROM integration_outbox WHERE id = $1",
        )
        .bind(event.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state.0, "pending");
        assert_eq!(state.1, 1);
        assert_eq!(state.2, "connection refused");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn active_lease_is_not_reclaimed_and_retry_reaches_dead_state(pool: PgPool) {
        insert_article(&pool).await;
        let policy = OutboxPolicy {
            lease_seconds: 30,
            retry_base_seconds: 1,
            retry_max_seconds: 1,
            max_attempts: 2,
        };
        let first = claim_outbox_with_policy(&pool, 1, policy)
            .await
            .unwrap()
            .remove(0);
        assert!(claim_outbox_with_policy(&pool, 1, policy)
            .await
            .unwrap()
            .is_empty());
        retry_outbox_with_policy(&pool, first.id, "temporary\nsynthetic failure", policy)
            .await
            .unwrap();
        sqlx::query("UPDATE integration_outbox SET available_at = now() WHERE id = $1")
            .bind(first.id)
            .execute(&pool)
            .await
            .unwrap();
        let second = claim_outbox_with_policy(&pool, 1, policy)
            .await
            .unwrap()
            .remove(0);
        retry_outbox_with_policy(&pool, second.id, "terminal synthetic failure", policy)
            .await
            .unwrap();
        let state: (String, i32, Option<DateTime<Utc>>) = sqlx::query_as(
            "SELECT status, attempts, locked_at FROM integration_outbox WHERE id = $1",
        )
        .bind(first.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state.0, "dead");
        assert_eq!(state.1, 2);
        assert!(state.2.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn manual_requeue_is_idempotent_and_audited(pool: PgPool) {
        let article_id = insert_article(&pool).await;
        let event_id: Uuid = sqlx::query_scalar(
            "UPDATE integration_outbox SET status = 'dead' WHERE aggregate_id = $1 RETURNING id",
        )
        .bind(article_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (username, password_hash) VALUES ('synthetic-admin', 'not-a-secret') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let first = manually_requeue(
            &pool,
            user_id,
            "core",
            &event_id.to_string(),
            "synthetic-requeue-1",
        )
        .await
        .unwrap();
        let duplicate = manually_requeue(
            &pool,
            user_id,
            "core",
            &event_id.to_string(),
            "synthetic-requeue-1",
        )
        .await
        .unwrap();
        assert!(first.accepted && !first.duplicate);
        assert!(duplicate.accepted && duplicate.duplicate);
        let state: (String, i32) =
            sqlx::query_as("SELECT status, requeue_count FROM integration_outbox WHERE id = $1")
                .bind(event_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(state, ("pending".into(), 1));
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM administrative_audit_log WHERE idempotency_key = 'synthetic-requeue-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit_count, 1);
    }
}
