use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CompanySettings {
    pub company_name: String,
    pub owner_name: String,
    pub address_line1: String,
    pub address_line2: String,
    pub zip: String,
    pub city: String,
    pub country: String,
    pub email: String,
    pub phone: String,
    pub tax_id: String,
    pub vat_id: String,
    pub iban: String,
    pub bic: String,
    pub bank_name: String,
    pub invoice_number_prefix: String,
    pub next_invoice_number: i32,
    pub next_customer_number: i32,
    pub invoice_footer_note: String,
    pub default_payment_terms_days: i32,
    pub skr: String,
    pub datev_berater_nr: String,
    pub datev_mandant_nr: String,
}

#[derive(Debug, Deserialize)]
pub struct CompanySettingsUpdate {
    pub company_name: String,
    pub owner_name: String,
    pub address_line1: String,
    pub address_line2: String,
    pub zip: String,
    pub city: String,
    pub country: String,
    pub email: String,
    pub phone: String,
    pub tax_id: String,
    pub vat_id: String,
    pub iban: String,
    pub bic: String,
    pub bank_name: String,
    pub invoice_number_prefix: String,
    pub invoice_footer_note: String,
    pub default_payment_terms_days: i32,
    pub skr: String,
    pub datev_berater_nr: String,
    pub datev_mandant_nr: String,
}

pub async fn get(pool: &PgPool) -> Result<CompanySettings, sqlx::Error> {
    sqlx::query_as!(
        CompanySettings,
        "SELECT company_name, owner_name, address_line1, address_line2, zip, city, country,
                email, phone, tax_id, vat_id, iban, bic, bank_name, invoice_number_prefix,
                next_invoice_number, next_customer_number, invoice_footer_note,
                default_payment_terms_days, skr, datev_berater_nr, datev_mandant_nr
         FROM company_settings WHERE id = 1"
    )
    .fetch_one(pool)
    .await
}

pub async fn update(
    pool: &PgPool,
    update: &CompanySettingsUpdate,
) -> Result<CompanySettings, sqlx::Error> {
    sqlx::query_as!(
        CompanySettings,
        "UPDATE company_settings SET
            company_name = $1, owner_name = $2, address_line1 = $3, address_line2 = $4,
            zip = $5, city = $6, country = $7, email = $8, phone = $9, tax_id = $10,
            vat_id = $11, iban = $12, bic = $13, bank_name = $14, invoice_number_prefix = $15,
            invoice_footer_note = $16, default_payment_terms_days = $17, skr = $18,
            datev_berater_nr = $19, datev_mandant_nr = $20, updated_at = now()
         WHERE id = 1
         RETURNING company_name, owner_name, address_line1, address_line2, zip, city, country,
                   email, phone, tax_id, vat_id, iban, bic, bank_name, invoice_number_prefix,
                   next_invoice_number, next_customer_number, invoice_footer_note,
                   default_payment_terms_days, skr, datev_berater_nr, datev_mandant_nr",
        update.company_name,
        update.owner_name,
        update.address_line1,
        update.address_line2,
        update.zip,
        update.city,
        update.country,
        update.email,
        update.phone,
        update.tax_id,
        update.vat_id,
        update.iban,
        update.bic,
        update.bank_name,
        update.invoice_number_prefix,
        update.invoice_footer_note,
        update.default_payment_terms_days,
        update.skr,
        update.datev_berater_nr,
        update.datev_mandant_nr,
    )
    .fetch_one(pool)
    .await
}
