//! Typst-based invoice PDF rendering: minijinja fills a `.typ` template, then
//! `typst compile` (shelled out, not the less-stable embedding API) turns it into a PDF.

mod data;

use std::process::Command;

use minijinja::{Environment, Error as JinjaError, ErrorKind, Value};
use uuid::Uuid;

pub use data::{
    format_date_de, format_money_de, CompanyInfo, CustomerInfo, InvoicePdfInput, LineItemRow,
    VatBreakdownRow,
};

const TEMPLATE_SOURCE: &str = include_str!("../templates/invoice.typ.jinja");

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("failed to render template: {0}")]
    Template(#[from] JinjaError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("typst compile failed: {0}")]
    TypstFailed(String),
}

/// Wraps a value as an escaped Typst string literal (including the surrounding
/// quotes), so templates can safely interpolate arbitrary user text — customer
/// names, descriptions, notes — without it being parsed as Typst markup.
fn typst_string_filter(value: Value) -> Result<String, JinjaError> {
    let s = value.as_str().ok_or_else(|| {
        JinjaError::new(
            ErrorKind::InvalidOperation,
            "tstr filter expects a string value",
        )
    })?;
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    Ok(format!("\"{escaped}\""))
}

fn render_typst_source(input: &InvoicePdfInput) -> Result<String, PdfError> {
    let mut env = Environment::new();
    env.add_filter("tstr", typst_string_filter);
    env.add_template("invoice", TEMPLATE_SOURCE)?;

    let context = minijinja::context! {
        invoice_number => input.invoice_number,
        is_correction => if input.is_correction { "true" } else { "false" },
        corrected_invoice_number => input.corrected_invoice_number,
        correction_reason => input.correction_reason,
        issue_date => format_date_de(input.issue_date),
        due_date => format_date_de(input.due_date),
        notes => input.notes,
        net_total => format_money_de(input.net_total),
        vat_total => format_money_de(input.vat_total),
        gross_total => format_money_de(input.gross_total),
        company => minijinja::context! {
            company_name => input.company.company_name,
            owner_name => input.company.owner_name,
            address_line1 => input.company.address_line1,
            address_line2 => input.company.address_line2,
            zip => input.company.zip,
            city => input.company.city,
            tax_id => input.company.tax_id,
            vat_id => input.company.vat_id,
            iban => input.company.iban,
            bic => input.company.bic,
            bank_name => input.company.bank_name,
            invoice_footer_note => input.company.invoice_footer_note,
        },
        customer => minijinja::context! {
            name => input.customer.name,
            contact_person => input.customer.contact_person,
            address_line1 => input.customer.address_line1,
            address_line2 => input.customer.address_line2,
            zip => input.customer.zip,
            city => input.customer.city,
        },
        line_items => input.line_items.iter().map(|item| minijinja::context! {
            description => item.description,
            quantity => format_money_de(item.quantity),
            unit => item.unit,
            unit_price_net => format_money_de(item.unit_price_net),
            vat_rate_percent => format_money_de(item.vat_rate_percent),
            net_amount => format_money_de(item.net_amount),
            gross_amount => format_money_de(item.gross_amount),
        }).collect::<Vec<_>>(),
        vat_breakdown => input.vat_breakdown.iter().map(|row| minijinja::context! {
            rate_percent => format_money_de(row.rate_percent),
            net_total => format_money_de(row.net_total),
            vat_total => format_money_de(row.vat_total),
            gross_total => format_money_de(row.gross_total),
        }).collect::<Vec<_>>(),
    };

    let tmpl = env.get_template("invoice")?;
    Ok(tmpl.render(context)?)
}

/// Renders an invoice to PDF bytes by shelling out to the `typst` CLI in a
/// scratch directory that's removed again once compilation finishes.
pub fn render_invoice_pdf(input: &InvoicePdfInput) -> Result<Vec<u8>, PdfError> {
    let typst_source = render_typst_source(input)?;

    let work_dir = std::env::temp_dir().join(format!("erplite-pdf-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&work_dir)?;
    let result = (|| {
        let source_path = work_dir.join("invoice.typ");
        let output_path = work_dir.join("invoice.pdf");
        std::fs::write(&source_path, &typst_source)?;

        let output = Command::new("typst")
            .arg("compile")
            .arg(&source_path)
            .arg(&output_path)
            .output()?;

        if !output.status.success() {
            return Err(PdfError::TypstFailed(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }

        Ok(std::fs::read(&output_path)?)
    })();

    let _ = std::fs::remove_dir_all(&work_dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    fn sample_input() -> InvoicePdfInput {
        InvoicePdfInput {
            invoice_number: "RE-2026-0001".into(),
            is_correction: false,
            corrected_invoice_number: String::new(),
            correction_reason: String::new(),
            issue_date: NaiveDate::from_ymd_opt(2026, 8, 11).unwrap(),
            due_date: NaiveDate::from_ymd_opt(2026, 8, 25).unwrap(),
            company: CompanyInfo {
                company_name: "itmitalles.de".into(),
                owner_name: "Tim Müßig".into(),
                address_line1: "Musterstraße 1".into(),
                address_line2: String::new(),
                zip: "12345".into(),
                city: "Müllerstädt".into(),
                tax_id: "12/345/67890".into(),
                vat_id: String::new(),
                iban: "DE00 0000 0000 0000 0000 00".into(),
                bic: String::new(),
                bank_name: String::new(),
                invoice_footer_note: "Vielen Dank für \"Ihren\" Auftrag!".into(),
            },
            customer: CustomerInfo {
                name: "Zweiter Kunde GmbH".into(),
                contact_person: "Örsel Özdemir".into(),
                address_line1: "Kundenstraße 5".into(),
                address_line2: String::new(),
                zip: "54321".into(),
                city: "Kundenstadt".into(),
            },
            line_items: vec![LineItemRow {
                description: "Beratung \\ Konzeption".into(),
                quantity: dec!(10.00),
                unit: "Std".into(),
                unit_price_net: dec!(100.00),
                vat_rate_percent: dec!(19.00),
                net_amount: dec!(1000.00),
                gross_amount: dec!(1190.00),
            }],
            vat_breakdown: vec![VatBreakdownRow {
                rate_percent: dec!(19.00),
                net_total: dec!(1000.00),
                vat_total: dec!(190.00),
                gross_total: dec!(1190.00),
            }],
            net_total: dec!(1000.00),
            vat_total: dec!(190.00),
            gross_total: dec!(1190.00),
            notes: String::new(),
        }
    }

    #[test]
    fn renders_valid_pdf_with_quotes_and_backslashes_in_input() {
        // sample_input() includes `"` and `\` in text fields (Typst string-literal
        // special characters) — this is a regression guard that they're escaped
        // rather than breaking out of the literal or crashing the compiler.
        let pdf = render_invoice_pdf(&sample_input()).expect("pdf rendering should succeed");
        assert!(pdf.starts_with(b"%PDF"), "output should be a PDF file");
        assert!(
            pdf.len() > 1000,
            "PDF should have real content, not just a header"
        );
    }

    #[test]
    fn correction_source_contains_an_explicit_original_reference() {
        let mut input = sample_input();
        input.invoice_number = "KR-2026-0001".into();
        input.is_correction = true;
        input.corrected_invoice_number = "RE-2026-0001".into();
        input.correction_reason = "Synthetic full reversal".into();
        let source = render_typst_source(&input).unwrap();
        assert!(source.contains("Korrekturrechnung"));
        assert!(source.contains("RE-2026-0001"));
        assert!(source.contains("Synthetic full reversal"));
    }
}
