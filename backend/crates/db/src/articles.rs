use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Article {
    pub id: Uuid,
    pub sku: String,
    pub name: String,
    pub unit: String,
    pub sales_price_net: Decimal,
    pub default_vat_rate_code: String,
    pub purchase_price_net: Option<Decimal>,
    pub stock_quantity: Decimal,
    pub min_stock_quantity: Option<Decimal>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ArticleInput {
    pub sku: String,
    pub name: String,
    pub unit: String,
    pub sales_price_net: Decimal,
    pub default_vat_rate_code: String,
    pub purchase_price_net: Option<Decimal>,
    pub min_stock_quantity: Option<Decimal>,
    pub active: bool,
}

pub async fn list(pool: &PgPool) -> Result<Vec<Article>, sqlx::Error> {
    sqlx::query_as!(Article, "SELECT * FROM articles ORDER BY sku")
        .fetch_all(pool)
        .await
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<Article>, sqlx::Error> {
    sqlx::query_as!(Article, "SELECT * FROM articles WHERE id = $1", id)
        .fetch_optional(pool)
        .await
}

pub async fn create(pool: &PgPool, input: &ArticleInput) -> Result<Article, sqlx::Error> {
    sqlx::query_as!(
        Article,
        "INSERT INTO articles (
            sku, name, unit, sales_price_net, default_vat_rate_code,
            purchase_price_net, min_stock_quantity, active
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING *",
        input.sku,
        input.name,
        input.unit,
        input.sales_price_net,
        input.default_vat_rate_code,
        input.purchase_price_net,
        input.min_stock_quantity,
        input.active,
    )
    .fetch_one(pool)
    .await
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    input: &ArticleInput,
) -> Result<Option<Article>, sqlx::Error> {
    sqlx::query_as!(
        Article,
        "UPDATE articles SET
            sku = $2, name = $3, unit = $4, sales_price_net = $5, default_vat_rate_code = $6,
            purchase_price_net = $7, min_stock_quantity = $8, active = $9
         WHERE id = $1
         RETURNING *",
        id,
        input.sku,
        input.name,
        input.unit,
        input.sales_price_net,
        input.default_vat_rate_code,
        input.purchase_price_net,
        input.min_stock_quantity,
        input.active,
    )
    .fetch_optional(pool)
    .await
}

#[derive(Debug, thiserror::Error)]
pub enum DeleteError {
    #[error("article not found")]
    NotFound,
    #[error("article is referenced by existing invoices or stock movements")]
    InUse,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), DeleteError> {
    let result = sqlx::query!("DELETE FROM articles WHERE id = $1", id)
        .execute(pool)
        .await;
    match result {
        Ok(r) if r.rows_affected() > 0 => Ok(()),
        Ok(_) => Err(DeleteError::NotFound),
        Err(sqlx::Error::Database(db_err)) if db_err.is_foreign_key_violation() => {
            Err(DeleteError::InUse)
        }
        Err(err) => Err(DeleteError::Sqlx(err)),
    }
}
