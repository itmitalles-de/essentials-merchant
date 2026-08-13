use chrono::{DateTime, NaiveDate, Utc};
use domain::invoice_status::InvoiceStatus;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Invoice {
    pub id: Uuid,
    pub invoice_number: Option<String>,
    pub customer_id: Uuid,
    pub status: String,
    pub issue_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub customer_snapshot: Option<Json>,
    pub company_snapshot: Option<Json>,
    pub net_total: Decimal,
    pub vat_total: Decimal,
    pub gross_total: Decimal,
    pub notes: String,
    pub pdf_path: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub document_type: String,
    pub corrects_invoice_id: Option<Uuid>,
    pub correction_reason: Option<String>,
    pub correction_idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct InvoiceListItem {
    pub id: Uuid,
    pub invoice_number: Option<String>,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub status: String,
    pub issue_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub gross_total: Decimal,
    pub created_at: DateTime<Utc>,
    pub document_type: String,
    pub corrects_invoice_id: Option<Uuid>,
    pub corrected_invoice_number: Option<String>,
    pub correction_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct InvoiceLineItem {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub position: i32,
    pub description: String,
    pub article_id: Option<Uuid>,
    pub quantity: Decimal,
    pub unit: String,
    pub unit_price_net: Decimal,
    pub vat_rate_code: String,
    pub vat_rate_percent: Decimal,
    pub net_amount: Decimal,
    pub vat_amount: Decimal,
    pub gross_amount: Decimal,
}

#[derive(Debug, Serialize)]
pub struct InvoiceWithLineItems {
    #[serde(flatten)]
    pub invoice: Invoice,
    pub line_items: Vec<InvoiceLineItem>,
    pub correction: Option<InvoiceReference>,
    pub corrected_invoice_number: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InvoiceReference {
    pub id: Uuid,
    pub invoice_number: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InvoiceInput {
    pub customer_id: Uuid,
    pub notes: String,
}

#[derive(Debug, Deserialize)]
pub struct LineItemInput {
    pub description: String,
    pub article_id: Option<Uuid>,
    pub quantity: Decimal,
    pub unit: String,
    pub unit_price_net: Decimal,
    pub vat_rate_code: String,
}

#[derive(Debug, Deserialize)]
pub struct CorrectionInput {
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct CorrectionCreation {
    pub correction: InvoiceWithLineItems,
    pub duplicate: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CorrectionError {
    #[error("invoice not found")]
    NotFound,
    #[error("only issued invoices can be corrected")]
    NotIssued,
    #[error("a correction cannot correct another correction")]
    CorrectionOfCorrection,
    #[error("invoice already has a full correction")]
    AlreadyCorrected,
    #[error("correction reason or idempotency key is invalid")]
    InvalidInput,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum TransitionError {
    #[error("invoice not found")]
    NotFound,
    #[error("cannot transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub async fn list(pool: &PgPool) -> Result<Vec<InvoiceListItem>, sqlx::Error> {
    sqlx::query_as!(
        InvoiceListItem,
        r#"SELECT i.id, i.invoice_number, i.customer_id, c.name AS customer_name,
                  i.status, i.issue_date, i.due_date, i.gross_total, i.created_at,
                  i.document_type, i.corrects_invoice_id,
                  original.invoice_number AS corrected_invoice_number,
                  i.correction_reason
           FROM invoices i JOIN customers c ON c.id = i.customer_id
           LEFT JOIN invoices original ON original.id = i.corrects_invoice_id
           ORDER BY i.created_at DESC"#
    )
    .fetch_all(pool)
    .await
}

pub async fn create_correction(
    pool: &PgPool,
    actor_user_id: Uuid,
    original_invoice_id: Uuid,
    input: &CorrectionInput,
    idempotency_key: &str,
) -> Result<CorrectionCreation, CorrectionError> {
    let reason = input.reason.trim();
    if reason.is_empty()
        || reason.len() > 1_000
        || idempotency_key.trim().is_empty()
        || idempotency_key.len() > 200
    {
        return Err(CorrectionError::InvalidInput);
    }

    let mut tx = pool.begin().await?;
    // Serialize equal idempotency keys before looking them up. This also makes
    // concurrent requests targeting different invoices fail deterministically
    // instead of leaking a unique-constraint race to the API.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(idempotency_key)
        .execute(&mut *tx)
        .await?;
    let existing: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, corrects_invoice_id FROM invoices WHERE correction_idempotency_key = $1",
    )
    .bind(idempotency_key)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((existing_id, corrects_invoice_id)) = existing {
        if corrects_invoice_id != Some(original_invoice_id) {
            return Err(CorrectionError::InvalidInput);
        }
        tx.commit().await?;
        let correction = get(pool, existing_id)
            .await?
            .ok_or(CorrectionError::NotFound)?;
        return Ok(CorrectionCreation {
            correction,
            duplicate: true,
        });
    }

    let original: Invoice = sqlx::query_as("SELECT * FROM invoices WHERE id = $1 FOR UPDATE")
        .bind(original_invoice_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(CorrectionError::NotFound)?;
    if original.status == "draft" {
        return Err(CorrectionError::NotIssued);
    }
    if original.document_type != "invoice" {
        return Err(CorrectionError::CorrectionOfCorrection);
    }
    let prior_correction: Option<(Uuid, Option<String>)> = sqlx::query_as(
        "SELECT id, correction_idempotency_key FROM invoices WHERE corrects_invoice_id = $1",
    )
    .bind(original_invoice_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((existing_id, existing_key)) = prior_correction {
        if existing_key.as_deref() == Some(idempotency_key) {
            tx.commit().await?;
            let correction = get(pool, existing_id)
                .await?
                .ok_or(CorrectionError::NotFound)?;
            return Ok(CorrectionCreation {
                correction,
                duplicate: true,
            });
        }
        return Err(CorrectionError::AlreadyCorrected);
    }

    let correction_number: String = sqlx::query_scalar(
        "UPDATE company_settings SET next_correction_number = next_correction_number + 1
         WHERE id = 1
         RETURNING correction_number_prefix || '-' || to_char(now(), 'YYYY') || '-' ||
             lpad((next_correction_number - 1)::text, 4, '0')",
    )
    .fetch_one(&mut *tx)
    .await?;
    let issue_date = Utc::now().date_naive();
    let correction_id: Uuid = sqlx::query_scalar(
        "INSERT INTO invoices (
             customer_id, status, customer_snapshot, company_snapshot, notes,
             document_type, corrects_invoice_id, correction_reason,
             correction_idempotency_key
         ) VALUES ($1, 'draft', $2, $3, $4, 'correction', $5, $6, $7)
         RETURNING id",
    )
    .bind(original.customer_id)
    .bind(original.customer_snapshot.clone())
    .bind(original.company_snapshot.clone())
    .bind(format!(
        "Full correction of {}: {}",
        original
            .invoice_number
            .as_deref()
            .unwrap_or("issued invoice"),
        reason
    ))
    .bind(original_invoice_id)
    .bind(reason)
    .bind(idempotency_key)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO invoice_line_items (
             invoice_id, position, description, article_id, quantity, unit,
             unit_price_net, vat_rate_code, vat_rate_percent, net_amount,
             vat_amount, gross_amount
         )
         SELECT $1, position, description, article_id, -abs(quantity), unit,
                unit_price_net, vat_rate_code, vat_rate_percent, -abs(net_amount),
                -abs(vat_amount), -abs(gross_amount)
         FROM invoice_line_items WHERE invoice_id = $2 ORDER BY position",
    )
    .bind(correction_id)
    .bind(original_invoice_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE invoices SET status = 'sent', invoice_number = $2,
             issue_date = $3, due_date = $3, net_total = -abs($4),
             vat_total = -abs($5), gross_total = -abs($6), sent_at = now()
         WHERE id = $1",
    )
    .bind(correction_id)
    .bind(&correction_number)
    .bind(issue_date)
    .bind(original.net_total)
    .bind(original.vat_total)
    .bind(original.gross_total)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO invoice_audit_log (
             actor_user_id, action, invoice_id, related_invoice_id,
             idempotency_key, details
         ) VALUES ($1, 'invoice.correction_created', $2, $3, $4, $5)",
    )
    .bind(actor_user_id)
    .bind(original_invoice_id)
    .bind(correction_id)
    .bind(idempotency_key)
    .bind(json!({
        "original_invoice_number": original.invoice_number,
        "correction_invoice_number": correction_number,
        "reason": reason,
    }))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let correction = get(pool, correction_id)
        .await?
        .ok_or(CorrectionError::NotFound)?;
    Ok(CorrectionCreation {
        correction,
        duplicate: false,
    })
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<InvoiceWithLineItems>, sqlx::Error> {
    let Some(invoice) = get_bare(pool, id).await? else {
        return Ok(None);
    };
    let line_items = sqlx::query_as!(
        InvoiceLineItem,
        "SELECT * FROM invoice_line_items WHERE invoice_id = $1 ORDER BY position",
        id
    )
    .fetch_all(pool)
    .await?;
    let correction = sqlx::query_as::<_, InvoiceReference>(
        "SELECT id, invoice_number FROM invoices WHERE corrects_invoice_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let corrected_invoice_number = if let Some(original_id) = invoice.corrects_invoice_id {
        sqlx::query_scalar("SELECT invoice_number FROM invoices WHERE id = $1")
            .bind(original_id)
            .fetch_optional(pool)
            .await?
            .flatten()
    } else {
        None
    };
    Ok(Some(InvoiceWithLineItems {
        invoice,
        line_items,
        correction,
        corrected_invoice_number,
    }))
}

/// Fetches the invoice row without its line items — used by handlers that only
/// need to check existence/status before delegating to a mutating function.
pub async fn get_bare(pool: &PgPool, id: Uuid) -> Result<Option<Invoice>, sqlx::Error> {
    sqlx::query_as!(Invoice, "SELECT * FROM invoices WHERE id = $1", id)
        .fetch_optional(pool)
        .await
}

pub async fn create(pool: &PgPool, input: &InvoiceInput) -> Result<Invoice, sqlx::Error> {
    sqlx::query_as!(
        Invoice,
        "INSERT INTO invoices (customer_id, notes) VALUES ($1, $2) RETURNING *",
        input.customer_id,
        input.notes,
    )
    .fetch_one(pool)
    .await
}

/// Only drafts may be edited; returns `None` if the invoice doesn't exist or isn't a draft.
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    input: &InvoiceInput,
) -> Result<Option<Invoice>, sqlx::Error> {
    sqlx::query_as!(
        Invoice,
        "UPDATE invoices SET customer_id = $2, notes = $3
         WHERE id = $1 AND status = 'draft'
         RETURNING *",
        id,
        input.customer_id,
        input.notes,
    )
    .fetch_optional(pool)
    .await
}

pub async fn set_pdf_path(pool: &PgPool, id: Uuid, pdf_path: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE invoices SET pdf_path = $2 WHERE id = $1",
        id,
        pdf_path
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Only drafts may be deleted.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM invoices WHERE id = $1 AND status = 'draft'",
        id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

async fn recompute_totals(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invoice_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE invoices SET
            net_total = COALESCE((SELECT SUM(net_amount) FROM invoice_line_items WHERE invoice_id = $1), 0),
            vat_total = COALESCE((SELECT SUM(vat_amount) FROM invoice_line_items WHERE invoice_id = $1), 0),
            gross_total = COALESCE((SELECT SUM(gross_amount) FROM invoice_line_items WHERE invoice_id = $1), 0)
         WHERE id = $1",
        invoice_id
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn add_line_item(
    pool: &PgPool,
    invoice_id: Uuid,
    input: &LineItemInput,
) -> Result<InvoiceLineItem, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let vat_rate_percent = sqlx::query_scalar!(
        "SELECT rate_percent FROM vat_rates WHERE code = $1",
        input.vat_rate_code
    )
    .fetch_one(&mut *tx)
    .await?;

    let net_amount = domain::vat::round_money(input.quantity * input.unit_price_net);
    let (vat_amount, gross_amount) = domain::vat::calc_vat(net_amount, vat_rate_percent);

    let next_position = sqlx::query_scalar!(
        r#"SELECT COALESCE(MAX(position), 0) + 1 AS "next_position!"
           FROM invoice_line_items WHERE invoice_id = $1"#,
        invoice_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let line_item = sqlx::query_as!(
        InvoiceLineItem,
        "INSERT INTO invoice_line_items (
            invoice_id, position, description, article_id, quantity, unit,
            unit_price_net, vat_rate_code, vat_rate_percent, net_amount, vat_amount, gross_amount
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING *",
        invoice_id,
        next_position,
        input.description,
        input.article_id,
        input.quantity,
        input.unit,
        input.unit_price_net,
        input.vat_rate_code,
        vat_rate_percent,
        net_amount,
        vat_amount,
        gross_amount,
    )
    .fetch_one(&mut *tx)
    .await?;

    recompute_totals(&mut tx, invoice_id).await?;
    tx.commit().await?;
    Ok(line_item)
}

pub async fn update_line_item(
    pool: &PgPool,
    invoice_id: Uuid,
    line_item_id: Uuid,
    input: &LineItemInput,
) -> Result<Option<InvoiceLineItem>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let vat_rate_percent = sqlx::query_scalar!(
        "SELECT rate_percent FROM vat_rates WHERE code = $1",
        input.vat_rate_code
    )
    .fetch_one(&mut *tx)
    .await?;

    let net_amount = domain::vat::round_money(input.quantity * input.unit_price_net);
    let (vat_amount, gross_amount) = domain::vat::calc_vat(net_amount, vat_rate_percent);

    let line_item = sqlx::query_as!(
        InvoiceLineItem,
        "UPDATE invoice_line_items SET
            description = $3, article_id = $4, quantity = $5, unit = $6, unit_price_net = $7,
            vat_rate_code = $8, vat_rate_percent = $9, net_amount = $10, vat_amount = $11, gross_amount = $12
         WHERE id = $1 AND invoice_id = $2
         RETURNING *",
        line_item_id,
        invoice_id,
        input.description,
        input.article_id,
        input.quantity,
        input.unit,
        input.unit_price_net,
        input.vat_rate_code,
        vat_rate_percent,
        net_amount,
        vat_amount,
        gross_amount,
    )
    .fetch_optional(&mut *tx)
    .await?;

    if line_item.is_some() {
        recompute_totals(&mut tx, invoice_id).await?;
    }
    tx.commit().await?;
    Ok(line_item)
}

pub async fn delete_line_item(
    pool: &PgPool,
    invoice_id: Uuid,
    line_item_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let result = sqlx::query!(
        "DELETE FROM invoice_line_items WHERE id = $1 AND invoice_id = $2",
        line_item_id,
        invoice_id
    )
    .execute(&mut *tx)
    .await?;
    let deleted = result.rows_affected() > 0;

    if deleted {
        recompute_totals(&mut tx, invoice_id).await?;
    }
    tx.commit().await?;
    Ok(deleted)
}

#[derive(sqlx::FromRow)]
struct InvoiceNumberingAndCompany {
    company_name: String,
    owner_name: String,
    address_line1: String,
    address_line2: String,
    zip: String,
    city: String,
    country: String,
    email: String,
    phone: String,
    tax_id: String,
    vat_id: String,
    iban: String,
    bic: String,
    bank_name: String,
    invoice_footer_note: String,
    invoice_number: String,
    default_payment_terms_days: i32,
}

pub async fn transition_status(
    pool: &PgPool,
    invoice_id: Uuid,
    target: InvoiceStatus,
) -> Result<Invoice, TransitionError> {
    let mut tx = pool.begin().await?;

    let invoice = sqlx::query_as!(
        Invoice,
        "SELECT * FROM invoices WHERE id = $1 FOR UPDATE",
        invoice_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(TransitionError::NotFound)?;

    let current = InvoiceStatus::parse(&invoice.status)
        .expect("status column always holds a valid InvoiceStatus string (DB CHECK constraint enforces this)");

    if !current.can_transition_to(target) {
        return Err(TransitionError::InvalidTransition {
            from: current.as_str().into(),
            to: target.as_str().into(),
        });
    }

    let updated = match target {
        InvoiceStatus::Sent => {
            let numbering = sqlx::query_as!(
                InvoiceNumberingAndCompany,
                r#"UPDATE company_settings SET next_invoice_number = next_invoice_number + 1
                   WHERE id = 1
                   RETURNING company_name, owner_name, address_line1, address_line2, zip, city,
                             country, email, phone, tax_id, vat_id, iban, bic, bank_name,
                             invoice_footer_note,
                             invoice_number_prefix || '-' || to_char(now(), 'YYYY') || '-' ||
                                 lpad((next_invoice_number - 1)::text, 4, '0') AS "invoice_number!",
                             default_payment_terms_days"#
            )
            .fetch_one(&mut *tx)
            .await?;

            let customer = sqlx::query_as!(
                crate::customers::Customer,
                "SELECT * FROM customers WHERE id = $1",
                invoice.customer_id
            )
            .fetch_one(&mut *tx)
            .await?;

            let issue_date = Utc::now().date_naive();
            let due_date = issue_date + chrono::Duration::days(numbering.default_payment_terms_days as i64);

            let customer_snapshot = json!({
                "customer_number": customer.customer_number,
                "name": customer.name,
                "contact_person": customer.contact_person,
                "address_line1": customer.address_line1,
                "address_line2": customer.address_line2,
                "zip": customer.zip,
                "city": customer.city,
                "country": customer.country,
                "ust_id": customer.ust_id,
            });
            let company_snapshot = json!({
                "company_name": numbering.company_name,
                "owner_name": numbering.owner_name,
                "address_line1": numbering.address_line1,
                "address_line2": numbering.address_line2,
                "zip": numbering.zip,
                "city": numbering.city,
                "country": numbering.country,
                "email": numbering.email,
                "phone": numbering.phone,
                "tax_id": numbering.tax_id,
                "vat_id": numbering.vat_id,
                "iban": numbering.iban,
                "bic": numbering.bic,
                "bank_name": numbering.bank_name,
                "invoice_footer_note": numbering.invoice_footer_note,
            });

            let updated = sqlx::query_as!(
                Invoice,
                "UPDATE invoices SET
                    status = 'sent', invoice_number = $2, issue_date = $3, due_date = $4,
                    customer_snapshot = $5, company_snapshot = $6, sent_at = now()
                 WHERE id = $1
                 RETURNING *",
                invoice_id,
                numbering.invoice_number,
                issue_date,
                due_date,
                customer_snapshot,
                company_snapshot,
            )
            .fetch_one(&mut *tx)
            .await?;

            // Stock-linked line items ship out once the invoice is finalized.
            let stocked_items = sqlx::query!(
                "SELECT article_id AS \"article_id!\", quantity
                 FROM invoice_line_items
                 WHERE invoice_id = $1 AND article_id IS NOT NULL",
                invoice_id
            )
            .fetch_all(&mut *tx)
            .await?;
            for item in stocked_items {
                sqlx::query!(
                    "INSERT INTO stock_movements (article_id, movement_type, quantity, reference_type, reference_id)
                     VALUES ($1, 'out', $2, 'invoice', $3)",
                    item.article_id,
                    -item.quantity.abs(),
                    invoice_id,
                )
                .execute(&mut *tx)
                .await?;
            }

            updated
        }
        InvoiceStatus::Paid => {
            sqlx::query_as!(
                Invoice,
                "UPDATE invoices SET status = 'paid', paid_at = now() WHERE id = $1 RETURNING *",
                invoice_id
            )
            .fetch_one(&mut *tx)
            .await?
        }
        InvoiceStatus::Overdue => {
            sqlx::query_as!(
                Invoice,
                "UPDATE invoices SET status = 'overdue' WHERE id = $1 RETURNING *",
                invoice_id
            )
            .fetch_one(&mut *tx)
            .await?
        }
        InvoiceStatus::Cancelled => {
            sqlx::query_as!(
                Invoice,
                "UPDATE invoices SET status = 'cancelled', cancelled_at = now() WHERE id = $1 RETURNING *",
                invoice_id
            )
            .fetch_one(&mut *tx)
            .await?
        }
        InvoiceStatus::Draft => unreachable!("no transition targets Draft"),
    };

    tx.commit().await?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::articles::ArticleInput;
    use crate::customers::CustomerInput;

    #[sqlx::test(migrations = "./migrations")]
    async fn correction_is_idempotent_immutable_and_does_not_book_stock(pool: PgPool) {
        let actor_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (username, password_hash, role)
             VALUES ('correction-admin', 'synthetic', 'administrator') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let customer = crate::customers::create(
            &pool,
            &CustomerInput {
                name: "Synthetic Customer GmbH".to_owned(),
                contact_person: "Fixture Person".to_owned(),
                address_line1: "Fixture Street 1".to_owned(),
                address_line2: String::new(),
                zip: "10115".to_owned(),
                city: "Berlin".to_owned(),
                country: "DE".to_owned(),
                email: "customer@example.test".to_owned(),
                phone: String::new(),
                ust_id: String::new(),
                default_payment_terms_days: None,
                notes: String::new(),
                active: true,
            },
        )
        .await
        .unwrap();
        let article = crate::articles::create(
            &pool,
            &ArticleInput {
                sku: "CORRECTION-SYNTHETIC-1".to_owned(),
                name: "Synthetic product".to_owned(),
                unit: "Stk".to_owned(),
                sales_price_net: Decimal::new(1000, 2),
                default_vat_rate_code: "STANDARD".to_owned(),
                purchase_price_net: None,
                min_stock_quantity: None,
                active: true,
            },
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO stock_movements (article_id, movement_type, quantity)
             VALUES ($1, 'in', 10)",
        )
        .bind(article.id)
        .execute(&pool)
        .await
        .unwrap();
        let invoice = create(
            &pool,
            &InvoiceInput {
                customer_id: customer.id,
                notes: "Original snapshot".to_owned(),
            },
        )
        .await
        .unwrap();
        add_line_item(
            &pool,
            invoice.id,
            &LineItemInput {
                description: "Synthetic product".to_owned(),
                article_id: Some(article.id),
                quantity: Decimal::new(2, 0),
                unit: "Stk".to_owned(),
                unit_price_net: Decimal::new(1000, 2),
                vat_rate_code: "STANDARD".to_owned(),
            },
        )
        .await
        .unwrap();
        let issued = transition_status(&pool, invoice.id, InvoiceStatus::Sent)
            .await
            .unwrap();
        let stock_after_issue: Decimal =
            sqlx::query_scalar("SELECT stock_quantity FROM articles WHERE id = $1")
                .bind(article.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stock_after_issue, Decimal::new(8, 0));

        let correction_input = CorrectionInput {
            reason: "Synthetic full reversal".to_owned(),
        };
        let (left, right) = tokio::join!(
            create_correction(
                &pool,
                actor_id,
                invoice.id,
                &correction_input,
                "correction-idempotency-1",
            ),
            create_correction(
                &pool,
                actor_id,
                invoice.id,
                &correction_input,
                "correction-idempotency-1",
            ),
        );
        let left = left.unwrap();
        let right = right.unwrap();
        let (first, duplicate) = if left.duplicate {
            (right, left)
        } else {
            (left, right)
        };
        assert!(!first.duplicate);
        assert!(duplicate.duplicate);
        assert_eq!(first.correction.invoice.id, duplicate.correction.invoice.id);
        let replayed = create_correction(
            &pool,
            actor_id,
            invoice.id,
            &correction_input,
            "correction-idempotency-1",
        )
        .await
        .unwrap();
        assert!(replayed.duplicate);
        assert_eq!(first.correction.invoice.id, replayed.correction.invoice.id);
        assert_eq!(first.correction.invoice.document_type, "correction");
        assert_eq!(
            first.correction.invoice.corrects_invoice_id,
            Some(invoice.id)
        );
        assert!(first.correction.invoice.net_total.is_sign_negative());
        assert!(first.correction.invoice.vat_total.is_sign_negative());
        assert!(first.correction.invoice.gross_total.is_sign_negative());
        assert!(first.correction.line_items[0].quantity.is_sign_negative());
        assert_eq!(
            first.correction.invoice.customer_snapshot,
            issued.customer_snapshot
        );

        let stock_after_correction: Decimal =
            sqlx::query_scalar("SELECT stock_quantity FROM articles WHERE id = $1")
                .bind(article.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let invoice_stock_movements: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM stock_movements WHERE reference_type = 'invoice'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stock_after_correction, stock_after_issue);
        assert_eq!(invoice_stock_movements, 1);
        assert!(
            sqlx::query("UPDATE invoices SET notes = 'mutated' WHERE id = $1")
                .bind(invoice.id)
                .execute(&pool)
                .await
                .is_err()
        );
        assert!(sqlx::query(
            "UPDATE invoice_line_items SET description = 'mutated' WHERE invoice_id = $1"
        )
        .bind(first.correction.invoice.id)
        .execute(&pool)
        .await
        .is_err());
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM invoice_audit_log
             WHERE action = 'invoice.correction_created'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit_count, 1);
    }
}
