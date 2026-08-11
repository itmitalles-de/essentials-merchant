use chrono::NaiveDate;
use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct CompanyInfo {
    pub company_name: String,
    pub owner_name: String,
    pub address_line1: String,
    pub address_line2: String,
    pub zip: String,
    pub city: String,
    pub tax_id: String,
    pub vat_id: String,
    pub iban: String,
    pub bic: String,
    pub bank_name: String,
    pub invoice_footer_note: String,
}

#[derive(Debug, Clone)]
pub struct CustomerInfo {
    pub name: String,
    pub contact_person: String,
    pub address_line1: String,
    pub address_line2: String,
    pub zip: String,
    pub city: String,
}

#[derive(Debug, Clone)]
pub struct LineItemRow {
    pub description: String,
    pub quantity: Decimal,
    pub unit: String,
    pub unit_price_net: Decimal,
    pub vat_rate_percent: Decimal,
    pub net_amount: Decimal,
    pub gross_amount: Decimal,
}

#[derive(Debug, Clone)]
pub struct VatBreakdownRow {
    pub rate_percent: Decimal,
    pub net_total: Decimal,
    pub vat_total: Decimal,
    pub gross_total: Decimal,
}

#[derive(Debug, Clone)]
pub struct InvoicePdfInput {
    pub invoice_number: String,
    pub issue_date: NaiveDate,
    pub due_date: NaiveDate,
    pub company: CompanyInfo,
    pub customer: CustomerInfo,
    pub line_items: Vec<LineItemRow>,
    pub vat_breakdown: Vec<VatBreakdownRow>,
    pub net_total: Decimal,
    pub vat_total: Decimal,
    pub gross_total: Decimal,
    pub notes: String,
}

/// German invoicing convention: "." as thousands separator, "," as decimal separator.
pub fn format_money_de(value: Decimal) -> String {
    let rounded = value.round_dp(2);
    let sign = if rounded.is_sign_negative() { "-" } else { "" };
    let abs = rounded.abs().to_string();
    let (int_part, frac_part) = abs.split_once('.').unwrap_or((abs.as_str(), "00"));
    let frac_part = format!("{frac_part:0<2}");

    let mut grouped = String::new();
    for (i, ch) in int_part.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    let int_grouped: String = grouped.chars().rev().collect();

    format!("{sign}{int_grouped},{frac_part}")
}

pub fn format_date_de(date: NaiveDate) -> String {
    date.format("%d.%m.%Y").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn formats_small_amount() {
        assert_eq!(format_money_de(dec!(19.00)), "19,00");
    }

    #[test]
    fn formats_thousands_separator() {
        assert_eq!(format_money_de(dec!(1243.50)), "1.243,50");
    }

    #[test]
    fn formats_millions_with_multiple_separators() {
        assert_eq!(format_money_de(dec!(1234567.89)), "1.234.567,89");
    }

    #[test]
    fn formats_zero() {
        assert_eq!(format_money_de(dec!(0)), "0,00");
    }

    #[test]
    fn formats_negative() {
        assert_eq!(format_money_de(dec!(-50.5)), "-50,50");
    }

    #[test]
    fn formats_date() {
        assert_eq!(
            format_date_de(NaiveDate::from_ymd_opt(2026, 8, 11).unwrap()),
            "11.08.2026"
        );
    }
}
