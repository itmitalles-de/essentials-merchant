use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;
use db::invoices::InvoiceWithLineItems;

#[derive(Debug, Deserialize)]
struct CustomerSnapshot {
    name: String,
    contact_person: String,
    address_line1: String,
    address_line2: String,
    zip: String,
    city: String,
}

#[derive(Debug, Deserialize)]
struct CompanySnapshot {
    company_name: String,
    owner_name: String,
    address_line1: String,
    address_line2: String,
    zip: String,
    city: String,
    tax_id: String,
    vat_id: String,
    iban: String,
    bic: String,
    bank_name: String,
    invoice_footer_note: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PdfGenError {
    #[error("invoice is missing its snapshot data (must be sent first)")]
    MissingSnapshot,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Pdf(#[from] pdf::PdfError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Renders the invoice's PDF and stores it under the configured storage dir,
/// recording the path on the invoice row. Requires the invoice to already be
/// sent (number and customer/company snapshots assigned) — called lazily from
/// the download endpoint, not as part of the `sent` transition itself.
pub async fn generate_and_store(state: &AppState, invoice_id: Uuid) -> Result<(), PdfGenError> {
    let full: InvoiceWithLineItems = db::invoices::get(&state.pool, invoice_id)
        .await?
        .ok_or(PdfGenError::MissingSnapshot)?;

    let customer_snapshot: CustomerSnapshot = serde_json::from_value(
        full.invoice
            .customer_snapshot
            .clone()
            .ok_or(PdfGenError::MissingSnapshot)?,
    )?;
    let company_snapshot: CompanySnapshot = serde_json::from_value(
        full.invoice
            .company_snapshot
            .clone()
            .ok_or(PdfGenError::MissingSnapshot)?,
    )?;
    let invoice_number = full
        .invoice
        .invoice_number
        .clone()
        .ok_or(PdfGenError::MissingSnapshot)?;
    let issue_date = full
        .invoice
        .issue_date
        .ok_or(PdfGenError::MissingSnapshot)?;
    let due_date = full.invoice.due_date.ok_or(PdfGenError::MissingSnapshot)?;

    let vat_breakdown_input: Vec<(rust_decimal::Decimal, rust_decimal::Decimal)> = full
        .line_items
        .iter()
        .map(|li| (li.net_amount, li.vat_rate_percent))
        .collect();
    let vat_breakdown = domain::vat::vat_breakdown(&vat_breakdown_input);

    let input = pdf::InvoicePdfInput {
        invoice_number: invoice_number.clone(),
        issue_date,
        due_date,
        company: pdf::CompanyInfo {
            company_name: company_snapshot.company_name,
            owner_name: company_snapshot.owner_name,
            address_line1: company_snapshot.address_line1,
            address_line2: company_snapshot.address_line2,
            zip: company_snapshot.zip,
            city: company_snapshot.city,
            tax_id: company_snapshot.tax_id,
            vat_id: company_snapshot.vat_id,
            iban: company_snapshot.iban,
            bic: company_snapshot.bic,
            bank_name: company_snapshot.bank_name,
            invoice_footer_note: company_snapshot.invoice_footer_note,
        },
        customer: pdf::CustomerInfo {
            name: customer_snapshot.name,
            contact_person: customer_snapshot.contact_person,
            address_line1: customer_snapshot.address_line1,
            address_line2: customer_snapshot.address_line2,
            zip: customer_snapshot.zip,
            city: customer_snapshot.city,
        },
        line_items: full
            .line_items
            .iter()
            .map(|li| pdf::LineItemRow {
                description: li.description.clone(),
                quantity: li.quantity,
                unit: li.unit.clone(),
                unit_price_net: li.unit_price_net,
                vat_rate_percent: li.vat_rate_percent,
                net_amount: li.net_amount,
                gross_amount: li.gross_amount,
            })
            .collect(),
        vat_breakdown: vat_breakdown
            .into_iter()
            .map(|row| pdf::VatBreakdownRow {
                rate_percent: row.rate_percent,
                net_total: row.net_total,
                vat_total: row.vat_total,
                gross_total: row.gross_total,
            })
            .collect(),
        net_total: full.invoice.net_total,
        vat_total: full.invoice.vat_total,
        gross_total: full.invoice.gross_total,
        notes: full.invoice.notes.clone(),
    };

    let bytes = pdf::render_invoice_pdf(&input)?;

    let safe_number = invoice_number.replace(['/', '\\'], "-");
    let path = std::path::Path::new(&state.pdf_storage_dir).join(format!("{safe_number}.pdf"));
    std::fs::write(&path, bytes)?;

    db::invoices::set_pdf_path(&state.pool, invoice_id, &path.to_string_lossy()).await?;
    Ok(())
}
