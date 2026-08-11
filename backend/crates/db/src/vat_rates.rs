use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct VatRate {
    pub code: String,
    pub rate_percent: Decimal,
}

pub async fn list(pool: &PgPool) -> Result<Vec<VatRate>, sqlx::Error> {
    sqlx::query_as!(
        VatRate,
        "SELECT code, rate_percent FROM vat_rates ORDER BY sort_order"
    )
    .fetch_all(pool)
    .await
}
