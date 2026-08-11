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
                  i.status, i.issue_date, i.due_date, i.gross_total, i.created_at
           FROM invoices i JOIN customers c ON c.id = i.customer_id
           ORDER BY i.created_at DESC"#
    )
    .fetch_all(pool)
    .await
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
    Ok(Some(InvoiceWithLineItems {
        invoice,
        line_items,
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

            sqlx::query_as!(
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
            .await?
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
