use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AccountingEntry {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub invoice_line_item_id: Uuid,
    pub document_type: String,
    pub document_number: String,
    pub corrected_document_number: Option<String>,
    pub customer_number: i32,
    pub booking_date: NaiveDate,
    pub service_date: NaiveDate,
    pub line_position: i32,
    pub booking_text: String,
    pub currency_code: String,
    pub net_amount: Decimal,
    pub tax_amount: Decimal,
    pub gross_amount: Decimal,
    pub tax_rate_percent: Decimal,
    pub source_sha256: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct StoredExport {
    pub payload: Vec<u8>,
    pub payload_sha256: String,
    pub duplicate: bool,
}

pub struct ExportBatch<'a> {
    pub actor_user_id: Uuid,
    pub idempotency_key: &'a str,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub parameters_sha256: &'a str,
    pub payload: &'a [u8],
    pub entry_ids: &'a [Uuid],
}

#[derive(Debug, Error)]
pub enum ExportStoreError {
    #[error("the idempotency key belongs to different export parameters")]
    IdempotencyConflict,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub async fn entries_for_period(
    pool: &PgPool,
    period_start: NaiveDate,
    period_end: NaiveDate,
) -> Result<Vec<AccountingEntry>, sqlx::Error> {
    sqlx::query_as::<_, AccountingEntry>(
        "SELECT id, invoice_id, invoice_line_item_id, document_type, document_number,
                corrected_document_number, customer_number, booking_date, service_date,
                line_position, booking_text, currency_code, net_amount, tax_amount,
                gross_amount, tax_rate_percent, source_sha256, created_at
         FROM accounting_entries
         WHERE booking_date BETWEEN $1 AND $2
         ORDER BY booking_date, document_number, line_position, id",
    )
    .bind(period_start)
    .bind(period_end)
    .fetch_all(pool)
    .await
}

pub async fn store_export(
    pool: &PgPool,
    batch: &ExportBatch<'_>,
) -> Result<StoredExport, ExportStoreError> {
    let payload_sha256 = hex::encode(Sha256::digest(batch.payload));
    let inserted = sqlx::query(
        "INSERT INTO accounting_export_batches
             (export_type, period_start, period_end, idempotency_key,
              parameters_sha256, payload_sha256, payload, entry_ids, created_by)
         VALUES ('datev_extf_v13', $1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(batch.period_start)
    .bind(batch.period_end)
    .bind(batch.idempotency_key)
    .bind(batch.parameters_sha256)
    .bind(&payload_sha256)
    .bind(batch.payload)
    .bind(batch.entry_ids)
    .bind(batch.actor_user_id)
    .execute(pool)
    .await?;
    if inserted.rows_affected() == 1 {
        return Ok(StoredExport {
            payload: batch.payload.to_vec(),
            payload_sha256,
            duplicate: false,
        });
    }
    let existing: (String, Vec<u8>, String) = sqlx::query_as(
        "SELECT parameters_sha256, payload, payload_sha256
         FROM accounting_export_batches WHERE idempotency_key = $1",
    )
    .bind(batch.idempotency_key)
    .fetch_one(pool)
    .await?;
    if existing.0 != batch.parameters_sha256 {
        return Err(ExportStoreError::IdempotencyConflict);
    }
    Ok(StoredExport {
        payload: existing.1,
        payload_sha256: existing.2,
        duplicate: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invoices::{self, CorrectionInput, InvoiceInput, LineItemInput};

    #[sqlx::test(migrations = "./migrations")]
    async fn issued_and_correction_entries_are_immutable_and_export_idempotent(pool: PgPool) {
        let actor: Uuid = sqlx::query_scalar(
            "INSERT INTO users (username, password_hash, role)
             VALUES ('accounting-admin', 'synthetic', 'administrator') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let customer: Uuid = sqlx::query_scalar(
            "INSERT INTO customers (customer_number, name)
             VALUES (10001, 'Synthetic Accounting Customer') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let invoice = invoices::create(
            &pool,
            &InvoiceInput {
                customer_id: customer,
                notes: String::new(),
            },
        )
        .await
        .unwrap();
        invoices::add_line_item(
            &pool,
            invoice.id,
            &LineItemInput {
                description: "Synthetic consulting".into(),
                article_id: None,
                quantity: Decimal::ONE,
                unit: "Stk".into(),
                unit_price_net: Decimal::new(10000, 2),
                vat_rate_code: "STANDARD".into(),
            },
        )
        .await
        .unwrap();
        let issued = invoices::transition_status(
            &pool,
            invoice.id,
            domain::invoice_status::InvoiceStatus::Sent,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE customers SET customer_number = 20002 WHERE id = $1")
            .bind(customer)
            .execute(&pool)
            .await
            .unwrap();
        let correction = invoices::create_correction(
            &pool,
            actor,
            issued.id,
            &CorrectionInput {
                reason: "Synthetic full reversal".into(),
            },
            "accounting-correction-once",
        )
        .await
        .unwrap();

        let date = issued.issue_date.unwrap();
        let entries = entries_for_period(&pool, date, date).await.unwrap();
        assert_eq!(entries.len(), 2);
        let invoice_entry = entries
            .iter()
            .find(|entry| entry.document_type == "invoice")
            .unwrap();
        let correction_entry = entries
            .iter()
            .find(|entry| entry.document_type == "correction")
            .unwrap();
        assert_eq!(invoice_entry.customer_number, 10001);
        assert_eq!(correction_entry.customer_number, 10001);
        assert_eq!(invoice_entry.gross_amount, Decimal::new(11900, 2));
        assert_eq!(correction_entry.gross_amount, Decimal::new(-11900, 2));
        assert_eq!(
            correction_entry.corrected_document_number.as_deref(),
            issued.invoice_number.as_deref()
        );
        assert_eq!(
            correction_entry.invoice_id,
            correction.correction.invoice.id
        );

        let mutation = sqlx::query("DELETE FROM accounting_entries WHERE id = $1")
            .bind(entries[0].id)
            .execute(&pool)
            .await;
        assert!(mutation.is_err());

        let entry_ids = entries.iter().map(|entry| entry.id).collect::<Vec<_>>();
        let parameters_sha256 = "a".repeat(64);
        let first = store_export(
            &pool,
            &ExportBatch {
                actor_user_id: actor,
                idempotency_key: "datev-export-once",
                period_start: date,
                period_end: date,
                parameters_sha256: &parameters_sha256,
                payload: b"synthetic export",
                entry_ids: &entry_ids,
            },
        )
        .await
        .unwrap();
        let duplicate = store_export(
            &pool,
            &ExportBatch {
                actor_user_id: actor,
                idempotency_key: "datev-export-once",
                period_start: date,
                period_end: date,
                parameters_sha256: &parameters_sha256,
                payload: b"different bytes are never allowed to replace the first export",
                entry_ids: &entry_ids,
            },
        )
        .await
        .unwrap();
        assert!(!first.duplicate);
        assert!(duplicate.duplicate);
        assert_eq!(first.payload, duplicate.payload);
    }
}
