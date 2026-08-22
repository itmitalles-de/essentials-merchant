use sqlx::postgres::{PgPool, PgPoolOptions};

pub mod accounting;
pub mod articles;
pub mod commerce;
pub mod company_settings;
pub mod customers;
pub mod invoices;
pub mod marketplace;
pub mod modules;
pub mod provider_secrets;
pub mod sales_orders;
pub mod stock_movements;
pub mod users;
pub mod vat_rates;

pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
}

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
