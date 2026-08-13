use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use db::accounting::AccountingEntry;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_BOOKINGS: usize = 99_999;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatevExportRequest {
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub fiscal_year_start: NaiveDate,
    pub advisor_number: String,
    pub client_number: String,
    pub account_length: u8,
    pub accounting_framework: String,
    pub currency_code: String,
    pub customer_accounts: BTreeMap<i32, String>,
    pub revenue_accounts_by_tax_rate: BTreeMap<String, String>,
    pub tax_keys_by_tax_rate: BTreeMap<String, String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DatevError {
    #[error("period and fiscal-year dates are invalid")]
    InvalidPeriod,
    #[error("advisor, client, account length, framework, or currency is invalid")]
    InvalidHeader,
    #[error("the period has no immutable accounting entries")]
    Empty,
    #[error("the DATEV booking batch limit of 99,999 rows was exceeded")]
    TooManyEntries,
    #[error("entry {0} has no valid customer account mapping")]
    MissingCustomerAccount(String),
    #[error("entry {0} has no valid revenue account mapping")]
    MissingRevenueAccount(String),
    #[error("entry {0} has no valid tax key mapping")]
    MissingTaxKey(String),
    #[error("entry {0} has an invalid amount or currency")]
    InvalidEntry(String),
}

pub fn render_booking_batch(
    request: &DatevExportRequest,
    entries: &[AccountingEntry],
) -> Result<Vec<u8>, DatevError> {
    validate_request(request)?;
    if entries.is_empty() {
        return Err(DatevError::Empty);
    }
    if entries.len() > MAX_BOOKINGS {
        return Err(DatevError::TooManyEntries);
    }

    let generated_at = entries
        .iter()
        .map(|entry| entry.created_at)
        .max()
        .ok_or(DatevError::Empty)?;
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|entry| {
        (
            entry.booking_date,
            entry.document_number.clone(),
            entry.line_position,
            entry.id,
        )
    });

    let mut output = String::from("\u{feff}");
    output.push_str(&header(request, generated_at));
    output.push_str("\r\n");
    output.push_str(&booking_headers().join(";"));
    output.push_str("\r\n");
    for entry in &sorted {
        output.push_str(&booking_row(request, entry)?);
        output.push_str("\r\n");
    }
    Ok(output.into_bytes())
}

fn validate_request(request: &DatevExportRequest) -> Result<(), DatevError> {
    if request.period_end < request.period_start
        || request.fiscal_year_start > request.period_start
        || request.period_start.year() != request.fiscal_year_start.year()
    {
        return Err(DatevError::InvalidPeriod);
    }
    let advisor = numeric_between(&request.advisor_number, 4, 7, 1_001, 9_999_999);
    let client = numeric_between(&request.client_number, 1, 5, 1, 99_999);
    let framework = request.accounting_framework.len() == 2
        && request
            .accounting_framework
            .chars()
            .all(|character| character.is_ascii_digit());
    let currency = request.currency_code.len() == 3
        && request
            .currency_code
            .chars()
            .all(|character| character.is_ascii_uppercase());
    if !advisor || !client || !(4..=8).contains(&request.account_length) || !framework || !currency
    {
        return Err(DatevError::InvalidHeader);
    }
    Ok(())
}

fn numeric_between(value: &str, min_len: usize, max_len: usize, min: u32, max: u32) -> bool {
    (min_len..=max_len).contains(&value.len())
        && value.chars().all(|character| character.is_ascii_digit())
        && value
            .parse::<u32>()
            .is_ok_and(|number| (min..=max).contains(&number))
}

fn valid_account(account: &str, maximum_length: usize) -> bool {
    !account.is_empty()
        && account.len() <= maximum_length
        && account.chars().all(|character| character.is_ascii_digit())
        && account.chars().any(|character| character != '0')
}

fn rate_key(rate: Decimal) -> String {
    format!("{:.2}", rate)
}

fn decimal_comma(value: Decimal) -> String {
    format!("{:.2}", value).replace('.', ",")
}

fn date_yyyymmdd(date: NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

fn date_ddmm(date: NaiveDate) -> String {
    date.format("%d%m").to_string()
}

fn timestamp(date: DateTime<Utc>) -> String {
    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}{:03}",
        date.year(),
        date.month(),
        date.day(),
        date.hour(),
        date.minute(),
        date.second(),
        date.timestamp_subsec_millis()
    )
}

fn text(value: &str, maximum_chars: usize) -> String {
    let sanitized = value
        .chars()
        .filter(|character| !character.is_control())
        .take(maximum_chars)
        .collect::<String>()
        .replace('"', "\"\"");
    format!("\"{sanitized}\"")
}

fn header(request: &DatevExportRequest, generated_at: DateTime<Utc>) -> String {
    vec![
        text("EXTF", 4),
        "700".into(),
        "21".into(),
        text("Buchungsstapel", 30),
        "13".into(),
        timestamp(generated_at),
        String::new(),
        text("EM", 2),
        text("Essentials+ Merchant", 25),
        text("", 25),
        request.advisor_number.clone(),
        request.client_number.clone(),
        date_yyyymmdd(request.fiscal_year_start),
        request.account_length.to_string(),
        date_yyyymmdd(request.period_start),
        date_yyyymmdd(request.period_end),
        text("Rechnungsausgang", 30),
        text("EM", 4),
        "1".into(),
        "0".into(),
        "0".into(),
        text(&request.currency_code, 3),
        String::new(),
        text("", 0),
        String::new(),
        String::new(),
        text(&request.accounting_framework, 2),
        String::new(),
        String::new(),
        text("", 0),
        text("EssentialsPlus", 16),
    ]
    .join(";")
}

fn booking_row(
    request: &DatevExportRequest,
    entry: &AccountingEntry,
) -> Result<String, DatevError> {
    if entry.currency_code != request.currency_code
        || entry.gross_amount.is_zero()
        || entry.gross_amount.abs() > Decimal::new(999_999_999_999, 2)
    {
        return Err(DatevError::InvalidEntry(entry.document_number.clone()));
    }
    let rate = rate_key(entry.tax_rate_percent);
    let customer_account = request
        .customer_accounts
        .get(&entry.customer_number)
        .filter(|account| valid_account(account, request.account_length as usize + 1))
        .ok_or_else(|| DatevError::MissingCustomerAccount(entry.document_number.clone()))?;
    let revenue_account = request
        .revenue_accounts_by_tax_rate
        .get(&rate)
        .filter(|account| valid_account(account, request.account_length as usize))
        .ok_or_else(|| DatevError::MissingRevenueAccount(entry.document_number.clone()))?;
    let tax_key = request
        .tax_keys_by_tax_rate
        .get(&rate)
        .filter(|key| {
            key.len() <= 4
                && !key.is_empty()
                && key.chars().all(|character| character.is_ascii_digit())
        })
        .ok_or_else(|| DatevError::MissingTaxKey(entry.document_number.clone()))?;

    let mut fields = vec![String::new(); 125];
    fields[0] = decimal_comma(entry.gross_amount.abs());
    fields[1] = text(
        if entry.gross_amount.is_sign_negative() {
            "H"
        } else {
            "S"
        },
        1,
    );
    fields[2] = text(&entry.currency_code, 3);
    fields[6] = customer_account.clone();
    fields[7] = revenue_account.clone();
    fields[8] = tax_key.clone();
    fields[9] = date_ddmm(entry.booking_date);
    fields[10] = text(&entry.document_number, 36);
    if let Some(reference) = &entry.corrected_document_number {
        fields[11] = text(reference, 12);
    }
    fields[13] = text(&entry.booking_text, 60);
    fields[114] = date_ddmm(entry.service_date);
    fields[118] = decimal_comma(entry.tax_rate_percent);
    Ok(fields.join(";"))
}

fn booking_headers() -> Vec<String> {
    let mut headers = [
        "Umsatz (ohne Soll/Haben-Kz)",
        "Soll/Haben-Kennzeichen",
        "WKZ Umsatz",
        "Kurs",
        "Basis-Umsatz",
        "WKZ Basis-Umsatz",
        "Konto",
        "Gegenkonto (ohne BU-Schlüssel)",
        "BU-Schlüssel",
        "Belegdatum",
        "Belegfeld 1",
        "Belegfeld 2",
        "Skonto",
        "Buchungstext",
        "Postensperre",
        "Diverse Adressnummer",
        "Geschäftspartnerbank",
        "Sachverhalt",
        "Zinssperre",
        "Beleglink",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    for number in 1..=8 {
        headers.push(format!("Beleginfo - Art {number}"));
        headers.push(format!("Beleginfo - Inhalt {number}"));
    }
    headers.extend(
        [
            "KOST1 - Kostenstelle",
            "KOST2 - Kostenstelle",
            "Kost-Menge",
            "EU-Land u. UStID (Bestimmung)",
            "EU-Steuersatz (Bestimmung)",
            "Abw. Versteuerungsart",
            "Sachverhalt L+L",
            "Funktionsergänzung L+L",
            "BU 49 Hauptfunktionstyp",
            "BU 49 Hauptfunktionsnummer",
            "BU 49 Funktionsergänzung",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    for number in 1..=20 {
        headers.push(format!("Zusatzinformation - Art {number}"));
        headers.push(format!("Zusatzinformation- Inhalt {number}"));
    }
    headers.extend(
        [
            "Stück",
            "Gewicht",
            "Zahlweise",
            "Forderungsart",
            "Veranlagungsjahr",
            "Zugeordnete Fälligkeit",
            "Skontotyp",
            "Auftragsnummer",
            "Buchungstyp",
            "USt-Schlüssel (Anzahlungen)",
            "EU-Land (Anzahlungen)",
            "Sachverhalt L+L (Anzahlungen)",
            "EU-Steuersatz (Anzahlungen)",
            "Erlöskonto (Anzahlungen)",
            "Herkunft-Kz",
            "Buchungs GUID",
            "KOST-Datum",
            "SEPA-Mandatsreferenz",
            "Skontosperre",
            "Gesellschaftername",
            "Beteiligtennummer",
            "Identifikationsnummer",
            "Zeichnernummer",
            "Postensperre bis",
            "Bezeichnung SoBil-Sachverhalt",
            "Kennzeichen SoBil-Buchung",
            "Festschreibung",
            "Leistungsdatum",
            "Datum Zuord. Steuerperiode",
            "Fälligkeit",
            "Generalumkehr (GU)",
            "Steuersatz",
            "Land",
            "Abrechnungsreferenz",
            "BVV-Position",
            "EU-Land u. UStID (Ursprung)",
            "EU-Steuersatz (Ursprung)",
            "Abw. Skontokonto",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    debug_assert_eq!(headers.len(), 125);
    headers
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use uuid::Uuid;

    use super::*;

    fn request() -> DatevExportRequest {
        DatevExportRequest {
            period_start: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            period_end: NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            fiscal_year_start: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            advisor_number: "29098".into(),
            client_number: "55003".into(),
            account_length: 4,
            accounting_framework: "03".into(),
            currency_code: "EUR".into(),
            customer_accounts: BTreeMap::from([(10001, "10001".into())]),
            revenue_accounts_by_tax_rate: BTreeMap::from([("19.00".into(), "8400".into())]),
            tax_keys_by_tax_rate: BTreeMap::from([("19.00".into(), "9".into())]),
        }
    }

    fn entry(number: &str, amount: Decimal, document_type: &str) -> AccountingEntry {
        AccountingEntry {
            id: Uuid::new_v4(),
            invoice_id: Uuid::new_v4(),
            invoice_line_item_id: Uuid::new_v4(),
            document_type: document_type.into(),
            document_number: number.into(),
            corrected_document_number: (document_type == "correction")
                .then(|| "RE-2026-0001".into()),
            customer_number: 10001,
            booking_date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            service_date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            line_position: 1,
            booking_text: "Synthetic \"consulting\"".into(),
            currency_code: "EUR".into(),
            net_amount: amount,
            tax_amount: amount * Decimal::new(19, 2),
            gross_amount: amount * Decimal::new(119, 2),
            tax_rate_percent: Decimal::new(1900, 2),
            source_sha256: "a".repeat(64),
            created_at: Utc.with_ymd_and_hms(2026, 1, 15, 12, 34, 56).unwrap(),
        }
    }

    #[test]
    fn extf_v13_is_bom_crlf_field_complete_and_deterministic() {
        let entries = vec![
            entry("RE-2026-0001", Decimal::new(10000, 2), "invoice"),
            entry("KR-2026-0001", Decimal::new(-10000, 2), "correction"),
        ];
        let first = render_booking_batch(&request(), &entries).unwrap();
        let second = render_booking_batch(&request(), &entries).unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with(&[0xef, 0xbb, 0xbf]));
        let content = String::from_utf8(first).unwrap();
        assert!(!content.replace("\r\n", "").contains('\n'));
        let lines = content.trim_end().split("\r\n").collect::<Vec<_>>();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].split(';').count(), 31);
        assert_eq!(lines[1].split(';').count(), 125);
        assert!(lines[0].contains(";700;21;\"Buchungsstapel\";13;"));
        for row in &lines[2..] {
            assert_eq!(row.split(';').count(), 125);
            assert!(row.contains("119,00"));
            assert!(row.contains("Synthetic \"\"consulting\"\""));
        }
        assert!(lines[2].starts_with("119,00;\"H\";"));
        assert!(lines[2].contains("\"RE-2026-0001\""));
        assert!(lines[3].starts_with("119,00;\"S\";"));
    }

    #[test]
    fn mappings_and_header_values_are_strictly_validated() {
        let mut invalid = request();
        invalid.customer_accounts.clear();
        assert!(matches!(
            render_booking_batch(&invalid, &[entry("RE-1", Decimal::ONE, "invoice")]),
            Err(DatevError::MissingCustomerAccount(_))
        ));
        let mut invalid = request();
        invalid.advisor_number = "1".into();
        assert_eq!(
            render_booking_batch(&invalid, &[entry("RE-1", Decimal::ONE, "invoice")]),
            Err(DatevError::InvalidHeader)
        );
    }
}
