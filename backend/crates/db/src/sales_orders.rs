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
    pub status: String,
    pub shipping_carrier: Option<String>,
    pub tracking_number: String,
    pub notes: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SalesOrderItem {
    pub id: Uuid,
    pub sales_order_id: Uuid,
    pub position: i32,
    pub article_id: Option<Uuid>,
    pub description: String,
    pub quantity: Decimal,
    pub unit: String,
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

pub async fn list(pool: &PgPool) -> Result<Vec<SalesOrder>, sqlx::Error> {
    sqlx::query_as!(
        SalesOrder,
        r#"SELECT o.id, o.order_number, o.customer_id, c.name AS customer_name, o.source,
                  o.external_order_id, o.status, o.shipping_carrier, o.tracking_number,
                  o.notes, o.created_at
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
                  o.external_order_id, o.status, o.shipping_carrier, o.tracking_number,
                  o.notes, o.created_at
           FROM sales_orders o JOIN customers c ON c.id = o.customer_id WHERE o.id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    let Some(order) = order else { return Ok(None); };
    let items = sqlx::query_as!(SalesOrderItem, "SELECT * FROM sales_order_items WHERE sales_order_id = $1 ORDER BY position", id)
        .fetch_all(pool)
        .await?;
    Ok(Some(SalesOrderWithItems { order, items }))
}

pub async fn create(pool: &PgPool, input: &CreateSalesOrderInput) -> Result<SalesOrderWithItems, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let order = sqlx::query_as!(
        SalesOrder,
        r#"INSERT INTO sales_orders (customer_id, source, external_order_id, shipping_carrier, tracking_number, notes)
           SELECT $1, $2, $3, $4, $5, $6
           FROM customers WHERE id = $1
           RETURNING id, order_number, customer_id,
             (SELECT name FROM customers WHERE id = $1) AS "customer_name!", source, external_order_id,
             status, shipping_carrier, tracking_number, notes, created_at"#,
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
