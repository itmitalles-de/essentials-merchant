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
    #[error("unknown Merchant SKU: {0}")]
    UnknownSku(String),
    #[error("idempotency record exists without its imported order")]
    InconsistentInbox,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub async fn claim_outbox(pool: &PgPool, limit: i64) -> Result<Vec<OutboxEvent>, sqlx::Error> {
    sqlx::query(
        "UPDATE integration_outbox
         SET status = 'pending', locked_at = NULL,
             last_error = COALESCE(last_error, 'worker lease expired')
         WHERE status = 'processing' AND locked_at < now() - interval '5 minutes'",
    )
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
    let attempts = sqlx::query_scalar::<_, i32>(
        "SELECT attempts FROM integration_outbox WHERE id = $1 AND status = 'processing'",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(attempts) = attempts else {
        return Ok(false);
    };
    let delay_seconds = 2_i32.pow(attempts.clamp(1, 10) as u32).min(3600);
    let status = if attempts >= 20 { "dead" } else { "pending" };
    let result = sqlx::query(
        "UPDATE integration_outbox
         SET status = $2, available_at = now() + make_interval(secs => $3),
             locked_at = NULL, last_error = $4
         WHERE id = $1 AND status = 'processing'",
    )
    .bind(id)
    .bind(status)
    .bind(f64::from(delay_seconds))
    .bind(error)
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
}
