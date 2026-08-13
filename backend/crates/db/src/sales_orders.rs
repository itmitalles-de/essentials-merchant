use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SalesOrder {
    pub id: Uuid,
    pub order_number: i64,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub source: String,
    pub external_order_id: Option<String>,
    pub external_status: Option<String>,
    pub status: String,
    pub shipping_carrier: Option<String>,
    pub tracking_number: String,
    pub notes: String,
    pub fulfilled_at: Option<DateTime<Utc>>,
    pub stock_booked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SalesOrderItem {
    pub id: Uuid,
    pub sales_order_id: Uuid,
    pub position: i32,
    pub article_id: Option<Uuid>,
    pub external_line_id: Option<String>,
    pub description: String,
    pub quantity: Decimal,
    pub unit: String,
    pub unit_price_net: Decimal,
    pub vat_rate_percent: Decimal,
    pub gross_amount: Decimal,
}

#[derive(Debug, Serialize)]
pub struct SalesOrderWithItems {
    #[serde(flatten)]
    pub order: SalesOrder,
    pub items: Vec<SalesOrderItem>,
}

#[derive(Debug, Deserialize)]
pub struct SalesOrderItemInput {
    pub article_id: Option<Uuid>,
    pub description: String,
    pub quantity: Decimal,
    pub unit: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSalesOrderInput {
    pub customer_id: Uuid,
    pub source: String,
    pub external_order_id: Option<String>,
    pub shipping_carrier: Option<String>,
    pub tracking_number: String,
    pub notes: String,
    pub items: Vec<SalesOrderItemInput>,
}

#[derive(Debug, Deserialize)]
pub struct FulfillSalesOrderInput {
    pub shipping_carrier: Option<String>,
    pub tracking_number: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FulfillError {
    #[error("sales order not found")]
    NotFound,
    #[error("sales order cannot be fulfilled from status {status}")]
    InvalidStatus { status: String },
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub async fn list(pool: &PgPool) -> Result<Vec<SalesOrder>, sqlx::Error> {
    sqlx::query_as!(
        SalesOrder,
        r#"SELECT o.id, o.order_number, o.customer_id, c.name AS customer_name, o.source,
                  o.external_order_id, o.external_status, o.status, o.shipping_carrier,
                  o.tracking_number, o.notes, o.fulfilled_at, o.stock_booked_at, o.created_at
           FROM sales_orders o JOIN customers c ON c.id = o.customer_id
           ORDER BY o.created_at DESC"#
    )
    .fetch_all(pool)
    .await
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<SalesOrderWithItems>, sqlx::Error> {
    let order = sqlx::query_as!(
        SalesOrder,
        r#"SELECT o.id, o.order_number, o.customer_id, c.name AS customer_name, o.source,
                  o.external_order_id, o.external_status, o.status, o.shipping_carrier,
                  o.tracking_number, o.notes, o.fulfilled_at, o.stock_booked_at, o.created_at
           FROM sales_orders o JOIN customers c ON c.id = o.customer_id WHERE o.id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    let Some(order) = order else {
        return Ok(None);
    };
    let items = sqlx::query_as!(
        SalesOrderItem,
        "SELECT * FROM sales_order_items WHERE sales_order_id = $1 ORDER BY position",
        id
    )
    .fetch_all(pool)
    .await?;
    Ok(Some(SalesOrderWithItems { order, items }))
}

pub async fn create(
    pool: &PgPool,
    input: &CreateSalesOrderInput,
) -> Result<SalesOrderWithItems, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let order = sqlx::query_as!(
        SalesOrder,
        r#"INSERT INTO sales_orders (customer_id, source, external_order_id, shipping_carrier, tracking_number, notes)
           SELECT $1, $2, $3, $4, $5, $6
           FROM customers WHERE id = $1
           RETURNING id, order_number, customer_id,
             (SELECT name FROM customers WHERE id = $1) AS "customer_name!", source, external_order_id,
             external_status, status, shipping_carrier, tracking_number, notes, fulfilled_at,
             stock_booked_at, created_at"#,
        input.customer_id, input.source, input.external_order_id, input.shipping_carrier,
        input.tracking_number, input.notes
    )
    .fetch_one(&mut *tx)
    .await?;

    let mut items = Vec::with_capacity(input.items.len());
    for (index, item) in input.items.iter().enumerate() {
        items.push(sqlx::query_as!(
            SalesOrderItem,
            "INSERT INTO sales_order_items (sales_order_id, position, article_id, description, quantity, unit) VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
            order.id, (index + 1) as i32, item.article_id, item.description, item.quantity, item.unit
        )
        .fetch_one(&mut *tx)
        .await?);
    }
    tx.commit().await?;
    Ok(SalesOrderWithItems { order, items })
}

/// Confirms a manual shipment. The order row is locked before stock movements
/// are written, so retrying a fulfillment request cannot book stock twice.
pub async fn fulfill(
    pool: &PgPool,
    id: Uuid,
    input: &FulfillSalesOrderInput,
) -> Result<SalesOrder, FulfillError> {
    let mut tx = pool.begin().await?;
    let order_state = sqlx::query_as::<_, (String, Option<DateTime<Utc>>, String, Option<String>)>(
        "SELECT status, stock_booked_at, source, external_order_id
         FROM sales_orders WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(FulfillError::NotFound)?;

    if order_state.0 != "open" {
        return Err(FulfillError::InvalidStatus {
            status: order_state.0,
        });
    }

    if order_state.1.is_none() {
        let stocked_items = sqlx::query_as::<_, (Uuid, Decimal)>(
            "SELECT article_id, SUM(quantity) FROM sales_order_items
             WHERE sales_order_id = $1 AND article_id IS NOT NULL
             GROUP BY article_id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;

        for (article_id, quantity) in stocked_items {
            sqlx::query(
                "INSERT INTO stock_movements (article_id, movement_type, quantity, reference_type, reference_id)
                 VALUES ($1, 'out', $2, 'sales_order', $3)",
            )
            .bind(article_id)
            .bind(-quantity.abs())
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
    }

    let order = sqlx::query_as::<_, SalesOrder>(
        "UPDATE sales_orders
         SET status = 'fulfilled', shipping_carrier = $2, tracking_number = $3,
             fulfilled_at = now(), stock_booked_at = COALESCE(stock_booked_at, now())
         WHERE id = $1
         RETURNING id, order_number, customer_id,
             (SELECT name FROM customers WHERE id = sales_orders.customer_id) AS customer_name,
             source, external_order_id, external_status, status, shipping_carrier, tracking_number,
             notes, fulfilled_at, stock_booked_at, created_at",
    )
    .bind(id)
    .bind(&input.shipping_carrier)
    .bind(&input.tracking_number)
    .fetch_one(&mut *tx)
    .await?;

    if order_state.2 == "vendure" {
        if let Some(external_order_id) = order_state.3 {
            let event_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO integration_outbox
                     (id, event_type, aggregate_type, aggregate_id, idempotency_key, payload)
                 VALUES ($1, 'vendure.fulfillment.project', 'sales_order', $2, $3, $4)",
            )
            .bind(event_id)
            .bind(id)
            .bind(format!("fulfillment:{id}:{event_id}"))
            .bind(serde_json::json!({
                "core_order_id": id,
                "vendure_order_id": external_order_id,
                "carrier": input.shipping_carrier,
                "tracking_number": input.tracking_number,
            }))
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(order)
}
