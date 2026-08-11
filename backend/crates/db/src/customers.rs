use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Customer {
    pub id: Uuid,
    pub customer_number: i32,
    pub name: String,
    pub contact_person: String,
    pub address_line1: String,
    pub address_line2: String,
    pub zip: String,
    pub city: String,
    pub country: String,
    pub email: String,
    pub phone: String,
    pub ust_id: String,
    pub default_payment_terms_days: Option<i32>,
    pub notes: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CustomerInput {
    pub name: String,
    pub contact_person: String,
    pub address_line1: String,
    pub address_line2: String,
    pub zip: String,
    pub city: String,
    pub country: String,
    pub email: String,
    pub phone: String,
    pub ust_id: String,
    pub default_payment_terms_days: Option<i32>,
    pub notes: String,
    pub active: bool,
}

pub async fn list(pool: &PgPool) -> Result<Vec<Customer>, sqlx::Error> {
    sqlx::query_as!(Customer, "SELECT * FROM customers ORDER BY customer_number")
        .fetch_all(pool)
        .await
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<Customer>, sqlx::Error> {
    sqlx::query_as!(Customer, "SELECT * FROM customers WHERE id = $1", id)
        .fetch_optional(pool)
        .await
}

pub async fn create(pool: &PgPool, input: &CustomerInput) -> Result<Customer, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let customer_number = sqlx::query_scalar!(
        "UPDATE company_settings SET next_customer_number = next_customer_number + 1
         WHERE id = 1 RETURNING next_customer_number - 1"
    )
    .fetch_one(&mut *tx)
    .await?
    .expect("next_customer_number - 1 is never null: the column is NOT NULL");

    let customer = sqlx::query_as!(
        Customer,
        "INSERT INTO customers (
            customer_number, name, contact_person, address_line1, address_line2,
            zip, city, country, email, phone, ust_id, default_payment_terms_days,
            notes, active
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         RETURNING *",
        customer_number,
        input.name,
        input.contact_person,
        input.address_line1,
        input.address_line2,
        input.zip,
        input.city,
        input.country,
        input.email,
        input.phone,
        input.ust_id,
        input.default_payment_terms_days,
        input.notes,
        input.active,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(customer)
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    input: &CustomerInput,
) -> Result<Option<Customer>, sqlx::Error> {
    sqlx::query_as!(
        Customer,
        "UPDATE customers SET
            name = $2, contact_person = $3, address_line1 = $4, address_line2 = $5,
            zip = $6, city = $7, country = $8, email = $9, phone = $10, ust_id = $11,
            default_payment_terms_days = $12, notes = $13, active = $14
         WHERE id = $1
         RETURNING *",
        id,
        input.name,
        input.contact_person,
        input.address_line1,
        input.address_line2,
        input.zip,
        input.city,
        input.country,
        input.email,
        input.phone,
        input.ust_id,
        input.default_payment_terms_days,
        input.notes,
        input.active,
    )
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM customers WHERE id = $1", id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
