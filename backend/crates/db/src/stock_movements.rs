use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StockMovement {
    pub id: Uuid,
    pub article_id: Uuid,
    pub movement_type: String,
    pub quantity: Decimal,
    pub reference_type: String,
    pub reference_id: Option<Uuid>,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

/// `in` and `out` carry an unsigned magnitude from the caller; the DB row's
/// stored quantity gets the sign implied by the type (`out` is negated).
/// `adjustment` takes the signed delta directly.
#[derive(Debug, Deserialize)]
pub struct ManualAdjustmentInput {
    pub movement_type: String,
    pub quantity: Decimal,
    pub note: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AdjustmentError {
    #[error("movement_type must be 'in', 'out', or 'adjustment'")]
    InvalidType,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub async fn list_for_article(
    pool: &PgPool,
    article_id: Uuid,
) -> Result<Vec<StockMovement>, sqlx::Error> {
    sqlx::query_as!(
        StockMovement,
        "SELECT * FROM stock_movements WHERE article_id = $1 ORDER BY created_at DESC",
        article_id
    )
    .fetch_all(pool)
    .await
}

pub async fn create_manual(
    pool: &PgPool,
    article_id: Uuid,
    input: &ManualAdjustmentInput,
) -> Result<StockMovement, AdjustmentError> {
    let signed_quantity = match input.movement_type.as_str() {
        "in" => input.quantity.abs(),
        "out" => -input.quantity.abs(),
        "adjustment" => input.quantity,
        _ => return Err(AdjustmentError::InvalidType),
    };

    let movement = sqlx::query_as!(
        StockMovement,
        "INSERT INTO stock_movements (article_id, movement_type, quantity, reference_type, note)
         VALUES ($1, $2, $3, 'manual', $4)
         RETURNING *",
        article_id,
        input.movement_type,
        signed_quantity,
        input.note,
    )
    .fetch_one(pool)
    .await?;

    Ok(movement)
}
