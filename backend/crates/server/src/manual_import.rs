//! Strict, side-effect-free parser for manually supplied Amazon Sales & Traffic reports.
//!
//! This module deliberately does not persist files, create report runs, or call Amazon. The
//! caller owns immutable archival and must only persist the returned preview after all operator
//! confirmations have passed.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use csv::{ReaderBuilder, StringRecord, Trim};
use db::marketplace::{ParsedMetric, ParsedSnapshot, SALES_AND_TRAFFIC};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MAX_MANUAL_REPORT_BYTES: usize = 10 * 1024 * 1024;
pub const MANUAL_SALES_TRAFFIC_PARSER_VERSION: &str = "manual-sales-traffic-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManualReportFormat {
    Json,
    Csv,
    Tsv,
}

/// Operator-confirmable metadata. Values never override report metadata: they must either match a
/// source value or fill a source field which is absent. Callers should parse once without metadata
/// for the upload preview and parse the exact same bytes again with explicit confirmations before
/// persistence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualImportMetadata {
    pub marketplace_id: Option<String>,
    pub period_start: Option<NaiveDate>,
    pub period_end: Option<NaiveDate>,
    pub reporting_timezone: Option<String>,
    pub currency_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataProvenance {
    Report,
    OperatorConfirmed,
    Missing,
}

#[derive(Debug, Clone)]
pub struct ManualImportPreview {
    pub format: ManualReportFormat,
    pub raw_sha256: String,
    pub raw_bytes: usize,
    pub report_type: String,
    pub marketplace_id: Option<String>,
    pub period_start: Option<NaiveDate>,
    pub period_end: Option<NaiveDate>,
    pub date_granularity: String,
    pub asin_granularity: String,
    pub parser_version: &'static str,
    pub reporting_timezone: Option<String>,
    pub timezone_source_note: String,
    pub currency_code: Option<String>,
    pub confirmation_required: bool,
    pub operator_confirmed: Vec<String>,
    pub metadata_provenance: BTreeMap<String, MetadataProvenance>,
    pub missing_fields: Vec<String>,
    pub warnings: Vec<String>,
    pub snapshot: ParsedSnapshot,
}

impl ManualImportPreview {
    /// Only confirmation-complete previews may cross the persistence boundary.
    pub fn ensure_ready_for_import(&self) -> Result<(), ManualImportError> {
        if self.confirmation_required {
            let fields = self
                .metadata_provenance
                .iter()
                .filter(|(_, provenance)| **provenance == MetadataProvenance::Missing)
                .map(|(field, _)| field.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ManualImportError::MetadataRequired(fields));
        }
        Ok(())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ManualImportError {
    #[error("manual report is empty")]
    Empty,
    #[error("manual report is {actual} bytes; maximum is {maximum} bytes")]
    TooLarge { actual: usize, maximum: usize },
    #[error("unsupported report format: {0}")]
    UnsupportedFormat(String),
    #[error("manual report is not valid UTF-8")]
    InvalidUtf8,
    #[error("invalid JSON report: {0}")]
    InvalidJson(String),
    #[error("invalid delimited report: {0}")]
    InvalidDelimited(String),
    #[error("missing required field: {0}")]
    MissingField(String),
    #[error("invalid field {field}: {reason}")]
    InvalidField { field: String, reason: String },
    #[error("operator confirmation is required for {0}")]
    MetadataRequired(String),
    #[error("metadata mismatch for {field}: expected {expected}, found {found}")]
    MetadataMismatch {
        field: String,
        expected: String,
        found: String,
    },
    #[error("report contains a prohibited PII field: {0}")]
    PiiHeader(String),
    #[error("conflicting currencies: expected {expected}, found {found}")]
    CurrencyConflict { expected: String, found: String },
    #[error("duplicate report row: {0}")]
    DuplicateRow(String),
}

#[derive(Debug, Clone, Copy)]
struct DetectedFormat {
    format: ManualReportFormat,
    delimiter: Option<u8>,
}

pub fn parse_manual_sales_and_traffic(
    raw: &[u8],
    metadata: &ManualImportMetadata,
) -> Result<ManualImportPreview, ManualImportError> {
    if raw.is_empty() {
        return Err(ManualImportError::Empty);
    }
    if raw.len() > MAX_MANUAL_REPORT_BYTES {
        return Err(ManualImportError::TooLarge {
            actual: raw.len(),
            maximum: MAX_MANUAL_REPORT_BYTES,
        });
    }
    validate_metadata(metadata)?;
    let detected = detect_format(raw)?;
    let raw_sha256 = sha256_hex(raw);
    match detected.format {
        ManualReportFormat::Json => parse_json_report(raw, metadata, raw_sha256),
        ManualReportFormat::Csv | ManualReportFormat::Tsv => parse_flat_report(
            raw,
            metadata,
            raw_sha256,
            detected.format,
            detected.delimiter.expect("flat format has a delimiter"),
        ),
    }
}

fn detect_format(raw: &[u8]) -> Result<DetectedFormat, ManualImportError> {
    let content = without_bom_and_leading_whitespace(raw);
    if content.is_empty() {
        return Err(ManualImportError::Empty);
    }
    if content.starts_with(b"PK\x03\x04") {
        return Err(ManualImportError::UnsupportedFormat(
            "ZIP archives are not accepted by the manual import".to_owned(),
        ));
    }
    if matches!(content.first(), Some(b'{') | Some(b'[')) {
        return Ok(DetectedFormat {
            format: ManualReportFormat::Json,
            delimiter: None,
        });
    }

    let text = std::str::from_utf8(content).map_err(|_| ManualImportError::InvalidUtf8)?;
    let header = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or(ManualImportError::Empty)?;
    let candidates = [b'\t', b',', b';']
        .into_iter()
        .map(|delimiter| (delimiter, count_unquoted(header.as_bytes(), delimiter)))
        .collect::<Vec<_>>();
    let maximum = candidates
        .iter()
        .map(|(_, count)| *count)
        .max()
        .unwrap_or(0);
    if maximum == 0 {
        return Err(ManualImportError::UnsupportedFormat(
            "expected JSON, comma/semicolon CSV, or TSV".to_owned(),
        ));
    }
    let winners = candidates
        .iter()
        .filter(|(_, count)| *count == maximum)
        .map(|(delimiter, _)| *delimiter)
        .collect::<Vec<_>>();
    if winners.len() != 1 {
        return Err(ManualImportError::UnsupportedFormat(
            "delimiter is ambiguous".to_owned(),
        ));
    }
    let delimiter = winners[0];
    Ok(DetectedFormat {
        format: if delimiter == b'\t' {
            ManualReportFormat::Tsv
        } else {
            ManualReportFormat::Csv
        },
        delimiter: Some(delimiter),
    })
}

fn without_bom_and_leading_whitespace(raw: &[u8]) -> &[u8] {
    let raw = raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(raw);
    let offset = raw
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(raw.len());
    &raw[offset..]
}

fn count_unquoted(line: &[u8], delimiter: u8) -> usize {
    let mut quoted = false;
    let mut count = 0;
    let mut index = 0;
    while index < line.len() {
        match line[index] {
            b'"' if quoted && line.get(index + 1) == Some(&b'"') => index += 1,
            b'"' => quoted = !quoted,
            byte if byte == delimiter && !quoted => count += 1,
            _ => {}
        }
        index += 1;
    }
    count
}

fn validate_metadata(metadata: &ManualImportMetadata) -> Result<(), ManualImportError> {
    match (metadata.period_start, metadata.period_end) {
        (Some(start), Some(end)) if start > end => {
            return Err(ManualImportError::InvalidField {
                field: "period".to_owned(),
                reason: "period_start must not be after period_end".to_owned(),
            });
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(ManualImportError::MetadataRequired(
                "both period_start and period_end".to_owned(),
            ));
        }
        _ => {}
    }
    for (field, value) in [
        ("marketplace_id", metadata.marketplace_id.as_deref()),
        ("reporting_timezone", metadata.reporting_timezone.as_deref()),
        ("currency_code", metadata.currency_code.as_deref()),
    ] {
        if value.is_some_and(|value| {
            value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control)
        }) {
            return Err(ManualImportError::InvalidField {
                field: field.to_owned(),
                reason: "must be non-empty, bounded text without control characters".to_owned(),
            });
        }
    }
    if let Some(currency) = &metadata.currency_code {
        normalize_currency(currency)?;
    }
    if let Some(marketplace_id) = &metadata.marketplace_id {
        validate_marketplace_id(marketplace_id)?;
    }
    Ok(())
}

fn validate_marketplace_id(value: &str) -> Result<(), ManualImportError> {
    if value.len() < 2
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
        || !value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err(ManualImportError::InvalidField {
            field: "marketplace_id".to_owned(),
            reason: "must be an official uppercase Amazon marketplace identifier".to_owned(),
        });
    }
    Ok(())
}

fn parse_json_report(
    raw: &[u8],
    metadata: &ManualImportMetadata,
    raw_sha256: String,
) -> Result<ManualImportPreview, ManualImportError> {
    let value: Value = serde_json::from_slice(without_bom_and_leading_whitespace(raw))
        .map_err(|error| ManualImportError::InvalidJson(error.to_string()))?;
    reject_json_pii_keys(&value, "$")?;
    let root = value
        .as_object()
        .ok_or_else(|| ManualImportError::InvalidJson("root must be an object".to_owned()))?;
    let specification = required_object(root.get("reportSpecification"), "reportSpecification")?;
    let report_type = required_string(
        specification.get("reportType"),
        "reportSpecification.reportType",
    )?;
    if report_type != SALES_AND_TRAFFIC {
        return Err(ManualImportError::InvalidField {
            field: "reportSpecification.reportType".to_owned(),
            reason: format!("expected {SALES_AND_TRAFFIC}, found {report_type}"),
        });
    }

    let marketplace_values = specification
        .get("marketplaceIds")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ManualImportError::MissingField("reportSpecification.marketplaceIds".to_owned())
        })?;
    if marketplace_values.len() != 1 {
        return Err(ManualImportError::InvalidField {
            field: "reportSpecification.marketplaceIds".to_owned(),
            reason: "exactly one marketplace is required".to_owned(),
        });
    }
    let marketplace_id = marketplace_values[0]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ManualImportError::InvalidField {
            field: "reportSpecification.marketplaceIds[0]".to_owned(),
            reason: "must be a non-empty string".to_owned(),
        })?
        .to_owned();
    validate_marketplace_id(&marketplace_id)?;
    confirm_text(
        "marketplace_id",
        metadata.marketplace_id.as_deref(),
        &marketplace_id,
    )?;

    let period_start = parse_json_report_date(
        required_string(
            specification.get("dataStartTime"),
            "reportSpecification.dataStartTime",
        )?,
        "reportSpecification.dataStartTime",
    )?;
    let period_end = parse_json_report_date(
        required_string(
            specification.get("dataEndTime"),
            "reportSpecification.dataEndTime",
        )?,
        "reportSpecification.dataEndTime",
    )?;
    if period_start > period_end {
        return Err(ManualImportError::InvalidField {
            field: "reportSpecification period".to_owned(),
            reason: "dataStartTime must not be after dataEndTime".to_owned(),
        });
    }
    confirm_date("period_start", metadata.period_start, period_start)?;
    confirm_date("period_end", metadata.period_end, period_end)?;

    let options = required_object(
        specification.get("reportOptions"),
        "reportSpecification.reportOptions",
    )?;
    let date_granularity = options
        .get("dateGranularity")
        .map(|value| {
            required_string(
                Some(value),
                "reportSpecification.reportOptions.dateGranularity",
            )
        })
        .transpose()?
        .unwrap_or("DAY");
    if !matches!(date_granularity, "DAY" | "WEEK" | "MONTH") {
        return Err(ManualImportError::InvalidField {
            field: "reportSpecification.reportOptions.dateGranularity".to_owned(),
            reason: format!("unsupported value {date_granularity}"),
        });
    }
    let asin_granularity = options
        .get("asinGranularity")
        .map(|value| {
            required_string(
                Some(value),
                "reportSpecification.reportOptions.asinGranularity",
            )
        })
        .transpose()?
        .unwrap_or("PARENT");
    if !matches!(asin_granularity, "PARENT" | "CHILD" | "SKU") {
        return Err(ManualImportError::InvalidField {
            field: "reportSpecification.reportOptions.asinGranularity".to_owned(),
            reason: format!("unsupported value {asin_granularity}"),
        });
    }

    let date_rows = required_array(root.get("salesAndTrafficByDate"), "salesAndTrafficByDate")?;
    let asin_rows = required_array(root.get("salesAndTrafficByAsin"), "salesAndTrafficByAsin")?;
    if date_rows.is_empty() && asin_rows.is_empty() {
        return Err(ManualImportError::MissingField(
            "non-empty salesAndTrafficByDate or salesAndTrafficByAsin".to_owned(),
        ));
    }

    let mut totals = Totals::default();
    let mut warnings = Vec::new();
    if !date_rows.is_empty() {
        let mut seen = HashSet::new();
        for (index, row) in date_rows.iter().enumerate() {
            let object = required_object(Some(row), &format!("salesAndTrafficByDate[{index}]"))?;
            let date = parse_flat_date(
                required_string(
                    object.get("date"),
                    &format!("salesAndTrafficByDate[{index}].date"),
                )?,
                &format!("salesAndTrafficByDate[{index}].date"),
            )?;
            if !date_bucket_is_covered(date, period_start, period_end, date_granularity) {
                return Err(ManualImportError::InvalidField {
                    field: format!("salesAndTrafficByDate[{index}].date"),
                    reason: "date bucket is outside the reportSpecification period".to_owned(),
                });
            }
            if !seen.insert(date) {
                return Err(ManualImportError::DuplicateRow(format!("date {date}")));
            }
            totals.add(parse_json_metric_row(
                object,
                "salesByDate",
                "trafficByDate",
                &format!("salesAndTrafficByDate[{index}]"),
            )?)?;
        }
        if !asin_rows.is_empty() {
            validate_json_asin_rows(asin_rows, asin_granularity, &mut totals)?;
            warnings.push(
                "Catalog totals use salesAndTrafficByDate; ASIN rows were validated but not double-counted."
                    .to_owned(),
            );
        }
    } else {
        let mut seen = HashSet::new();
        for (index, row) in asin_rows.iter().enumerate() {
            let object = required_object(Some(row), &format!("salesAndTrafficByAsin[{index}]"))?;
            let dimension = json_asin_dimension(object, asin_granularity, index)?;
            if !seen.insert(dimension) {
                return Err(ManualImportError::DuplicateRow(format!(
                    "salesAndTrafficByAsin[{index}] duplicates a prior {asin_granularity} dimension"
                )));
            }
            totals.add(parse_json_metric_row(
                object,
                "salesByAsin",
                "trafficByAsin",
                &format!("salesAndTrafficByAsin[{index}]"),
            )?)?;
        }
    }

    let currency = totals.currency.clone().ok_or_else(|| {
        ManualImportError::MissingField("orderedProductSales.currencyCode".to_owned())
    })?;
    confirm_currency(metadata.currency_code.as_deref(), &currency)?;
    let timezone = metadata
        .reporting_timezone
        .as_deref()
        .map(str::trim)
        .map(str::to_owned);
    let mut metadata_provenance = BTreeMap::from([
        ("marketplace_id".to_owned(), MetadataProvenance::Report),
        ("period_start".to_owned(), MetadataProvenance::Report),
        ("period_end".to_owned(), MetadataProvenance::Report),
        ("currency_code".to_owned(), MetadataProvenance::Report),
    ]);
    let mut operator_confirmed = Vec::new();
    let timezone_source_note = if timezone.is_some() {
        metadata_provenance.insert(
            "reporting_timezone".to_owned(),
            MetadataProvenance::OperatorConfirmed,
        );
        operator_confirmed.push("reporting_timezone".to_owned());
        warnings.push("operator_confirmed: reporting_timezone".to_owned());
        "Reporting timezone supplied and confirmed by the operator; Amazon report dates are date-only."
            .to_owned()
    } else {
        metadata_provenance.insert("reporting_timezone".to_owned(), MetadataProvenance::Missing);
        warnings.push(
            "Amazon JSON dates are date-only; confirm the marketplace reporting timezone before import."
                .to_owned(),
        );
        "Amazon JSON does not provide an authoritative reporting timezone; operator confirmation is pending."
            .to_owned()
    };
    build_preview(PreviewInput {
        format: ManualReportFormat::Json,
        raw_sha256,
        raw_bytes: raw.len(),
        marketplace_id: Some(marketplace_id),
        period_start: Some(period_start),
        period_end: Some(period_end),
        date_granularity: date_granularity.to_owned(),
        asin_granularity: asin_granularity.to_owned(),
        reporting_timezone: timezone,
        timezone_source_note,
        currency_code: Some(currency),
        operator_confirmed,
        metadata_provenance,
        warnings,
        totals,
    })
}

fn date_bucket_is_covered(
    date: NaiveDate,
    period_start: NaiveDate,
    period_end: NaiveDate,
    date_granularity: &str,
) -> bool {
    if date > period_end {
        return false;
    }
    let earliest_bucket_start = match date_granularity {
        "DAY" => period_start,
        "WEEK" => period_start
            .checked_sub_signed(Duration::days(6))
            .unwrap_or(NaiveDate::MIN),
        "MONTH" => period_start
            .with_day(1)
            .expect("a valid date has a first day of month"),
        _ => return false,
    };
    date >= earliest_bucket_start
}

fn reject_json_pii_keys(value: &Value, path: &str) -> Result<(), ManualImportError> {
    match value {
        Value::Object(object) => {
            for (key, nested) in object {
                let nested_path = format!("{path}.{key}");
                if pii_header(&normalize_header(key)) {
                    return Err(ManualImportError::PiiHeader(nested_path));
                }
                reject_json_pii_keys(nested, &nested_path)?;
            }
        }
        Value::Array(values) => {
            for (index, nested) in values.iter().enumerate() {
                reject_json_pii_keys(nested, &format!("{path}[{index}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_json_asin_rows(
    rows: &[Value],
    asin_granularity: &str,
    totals: &mut Totals,
) -> Result<(), ManualImportError> {
    let mut seen = HashSet::new();
    for (index, row) in rows.iter().enumerate() {
        let object = required_object(Some(row), &format!("salesAndTrafficByAsin[{index}]"))?;
        let dimension = json_asin_dimension(object, asin_granularity, index)?;
        if !seen.insert(dimension) {
            return Err(ManualImportError::DuplicateRow(format!(
                "salesAndTrafficByAsin[{index}] duplicates a prior {asin_granularity} dimension"
            )));
        }
        let parsed = parse_json_metric_row(
            object,
            "salesByAsin",
            "trafficByAsin",
            &format!("salesAndTrafficByAsin[{index}]"),
        )?;
        if let Some(currency) = &parsed.currency {
            totals.register_currency(currency)?;
        }
        if let Some((_, Some(currency))) = &parsed.b2b_sales {
            totals.register_currency(currency)?;
        }
    }
    Ok(())
}

fn json_asin_dimension(
    object: &serde_json::Map<String, Value>,
    granularity: &str,
    index: usize,
) -> Result<String, ManualImportError> {
    let field = match granularity {
        "PARENT" => "parentAsin",
        "CHILD" => "childAsin",
        "SKU" => "sku",
        _ => unreachable!(),
    };
    required_string(
        object.get(field),
        &format!("salesAndTrafficByAsin[{index}].{field}"),
    )
    .map(str::to_owned)
}

fn parse_json_metric_row(
    row: &serde_json::Map<String, Value>,
    sales_field: &str,
    traffic_field: &str,
    path: &str,
) -> Result<RowMetrics, ManualImportError> {
    let sales = required_object(row.get(sales_field), &format!("{path}.{sales_field}"))?;
    let (sales_value, currency) = parse_json_money(
        sales.get("orderedProductSales"),
        &format!("{path}.{sales_field}.orderedProductSales"),
    )?;
    let units = parse_json_count(
        sales.get("unitsOrdered"),
        &format!("{path}.{sales_field}.unitsOrdered"),
        true,
    )?
    .expect("required units return a value");
    let b2b_sales = parse_optional_json_money(
        sales.get("orderedProductSalesB2B"),
        &format!("{path}.{sales_field}.orderedProductSalesB2B"),
    )?;
    let b2b_units = parse_json_count(
        sales.get("unitsOrderedB2B"),
        &format!("{path}.{sales_field}.unitsOrderedB2B"),
        false,
    )?;
    let traffic = match row.get(traffic_field) {
        None | Some(Value::Null) => None,
        Some(value) => Some(required_object(
            Some(value),
            &format!("{path}.{traffic_field}"),
        )?),
    };
    let optional_traffic = |name: &str| -> Result<Option<Decimal>, ManualImportError> {
        parse_json_count(
            traffic.and_then(|traffic| traffic.get(name)),
            &format!("{path}.{traffic_field}.{name}"),
            false,
        )
    };
    let optional_percent =
        |name: &str, bounded_to_100: bool| -> Result<Option<Decimal>, ManualImportError> {
            parse_json_percent(
                traffic.and_then(|traffic| traffic.get(name)),
                &format!("{path}.{traffic_field}.{name}"),
                bounded_to_100,
            )
        };
    Ok(RowMetrics {
        sales: sales_value,
        currency: Some(currency),
        units,
        sessions: optional_traffic("sessions")?,
        page_views: optional_traffic("pageViews")?,
        reported_conversion: optional_percent("unitSessionPercentage", false)?,
        buy_box_percentage: optional_percent("buyBoxPercentage", true)?,
        b2b_sales: b2b_sales.map(|(value, currency)| (value, Some(currency))),
        b2b_units,
    })
}

fn parse_json_money(
    value: Option<&Value>,
    path: &str,
) -> Result<(Decimal, String), ManualImportError> {
    let object = required_object(value, path)?;
    let amount = parse_json_decimal(object.get("amount"), &format!("{path}.amount"), true)?
        .expect("required amount returns a value");
    let currency = normalize_currency(required_string(
        object.get("currencyCode"),
        &format!("{path}.currencyCode"),
    )?)?;
    Ok((amount, currency))
}

fn parse_optional_json_money(
    value: Option<&Value>,
    path: &str,
) -> Result<Option<(Decimal, String)>, ManualImportError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => parse_json_money(Some(value), path).map(Some),
    }
}

fn parse_json_count(
    value: Option<&Value>,
    path: &str,
    required: bool,
) -> Result<Option<Decimal>, ManualImportError> {
    let value = parse_json_decimal(value, path, required)?;
    if value.is_some_and(|value| value.fract() != Decimal::ZERO) {
        return Err(ManualImportError::InvalidField {
            field: path.to_owned(),
            reason: "must be a whole number".to_owned(),
        });
    }
    Ok(value)
}

fn parse_json_percent(
    value: Option<&Value>,
    path: &str,
    bounded_to_100: bool,
) -> Result<Option<Decimal>, ManualImportError> {
    let value = parse_json_decimal(value, path, false)?;
    if bounded_to_100 && value.is_some_and(|value| value > Decimal::from(100)) {
        return Err(ManualImportError::InvalidField {
            field: path.to_owned(),
            reason: "percentage must be between 0 and 100".to_owned(),
        });
    }
    Ok(value)
}

fn parse_json_decimal(
    value: Option<&Value>,
    path: &str,
    required: bool,
) -> Result<Option<Decimal>, ManualImportError> {
    let Some(value) = value else {
        return if required {
            Err(ManualImportError::MissingField(path.to_owned()))
        } else {
            Ok(None)
        };
    };
    if value.is_null() && !required {
        return Ok(None);
    }
    let text = match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => {
            return Err(ManualImportError::InvalidField {
                field: path.to_owned(),
                reason: "must be a JSON number or decimal string".to_owned(),
            });
        }
    };
    let decimal = Decimal::from_str_exact(&text).map_err(|_| ManualImportError::InvalidField {
        field: path.to_owned(),
        reason: "must be an exact decimal".to_owned(),
    })?;
    if decimal < Decimal::ZERO {
        return Err(ManualImportError::InvalidField {
            field: path.to_owned(),
            reason: "must not be negative".to_owned(),
        });
    }
    Ok(Some(decimal))
}

fn required_object<'a>(
    value: Option<&'a Value>,
    path: &str,
) -> Result<&'a serde_json::Map<String, Value>, ManualImportError> {
    value
        .and_then(Value::as_object)
        .ok_or_else(|| ManualImportError::MissingField(path.to_owned()))
}

fn required_string<'a>(value: Option<&'a Value>, path: &str) -> Result<&'a str, ManualImportError> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ManualImportError::MissingField(path.to_owned()))
}

fn required_array<'a>(
    value: Option<&'a Value>,
    path: &str,
) -> Result<&'a [Value], ManualImportError> {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| ManualImportError::MissingField(path.to_owned()))
}

fn parse_json_report_date(value: &str, path: &str) -> Result<NaiveDate, ManualImportError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .or_else(|_| DateTime::parse_from_rfc3339(value).map(|date| date.date_naive()))
        .map_err(|_| ManualImportError::InvalidField {
            field: path.to_owned(),
            reason: "must be an ISO date or RFC 3339 timestamp".to_owned(),
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FlatField {
    Date,
    Marketplace,
    ChildAsin,
    ParentAsin,
    Sku,
    Revenue,
    Currency,
    Units,
    Sessions,
    PageViews,
    Conversion,
    BuyBox,
    B2bRevenue,
    B2bUnits,
}

fn parse_flat_report(
    raw: &[u8],
    metadata: &ManualImportMetadata,
    raw_sha256: String,
    format: ManualReportFormat,
    delimiter: u8,
) -> Result<ManualImportPreview, ManualImportError> {
    let confirmed_currency = metadata
        .currency_code
        .as_deref()
        .map(normalize_currency)
        .transpose()?;

    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(false)
        .trim(Trim::All)
        .from_reader(raw);
    let headers = reader
        .headers()
        .map_err(|error| ManualImportError::InvalidDelimited(error.to_string()))?
        .clone();
    if headers.is_empty() {
        return Err(ManualImportError::MissingField("header row".to_owned()));
    }
    let (columns, unknown_columns) = map_flat_headers(&headers)?;
    for required in [FlatField::Revenue, FlatField::Units] {
        if !columns.contains_key(&required) {
            return Err(ManualImportError::MissingField(match required {
                FlatField::Revenue => "ordered product sales column".to_owned(),
                FlatField::Units => "units ordered column".to_owned(),
                _ => unreachable!(),
            }));
        }
    }
    if [
        FlatField::Sessions,
        FlatField::PageViews,
        FlatField::Conversion,
        FlatField::BuyBox,
    ]
    .iter()
    .all(|field| !columns.contains_key(field))
    {
        return Err(ManualImportError::MissingField(
            "Sales and Traffic traffic column (sessions, page views, conversion, or buy box)"
                .to_owned(),
        ));
    }

    let dimension = [
        (FlatField::ChildAsin, "CHILD"),
        (FlatField::ParentAsin, "PARENT"),
        (FlatField::Sku, "SKU"),
    ]
    .into_iter()
    .find(|(field, _)| columns.contains_key(field));
    let asin_granularity = dimension.map(|(_, name)| name).unwrap_or("TOTAL");
    let date_granularity = if columns.contains_key(&FlatField::Date) {
        "DAY"
    } else {
        "PERIOD"
    };

    let mut totals = Totals::default();
    let mut seen = HashSet::new();
    let mut row_count = 0usize;
    let mut row_dates = Vec::new();
    let mut source_marketplaces = BTreeSet::new();
    let mut source_currency_seen = false;
    let mut operator_currency_used = false;
    let mut unresolved_currency = false;
    for (offset, record) in reader.records().enumerate() {
        let record = record.map_err(|error| {
            ManualImportError::InvalidDelimited(format!("row {}: {error}", offset + 2))
        })?;
        if record.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        let row_number = offset + 2;
        let date = columns
            .get(&FlatField::Date)
            .map(|index| {
                let value = required_cell(&record, *index, row_number, "date")?;
                parse_flat_date(value, &format!("row {row_number} date"))
            })
            .transpose()?;
        if let Some(date) = date {
            row_dates.push(date);
        }
        if let Some(index) = columns.get(&FlatField::Marketplace) {
            let found = required_cell(&record, *index, row_number, "marketplace")?;
            source_marketplaces.insert(found.to_owned());
        }
        let dimension_value = dimension
            .map(|(field, _)| {
                required_cell(&record, columns[&field], row_number, "ASIN/SKU dimension")
                    .map(str::to_owned)
            })
            .transpose()?;
        let dedupe_key = format!(
            "{}|{}",
            date.map(|date| date.to_string())
                .unwrap_or_else(|| "period".to_owned()),
            dimension_value.as_deref().unwrap_or("total")
        );
        if !seen.insert(dedupe_key) {
            return Err(ManualImportError::DuplicateRow(format!(
                "row {row_number} duplicates a prior date/dimension key"
            )));
        }

        let revenue_cell = required_cell(
            &record,
            columns[&FlatField::Revenue],
            row_number,
            "ordered product sales",
        )?;
        let (sales, money_currency) = parse_money_cell(
            revenue_cell,
            &format!("row {row_number} ordered product sales"),
        )?;
        let column_currency = columns
            .get(&FlatField::Currency)
            .and_then(|index| optional_cell(&record, *index))
            .map(normalize_currency)
            .transpose()?;
        let source_currency =
            reconcile_currencies([column_currency.as_deref(), money_currency.as_deref()])?;
        let currency = if let Some(source_currency) = source_currency {
            source_currency_seen = true;
            confirm_currency(confirmed_currency.as_deref(), &source_currency)?;
            Some(source_currency)
        } else if let Some(confirmed_currency) = &confirmed_currency {
            operator_currency_used = true;
            Some(confirmed_currency.clone())
        } else {
            unresolved_currency = true;
            None
        };
        let units = parse_count_cell(
            required_cell(
                &record,
                columns[&FlatField::Units],
                row_number,
                "units ordered",
            )?,
            &format!("row {row_number} units ordered"),
        )?;
        let sessions = optional_flat_decimal(
            &record,
            columns.get(&FlatField::Sessions).copied(),
            row_number,
            "sessions",
            true,
        )?;
        let page_views = optional_flat_decimal(
            &record,
            columns.get(&FlatField::PageViews).copied(),
            row_number,
            "page views",
            true,
        )?;
        let reported_conversion = optional_flat_percent(
            &record,
            columns.get(&FlatField::Conversion).copied(),
            row_number,
            "unit session percentage",
            false,
        )?;
        let buy_box_percentage = optional_flat_percent(
            &record,
            columns.get(&FlatField::BuyBox).copied(),
            row_number,
            "buy box percentage",
            true,
        )?;
        let b2b_sales = columns
            .get(&FlatField::B2bRevenue)
            .map(|index| {
                let cell = required_cell(&record, *index, row_number, "B2B ordered product sales")?;
                let (value, hint) =
                    parse_money_cell(cell, &format!("row {row_number} B2B ordered product sales"))?;
                let source = reconcile_currencies([
                    column_currency.as_deref(),
                    money_currency.as_deref(),
                    hint.as_deref(),
                ])?;
                let currency = if let Some(source) = source {
                    confirm_currency(confirmed_currency.as_deref(), &source)?;
                    Some(source)
                } else {
                    confirmed_currency.clone()
                };
                Ok::<_, ManualImportError>((value, currency))
            })
            .transpose()?;
        let b2b_units = columns
            .get(&FlatField::B2bUnits)
            .map(|index| {
                parse_count_cell(
                    required_cell(&record, *index, row_number, "B2B units ordered")?,
                    &format!("row {row_number} B2B units ordered"),
                )
            })
            .transpose()?;
        totals.add(RowMetrics {
            sales,
            currency,
            units,
            sessions,
            page_views,
            reported_conversion,
            buy_box_percentage,
            b2b_sales,
            b2b_units,
        })?;
        row_count += 1;
    }
    if row_count == 0 {
        return Err(ManualImportError::MissingField("data rows".to_owned()));
    }

    let mut warnings = Vec::new();
    if unknown_columns > 0 {
        warnings.push(format!(
            "Ignored {unknown_columns} unrecognized aggregate column(s); no row values were copied."
        ));
    }
    if source_marketplaces.len() > 1 {
        return Err(ManualImportError::InvalidField {
            field: "marketplace_id".to_owned(),
            reason: "flat report contains more than one marketplace".to_owned(),
        });
    }
    let source_marketplace = source_marketplaces.into_iter().next();
    let (marketplace_id, marketplace_provenance) = resolve_text_metadata(
        "marketplace_id",
        source_marketplace,
        metadata.marketplace_id.as_deref(),
    )?;
    if let Some(marketplace_id) = &marketplace_id {
        validate_marketplace_id(marketplace_id)?;
    }

    let source_period = if row_dates.is_empty() {
        None
    } else {
        Some((
            *row_dates
                .iter()
                .min()
                .expect("non-empty dates have a minimum"),
            *row_dates
                .iter()
                .max()
                .expect("non-empty dates have a maximum"),
        ))
    };
    let (period_start, period_end, period_provenance) =
        resolve_period_metadata(source_period, metadata.period_start, metadata.period_end)?;
    let reporting_timezone = metadata
        .reporting_timezone
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let timezone_provenance = if reporting_timezone.is_some() {
        MetadataProvenance::OperatorConfirmed
    } else {
        MetadataProvenance::Missing
    };
    let currency_code = if unresolved_currency {
        None
    } else {
        totals.currency.clone()
    };
    let currency_provenance = if currency_code.is_none() {
        MetadataProvenance::Missing
    } else if operator_currency_used || !source_currency_seen {
        MetadataProvenance::OperatorConfirmed
    } else {
        MetadataProvenance::Report
    };

    let metadata_provenance = BTreeMap::from([
        ("marketplace_id".to_owned(), marketplace_provenance),
        ("period_start".to_owned(), period_provenance),
        ("period_end".to_owned(), period_provenance),
        ("reporting_timezone".to_owned(), timezone_provenance),
        ("currency_code".to_owned(), currency_provenance),
    ]);
    let operator_confirmed = metadata_provenance
        .iter()
        .filter(|(_, provenance)| **provenance == MetadataProvenance::OperatorConfirmed)
        .map(|(field, _)| field.clone())
        .collect::<Vec<_>>();
    if !operator_confirmed.is_empty() {
        warnings.push(format!(
            "operator_confirmed: {}",
            operator_confirmed.join(", ")
        ));
    }
    let timezone_source_note = if reporting_timezone.is_some() {
        "Reporting timezone is operator-confirmed because flat exports do not provide an authoritative timezone."
            .to_owned()
    } else {
        warnings.push(
            "Flat report has no authoritative reporting timezone; operator confirmation is required before import."
                .to_owned(),
        );
        "Flat report has no authoritative reporting timezone; operator confirmation is pending."
            .to_owned()
    };
    build_preview(PreviewInput {
        format,
        raw_sha256,
        raw_bytes: raw.len(),
        marketplace_id,
        period_start,
        period_end,
        date_granularity: date_granularity.to_owned(),
        asin_granularity: asin_granularity.to_owned(),
        reporting_timezone,
        timezone_source_note,
        currency_code,
        operator_confirmed,
        metadata_provenance,
        warnings,
        totals,
    })
}

fn map_flat_headers(
    headers: &StringRecord,
) -> Result<(HashMap<FlatField, usize>, usize), ManualImportError> {
    let mut columns = HashMap::new();
    let mut unknown = 0;
    for (index, raw_header) in headers.iter().enumerate() {
        let header = raw_header.trim_start_matches('\u{feff}').trim();
        let normalized = normalize_header(header);
        if pii_header(&normalized) {
            return Err(ManualImportError::PiiHeader(header.to_owned()));
        }
        if let Some(field) = flat_header_alias(&normalized) {
            if columns.insert(field, index).is_some() {
                return Err(ManualImportError::InvalidDelimited(format!(
                    "duplicate semantic column: {header}"
                )));
            }
        } else {
            unknown += 1;
        }
    }
    Ok((columns, unknown))
}

fn normalize_header(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn flat_header_alias(value: &str) -> Option<FlatField> {
    Some(match value {
        "date" | "reportdate" | "datum" => FlatField::Date,
        "marketplace" | "marketplaceid" | "marktplatz" | "marktplatzid" => FlatField::Marketplace,
        "childasin" | "asinchild" | "untergeordneteasin" => FlatField::ChildAsin,
        "parentasin" | "asinparent" | "übergeordneteasin" | "uebergeordneteasin" => {
            FlatField::ParentAsin
        }
        "sku" | "händlersku" | "haendlersku" => FlatField::Sku,
        "orderedproductsales"
        | "orderedproductsalesamount"
        | "revenue"
        | "sales"
        | "umsatz"
        | "umsatzbestellterprodukte"
        | "bestellterproduktumsatz" => FlatField::Revenue,
        "currency" | "currencycode" | "währung" | "waehrung" => FlatField::Currency,
        "unitsordered" | "orderedunits" | "bestellteeinheiten" | "bestellteprodukte"
        | "einheitenbestellt" => FlatField::Units,
        "sessions" | "sessionstotal" | "sitzungen" | "sitzungengesamt" => FlatField::Sessions,
        "pageviews" | "pageviewstotal" | "seitenaufrufe" | "seitenaufrufegesamt" => {
            FlatField::PageViews
        }
        "unitsessionpercentage"
        | "unitsessionpercent"
        | "conversion"
        | "conversionrate"
        | "prozentsatzdereinheitenprositzung" => FlatField::Conversion,
        "buyboxpercentage"
        | "buyboxpercent"
        | "buybox"
        | "featuredofferbuyboxpercentage"
        | "prozentsatzfüreinkaufswagenfeld"
        | "prozentsatzfuereinkaufswagenfeld" => FlatField::BuyBox,
        "orderedproductsalesb2b"
        | "b2borderedproductsales"
        | "b2bsales"
        | "b2brevenue"
        | "b2bumsatz"
        | "umsatzbestellterprodukteb2b" => FlatField::B2bRevenue,
        "unitsorderedb2b" | "b2bunitsordered" | "b2bunits" | "bestellteeinheitenb2b" => {
            FlatField::B2bUnits
        }
        _ => return None,
    })
}

fn pii_header(value: &str) -> bool {
    [
        "buyer",
        "customer",
        "recipient",
        "email",
        "phone",
        "address",
        "orderid",
        "purchaseorder",
        "comment",
        "postalcode",
        "käufer",
        "kaeufer",
        "kunde",
        "kunden",
        "empfänger",
        "empfaenger",
        "telefon",
        "adresse",
        "bestellnummer",
        "bestellid",
        "kommentar",
        "postleitzahl",
    ]
    .iter()
    .any(|forbidden| value.contains(forbidden))
}

fn required_cell<'a>(
    record: &'a StringRecord,
    index: usize,
    row: usize,
    field: &str,
) -> Result<&'a str, ManualImportError> {
    record
        .get(index)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ManualImportError::MissingField(format!("row {row} {field}")))
}

fn optional_flat_decimal(
    record: &StringRecord,
    index: Option<usize>,
    row: usize,
    field: &str,
    whole_number: bool,
) -> Result<Option<Decimal>, ManualImportError> {
    let Some(index) = index else {
        return Ok(None);
    };
    let Some(value) = record
        .get(index)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let value = parse_locale_decimal(value, &format!("row {row} {field}"), false)?;
    if whole_number && value.fract() != Decimal::ZERO {
        return Err(ManualImportError::InvalidField {
            field: format!("row {row} {field}"),
            reason: "must be a whole number".to_owned(),
        });
    }
    Ok(Some(value))
}

fn optional_flat_percent(
    record: &StringRecord,
    index: Option<usize>,
    row: usize,
    field: &str,
    bounded_to_100: bool,
) -> Result<Option<Decimal>, ManualImportError> {
    let Some(index) = index else {
        return Ok(None);
    };
    let Some(value) = record
        .get(index)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let value = parse_locale_decimal(value, &format!("row {row} {field}"), true)?;
    if bounded_to_100 && value > Decimal::from(100) {
        return Err(ManualImportError::InvalidField {
            field: format!("row {row} {field}"),
            reason: "percentage must be between 0 and 100".to_owned(),
        });
    }
    Ok(Some(value))
}

fn parse_count_cell(value: &str, field: &str) -> Result<Decimal, ManualImportError> {
    let value = parse_locale_decimal(value, field, false)?;
    if value.fract() != Decimal::ZERO {
        return Err(ManualImportError::InvalidField {
            field: field.to_owned(),
            reason: "must be a whole number".to_owned(),
        });
    }
    Ok(value)
}

fn parse_money_cell(
    value: &str,
    field: &str,
) -> Result<(Decimal, Option<String>), ManualImportError> {
    let mut text = value.trim().to_owned();
    let mut currencies = Vec::new();
    for (symbol, currency) in [('€', "EUR"), ('£', "GBP")] {
        if text.contains(symbol) {
            text = text.replace(symbol, "");
            currencies.push(currency.to_owned());
        }
    }
    text = text.replace('$', "");
    let parts = text.split_whitespace().collect::<Vec<_>>();
    if let Some(first) = parts.first().filter(|part| is_currency_token(part)) {
        currencies.push(normalize_currency(first)?);
        text = parts[1..].join("");
    } else if let Some(last) = parts.last().filter(|part| is_currency_token(part)) {
        currencies.push(normalize_currency(last)?);
        text = parts[..parts.len() - 1].join("");
    }
    currencies.sort();
    currencies.dedup();
    if currencies.len() > 1 {
        return Err(ManualImportError::CurrencyConflict {
            expected: currencies[0].clone(),
            found: currencies[1].clone(),
        });
    }
    let amount = parse_locale_decimal(&text, field, false)?;
    Ok((amount, currencies.pop()))
}

fn is_currency_token(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn parse_locale_decimal(
    value: &str,
    field: &str,
    allow_percent: bool,
) -> Result<Decimal, ManualImportError> {
    let mut value = value.trim().to_owned();
    if allow_percent {
        value = value.trim_end_matches('%').trim().to_owned();
    } else if value.contains('%') {
        return Err(ManualImportError::InvalidField {
            field: field.to_owned(),
            reason: "unexpected percent sign".to_owned(),
        });
    }
    let negative_parentheses = value.starts_with('(') && value.ends_with(')');
    if negative_parentheses {
        value = value[1..value.len() - 1].to_owned();
    }
    value.retain(|character| !character.is_whitespace() && character != '\'' && character != '’');
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_digit() || matches!(character, '+' | '-' | '.' | ',')
        })
    {
        return Err(ManualImportError::InvalidField {
            field: field.to_owned(),
            reason: "must be a locale-formatted decimal".to_owned(),
        });
    }

    let sign = if negative_parentheses { "-" } else { "" };
    let unsigned = value.trim_start_matches(['+', '-']);
    let explicit_negative = value.starts_with('-');
    if unsigned.is_empty() || !unsigned.chars().any(|character| character.is_ascii_digit()) {
        return Err(ManualImportError::InvalidField {
            field: field.to_owned(),
            reason: "must contain digits".to_owned(),
        });
    }
    let normalized = normalize_decimal_separators(unsigned, field)?;
    let signed = format!(
        "{}{}",
        if explicit_negative || negative_parentheses {
            "-"
        } else {
            sign
        },
        normalized
    );
    let decimal =
        Decimal::from_str_exact(&signed).map_err(|_| ManualImportError::InvalidField {
            field: field.to_owned(),
            reason: "must be an exact decimal".to_owned(),
        })?;
    if decimal < Decimal::ZERO {
        return Err(ManualImportError::InvalidField {
            field: field.to_owned(),
            reason: "must not be negative".to_owned(),
        });
    }
    Ok(decimal)
}

fn normalize_decimal_separators(value: &str, field: &str) -> Result<String, ManualImportError> {
    let comma_count = value.matches(',').count();
    let dot_count = value.matches('.').count();
    if comma_count > 0 && dot_count > 0 {
        let comma_position = value.rfind(',').unwrap();
        let dot_position = value.rfind('.').unwrap();
        let (decimal, grouping) = if comma_position > dot_position {
            (',', '.')
        } else {
            ('.', ',')
        };
        if value.matches(decimal).count() != 1 {
            return invalid_decimal_grouping(field);
        }
        let (integer, fraction) = value.rsplit_once(decimal).unwrap();
        validate_grouped_integer(integer, grouping, field)?;
        if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            return invalid_decimal_grouping(field);
        }
        return Ok(format!("{}.{}", integer.replace(grouping, ""), fraction));
    }
    let (separator, count) = if comma_count > 0 {
        (',', comma_count)
    } else {
        ('.', dot_count)
    };
    if count == 0 {
        return value
            .bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| value.to_owned())
            .ok_or_else(|| ManualImportError::InvalidField {
                field: field.to_owned(),
                reason: "must contain only digits".to_owned(),
            });
    }
    if count > 1 {
        let groups = value.split(separator).collect::<Vec<_>>();
        if groups[0].is_empty()
            || groups[0].len() > 3
            || !groups[0].bytes().all(|byte| byte.is_ascii_digit())
            || !groups[1..]
                .iter()
                .all(|group| group.len() == 3 && group.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return invalid_decimal_grouping(field);
        }
        return Ok(groups.join(""));
    }
    let (integer, fraction) = value.split_once(separator).unwrap();
    if integer.is_empty()
        || fraction.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return invalid_decimal_grouping(field);
    }
    if fraction.len() == 3 && integer.len() <= 3 && integer != "0" {
        return Err(ManualImportError::InvalidField {
            field: field.to_owned(),
            reason: format!("value {value} is ambiguous between a decimal and thousands grouping"),
        });
    }
    Ok(format!("{integer}.{fraction}"))
}

fn validate_grouped_integer(
    value: &str,
    grouping: char,
    field: &str,
) -> Result<(), ManualImportError> {
    let groups = value.split(grouping).collect::<Vec<_>>();
    if groups[0].is_empty()
        || groups[0].len() > 3
        || !groups[0].bytes().all(|byte| byte.is_ascii_digit())
        || !groups[1..]
            .iter()
            .all(|group| group.len() == 3 && group.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return invalid_decimal_grouping(field);
    }
    Ok(())
}

fn invalid_decimal_grouping<T>(field: &str) -> Result<T, ManualImportError> {
    Err(ManualImportError::InvalidField {
        field: field.to_owned(),
        reason: "invalid or ambiguous decimal grouping".to_owned(),
    })
}

fn parse_flat_date(value: &str, field: &str) -> Result<NaiveDate, ManualImportError> {
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Ok(date);
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%d.%m.%Y") {
        return Ok(date);
    }
    for (separator, first_format, second_format) in
        [('/', "%d/%m/%Y", "%m/%d/%Y"), ('-', "%d-%m-%Y", "%m-%d-%Y")]
    {
        if value.contains(separator) {
            let first = NaiveDate::parse_from_str(value, first_format).ok();
            let second = NaiveDate::parse_from_str(value, second_format).ok();
            return match (first, second) {
                (Some(left), Some(right)) if left != right => {
                    Err(ManualImportError::InvalidField {
                        field: field.to_owned(),
                        reason: format!("ambiguous date {value}; use YYYY-MM-DD"),
                    })
                }
                (Some(date), _) | (_, Some(date)) => Ok(date),
                _ => Err(ManualImportError::InvalidField {
                    field: field.to_owned(),
                    reason: "unsupported date; use YYYY-MM-DD".to_owned(),
                }),
            };
        }
    }
    Err(ManualImportError::InvalidField {
        field: field.to_owned(),
        reason: "unsupported date; use YYYY-MM-DD".to_owned(),
    })
}

fn optional_cell(record: &StringRecord, index: usize) -> Option<&str> {
    record
        .get(index)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn resolve_text_metadata(
    field: &str,
    source: Option<String>,
    confirmed: Option<&str>,
) -> Result<(Option<String>, MetadataProvenance), ManualImportError> {
    let confirmed = confirmed
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    match (source, confirmed) {
        (Some(source), Some(confirmed)) if source != confirmed => {
            Err(ManualImportError::MetadataMismatch {
                field: field.to_owned(),
                expected: source,
                found: confirmed,
            })
        }
        (Some(source), _) => Ok((Some(source), MetadataProvenance::Report)),
        (None, Some(confirmed)) => Ok((Some(confirmed), MetadataProvenance::OperatorConfirmed)),
        (None, None) => Ok((None, MetadataProvenance::Missing)),
    }
}

fn resolve_period_metadata(
    source: Option<(NaiveDate, NaiveDate)>,
    confirmed_start: Option<NaiveDate>,
    confirmed_end: Option<NaiveDate>,
) -> Result<(Option<NaiveDate>, Option<NaiveDate>, MetadataProvenance), ManualImportError> {
    match (source, confirmed_start, confirmed_end) {
        (Some((source_start, _)), Some(confirmed), _) if source_start != confirmed => {
            Err(ManualImportError::MetadataMismatch {
                field: "period_start".to_owned(),
                expected: source_start.to_string(),
                found: confirmed.to_string(),
            })
        }
        (Some((_, source_end)), _, Some(confirmed)) if source_end != confirmed => {
            Err(ManualImportError::MetadataMismatch {
                field: "period_end".to_owned(),
                expected: source_end.to_string(),
                found: confirmed.to_string(),
            })
        }
        (Some((start, end)), _, _) => Ok((Some(start), Some(end), MetadataProvenance::Report)),
        (None, Some(start), Some(end)) => Ok((
            Some(start),
            Some(end),
            MetadataProvenance::OperatorConfirmed,
        )),
        (None, None, None) => Ok((None, None, MetadataProvenance::Missing)),
        (None, _, _) => Err(ManualImportError::MetadataRequired(
            "both period_start and period_end".to_owned(),
        )),
    }
}

fn confirm_text(field: &str, expected: Option<&str>, found: &str) -> Result<(), ManualImportError> {
    if let Some(expected) = expected.map(str::trim).filter(|value| !value.is_empty()) {
        if expected != found {
            return Err(ManualImportError::MetadataMismatch {
                field: field.to_owned(),
                expected: expected.to_owned(),
                found: found.to_owned(),
            });
        }
    }
    Ok(())
}

fn confirm_date(
    field: &str,
    expected: Option<NaiveDate>,
    found: NaiveDate,
) -> Result<(), ManualImportError> {
    if let Some(expected) = expected {
        if expected != found {
            return Err(ManualImportError::MetadataMismatch {
                field: field.to_owned(),
                expected: expected.to_string(),
                found: found.to_string(),
            });
        }
    }
    Ok(())
}

fn confirm_currency(expected: Option<&str>, found: &str) -> Result<(), ManualImportError> {
    if let Some(expected) = expected {
        let expected = normalize_currency(expected)?;
        if expected != found {
            return Err(ManualImportError::CurrencyConflict {
                expected,
                found: found.to_owned(),
            });
        }
    }
    Ok(())
}

fn normalize_currency(value: &str) -> Result<String, ManualImportError> {
    let currency = value.trim().to_ascii_uppercase();
    if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return Err(ManualImportError::InvalidField {
            field: "currency_code".to_owned(),
            reason: "must be a three-letter ISO-style code".to_owned(),
        });
    }
    Ok(currency)
}

fn reconcile_currencies<'a>(
    candidates: impl IntoIterator<Item = Option<&'a str>>,
) -> Result<Option<String>, ManualImportError> {
    let mut resolved: Option<String> = None;
    for candidate in candidates.into_iter().flatten() {
        let candidate = normalize_currency(candidate)?;
        if let Some(existing) = &resolved {
            if existing != &candidate {
                return Err(ManualImportError::CurrencyConflict {
                    expected: existing.clone(),
                    found: candidate,
                });
            }
        } else {
            resolved = Some(candidate);
        }
    }
    Ok(resolved)
}

#[derive(Debug)]
struct RowMetrics {
    sales: Decimal,
    currency: Option<String>,
    units: Decimal,
    sessions: Option<Decimal>,
    page_views: Option<Decimal>,
    reported_conversion: Option<Decimal>,
    buy_box_percentage: Option<Decimal>,
    b2b_sales: Option<(Decimal, Option<String>)>,
    b2b_units: Option<Decimal>,
}

#[derive(Debug, Default)]
struct WeightedPercentage {
    weighted_sum: Decimal,
    weight: Decimal,
    unweighted: Vec<Decimal>,
}

impl WeightedPercentage {
    fn add(&mut self, value: Decimal, weight: Option<Decimal>) {
        self.unweighted.push(value);
        if let Some(weight) = weight.filter(|weight| *weight > Decimal::ZERO) {
            self.weighted_sum += value * weight;
            self.weight += weight;
        }
    }

    fn value(&self) -> Option<Decimal> {
        if self.weight > Decimal::ZERO {
            Some((self.weighted_sum / self.weight).round_dp(4))
        } else if self.unweighted.len() == 1 {
            self.unweighted.first().copied()
        } else {
            None
        }
    }
}

#[derive(Debug, Default)]
struct Totals {
    currency: Option<String>,
    sales: Decimal,
    units: Decimal,
    sessions: Decimal,
    sessions_present: bool,
    page_views: Decimal,
    page_views_present: bool,
    reported_conversion: WeightedPercentage,
    buy_box_percentage: WeightedPercentage,
    b2b_sales: Decimal,
    b2b_sales_present: bool,
    b2b_units: Decimal,
    b2b_units_present: bool,
    rows: usize,
}

impl Totals {
    fn register_currency(&mut self, currency: &str) -> Result<(), ManualImportError> {
        if let Some(existing) = &self.currency {
            if existing != currency {
                return Err(ManualImportError::CurrencyConflict {
                    expected: existing.clone(),
                    found: currency.to_owned(),
                });
            }
        } else {
            self.currency = Some(currency.to_owned());
        }
        Ok(())
    }

    fn add(&mut self, row: RowMetrics) -> Result<(), ManualImportError> {
        if let Some(currency) = &row.currency {
            self.register_currency(currency)?;
        }
        if row.sales < Decimal::ZERO || row.units < Decimal::ZERO {
            return Err(ManualImportError::InvalidField {
                field: "metric row".to_owned(),
                reason: "sales and units must not be negative".to_owned(),
            });
        }
        self.sales += row.sales;
        self.units += row.units;
        if let Some(sessions) = row.sessions {
            self.sessions_present = true;
            self.sessions += sessions;
        }
        if let Some(page_views) = row.page_views {
            self.page_views_present = true;
            self.page_views += page_views;
        }
        if let Some(conversion) = row.reported_conversion {
            self.reported_conversion.add(conversion, row.sessions);
        }
        if let Some(buy_box) = row.buy_box_percentage {
            self.buy_box_percentage.add(buy_box, row.page_views);
        }
        if let Some((sales, currency)) = row.b2b_sales {
            if let Some(currency) = currency {
                self.register_currency(&currency)?;
            }
            self.b2b_sales_present = true;
            self.b2b_sales += sales;
        }
        if let Some(units) = row.b2b_units {
            self.b2b_units_present = true;
            self.b2b_units += units;
        }
        self.rows += 1;
        Ok(())
    }
}

struct PreviewInput {
    format: ManualReportFormat,
    raw_sha256: String,
    raw_bytes: usize,
    marketplace_id: Option<String>,
    period_start: Option<NaiveDate>,
    period_end: Option<NaiveDate>,
    date_granularity: String,
    asin_granularity: String,
    reporting_timezone: Option<String>,
    timezone_source_note: String,
    currency_code: Option<String>,
    operator_confirmed: Vec<String>,
    metadata_provenance: BTreeMap<String, MetadataProvenance>,
    warnings: Vec<String>,
    totals: Totals,
}

fn build_preview(mut input: PreviewInput) -> Result<ManualImportPreview, ManualImportError> {
    let calculated_conversion =
        if input.totals.sessions_present && input.totals.sessions > Decimal::ZERO {
            Some((input.totals.units / input.totals.sessions * Decimal::from(100)).round_dp(4))
        } else {
            input.totals.reported_conversion.value()
        };
    if let (Some(calculated), Some(reported)) = (
        calculated_conversion,
        input.totals.reported_conversion.value(),
    ) {
        if (calculated - reported).abs() > Decimal::new(10, 2) {
            input.warnings.push(
                "Reported Unit Session Percentage differs from units/sessions by more than 0.10 percentage points."
                    .to_owned(),
            );
        }
    }
    let buy_box = input.totals.buy_box_percentage.value();
    let b2b_revenue_share = if input.totals.b2b_sales_present && input.totals.sales > Decimal::ZERO
    {
        Some((input.totals.b2b_sales / input.totals.sales * Decimal::from(100)).round_dp(4))
    } else {
        None
    };
    let b2b_units_share = if input.totals.b2b_units_present && input.totals.units > Decimal::ZERO {
        Some((input.totals.b2b_units / input.totals.units * Decimal::from(100)).round_dp(4))
    } else {
        None
    };

    let mut missing = BTreeSet::new();
    let mut confirmation_missing = BTreeSet::new();
    for (missing_field, absent) in [
        ("marketplace_id", input.marketplace_id.is_none()),
        ("period_start", input.period_start.is_none()),
        ("period_end", input.period_end.is_none()),
        ("reporting_timezone", input.reporting_timezone.is_none()),
        ("currency_code", input.currency_code.is_none()),
    ] {
        if absent {
            missing.insert(missing_field.to_owned());
            confirmation_missing.insert(missing_field.to_owned());
        }
    }
    if !input.totals.sessions_present {
        missing.insert("sessions".to_owned());
    }
    if !input.totals.page_views_present {
        missing.insert("page_views".to_owned());
    }
    if calculated_conversion.is_none() {
        missing.insert("unit_session_percentage".to_owned());
    }
    if buy_box.is_none() {
        missing.insert("buy_box_percentage".to_owned());
    }
    if !input.totals.b2b_sales_present {
        missing.insert("b2b_ordered_product_sales".to_owned());
        missing.insert("b2b_revenue_share".to_owned());
    }
    if !input.totals.b2b_units_present {
        missing.insert("b2b_units_ordered".to_owned());
        missing.insert("b2b_units_share".to_owned());
    }
    let confirmation_required = !confirmation_missing.is_empty();
    if confirmation_required {
        input.warnings.push(format!(
            "confirmation_required: {}",
            confirmation_missing
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let evidence_hash = input.raw_sha256.clone();
    let format_label = match input.format {
        ManualReportFormat::Json => "json",
        ManualReportFormat::Csv => "csv",
        ManualReportFormat::Tsv => "tsv",
    };
    let evidence = |aggregation: &str| {
        json!({
            "source": "manual_official_amazon_report",
            "format": format_label,
            "aggregation": aggregation,
            "raw_sha256": evidence_hash,
        })
    };
    let mut metrics = vec![
        parsed_metric(
            "ordered_product_sales",
            input.totals.sales,
            "currency",
            input.currency_code.as_deref(),
            evidence("sum"),
        ),
        parsed_metric(
            "units_ordered",
            input.totals.units,
            "units",
            None,
            evidence("sum"),
        ),
    ];
    if input.totals.sessions_present {
        metrics.push(parsed_metric(
            "sessions",
            input.totals.sessions,
            "sessions",
            None,
            evidence("sum"),
        ));
    }
    if input.totals.page_views_present {
        metrics.push(parsed_metric(
            "page_views",
            input.totals.page_views,
            "views",
            None,
            evidence("sum"),
        ));
    }
    if let Some(conversion) = calculated_conversion {
        metrics.push(parsed_metric(
            "conversion_rate",
            conversion,
            "percent",
            None,
            evidence("units_ordered / sessions * 100, with reported fallback"),
        ));
    }
    if let Some(buy_box) = buy_box {
        metrics.push(parsed_metric(
            "buy_box_percentage",
            buy_box,
            "percent",
            None,
            evidence("page-view-weighted percentage"),
        ));
    }
    if input.totals.b2b_sales_present {
        metrics.push(parsed_metric(
            "b2b_ordered_product_sales",
            input.totals.b2b_sales,
            "currency",
            input.currency_code.as_deref(),
            evidence("sum"),
        ));
    }
    if input.totals.b2b_units_present {
        metrics.push(parsed_metric(
            "b2b_units_ordered",
            input.totals.b2b_units,
            "units",
            None,
            evidence("sum"),
        ));
    }
    if let Some(share) = b2b_revenue_share {
        metrics.push(parsed_metric(
            "b2b_revenue_share",
            share,
            "percent",
            None,
            evidence("b2b_ordered_product_sales / ordered_product_sales * 100"),
        ));
    }
    if let Some(share) = b2b_units_share {
        metrics.push(parsed_metric(
            "b2b_units_share",
            share,
            "percent",
            None,
            evidence("b2b_units_ordered / units_ordered * 100"),
        ));
    }

    let granularity = format!(
        "{}_{}",
        input.date_granularity.to_ascii_lowercase(),
        input.asin_granularity.to_ascii_lowercase()
    );
    let period_days = input
        .period_start
        .zip(input.period_end)
        .map(|(start, end)| (end - start).num_days() + 1);
    let comparability_key = match (
        period_days,
        input.currency_code.as_deref(),
        input.reporting_timezone.as_deref(),
    ) {
        (Some(days), Some(currency), Some(timezone)) if !confirmation_required => format!(
            "manual-sales-traffic:{granularity}:{days}d:currency={}:timezone={}",
            currency.to_ascii_uppercase(),
            normalize_comparability_component(timezone)
        ),
        _ => format!(
            "manual-sales-traffic:confirmation-required:sha256={}",
            input.raw_sha256
        ),
    };
    let missing_fields = missing.into_iter().collect::<Vec<_>>();
    let summary = json!({
        "report_type": SALES_AND_TRAFFIC,
        "marketplace_id": input.marketplace_id.clone(),
        "period_start": input.period_start,
        "period_end": input.period_end,
        "data_freshness": input.period_end,
        "date_granularity": input.date_granularity.clone(),
        "asin_granularity": input.asin_granularity.clone(),
        "reporting_timezone": input.reporting_timezone.clone(),
        "timezone": input.reporting_timezone.clone(),
        "timezone_source_note": input.timezone_source_note.clone(),
        "currency": input.currency_code.clone(),
        "currency_code": input.currency_code.clone(),
        "ordered_product_sales": input.totals.sales.to_string(),
        "units_ordered": input.totals.units.to_string(),
        "sessions": input.totals.sessions_present.then(|| input.totals.sessions.to_string()),
        "page_views": input.totals.page_views_present.then(|| input.totals.page_views.to_string()),
        "unit_session_percentage": calculated_conversion.map(|value| value.to_string()),
        "conversion_rate": calculated_conversion.map(|value| value.to_string()),
        "buy_box_percentage": buy_box.map(|value| value.to_string()),
        "b2b_ordered_product_sales": input.totals.b2b_sales_present.then(|| input.totals.b2b_sales.to_string()),
        "b2b_units_ordered": input.totals.b2b_units_present.then(|| input.totals.b2b_units.to_string()),
        "b2b_revenue_share": b2b_revenue_share.map(|value| value.to_string()),
        "b2b_units_share": b2b_units_share.map(|value| value.to_string()),
        "row_count": input.totals.rows,
        "parser_version": MANUAL_SALES_TRAFFIC_PARSER_VERSION,
        "raw_sha256": input.raw_sha256.clone(),
        "confirmation_required": confirmation_required,
        "operator_confirmed": input.operator_confirmed.clone(),
        "metadata_provenance": input.metadata_provenance.clone(),
        "missing_fields": missing_fields.clone(),
        "warnings": input.warnings.clone(),
    });
    let snapshot = ParsedSnapshot {
        parser_version: MANUAL_SALES_TRAFFIC_PARSER_VERSION.to_owned(),
        period_start: input.period_start.map(|period_start| {
            Utc.from_utc_datetime(
                &period_start
                    .and_hms_opt(0, 0, 0)
                    .expect("valid date has midnight"),
            )
        }),
        period_end: input.period_end.map(|period_end| {
            Utc.from_utc_datetime(
                &period_end
                    .and_hms_opt(23, 59, 59)
                    .expect("valid date has end of day"),
            )
        }),
        granularity: granularity.clone(),
        comparability_key,
        summary,
        metrics,
    };
    Ok(ManualImportPreview {
        format: input.format,
        raw_sha256: input.raw_sha256,
        raw_bytes: input.raw_bytes,
        report_type: SALES_AND_TRAFFIC.to_owned(),
        marketplace_id: input.marketplace_id,
        period_start: input.period_start,
        period_end: input.period_end,
        date_granularity: input.date_granularity,
        asin_granularity: input.asin_granularity,
        parser_version: MANUAL_SALES_TRAFFIC_PARSER_VERSION,
        reporting_timezone: input.reporting_timezone,
        timezone_source_note: input.timezone_source_note,
        currency_code: input.currency_code,
        confirmation_required,
        operator_confirmed: input.operator_confirmed,
        metadata_provenance: input.metadata_provenance,
        missing_fields,
        warnings: input.warnings,
        snapshot,
    })
}

fn normalize_comparability_component(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn parsed_metric(
    name: &str,
    value: Decimal,
    unit: &str,
    currency: Option<&str>,
    evidence: Value,
) -> ParsedMetric {
    ParsedMetric {
        metric_name: name.to_owned(),
        dimension_type: "catalog".to_owned(),
        dimension_key: String::new(),
        value_numeric: value,
        unit: unit.to_owned(),
        currency_code: currency.map(str::to_owned),
        evidence,
    }
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTHETIC_MARKETPLACE: &str = "SYNTHETIC-TEST-MARKETPLACE-NOT-REAL";
    const SYNTHETIC_TIMEZONE: &str = "SYNTHETIC/TEST-TIMEZONE";

    fn metadata(start: &str, end: &str) -> ManualImportMetadata {
        ManualImportMetadata {
            marketplace_id: Some(SYNTHETIC_MARKETPLACE.to_owned()),
            period_start: Some(NaiveDate::parse_from_str(start, "%Y-%m-%d").unwrap()),
            period_end: Some(NaiveDate::parse_from_str(end, "%Y-%m-%d").unwrap()),
            reporting_timezone: Some(SYNTHETIC_TIMEZONE.to_owned()),
            currency_code: Some("EUR".to_owned()),
        }
    }

    fn metric<'a>(preview: &'a ManualImportPreview, name: &str) -> &'a ParsedMetric {
        preview
            .snapshot
            .metrics
            .iter()
            .find(|metric| metric.metric_name == name)
            .unwrap()
    }

    #[test]
    fn parses_official_json_aggregate_metrics_without_double_counting() {
        // SYNTHETIC TEST DATA ONLY. Identifiers and values are invented and not Mantle data.
        let fixture = format!(
            r#"{{
              "fixtureClassification":"SYNTHETIC TEST DATA - NO BUSINESS DATA",
              "reportSpecification":{{
                "reportType":"GET_SALES_AND_TRAFFIC_REPORT",
                "reportOptions":{{"dateGranularity":"DAY","asinGranularity":"CHILD"}},
                "dataStartTime":"2026-01-01",
                "dataEndTime":"2026-01-02",
                "marketplaceIds":["{SYNTHETIC_MARKETPLACE}"]
              }},
              "salesAndTrafficByDate":[
                {{"date":"2026-01-01","salesByDate":{{
                  "orderedProductSales":{{"amount":"120.50","currencyCode":"EUR"}},
                  "unitsOrdered":10,
                  "orderedProductSalesB2B":{{"amount":"24.10","currencyCode":"EUR"}},
                  "unitsOrderedB2B":2
                }},"trafficByDate":{{"sessions":100,"pageViews":150,
                  "unitSessionPercentage":10,"buyBoxPercentage":80}}}},
                {{"date":"2026-01-02","salesByDate":{{
                  "orderedProductSales":{{"amount":"79.50","currencyCode":"EUR"}},
                  "unitsOrdered":5,
                  "orderedProductSalesB2B":{{"amount":"15.90","currencyCode":"EUR"}},
                  "unitsOrderedB2B":1
                }},"trafficByDate":{{"sessions":50,"pageViews":50,
                  "unitSessionPercentage":10,"buyBoxPercentage":60}}}}
              ],
              "salesAndTrafficByAsin":[{{
                "childAsin":"SYNTHETIC-TEST-ASIN-NOT-REAL",
                "salesByAsin":{{"orderedProductSales":{{"amount":"200.00","currencyCode":"EUR"}},"unitsOrdered":15}},
                "trafficByAsin":{{"sessions":150,"pageViews":200}}
              }}]
            }}"#
        );
        let preview = parse_manual_sales_and_traffic(
            fixture.as_bytes(),
            &metadata("2026-01-01", "2026-01-02"),
        )
        .unwrap();
        assert_eq!(preview.format, ManualReportFormat::Json);
        assert_eq!(
            preview.marketplace_id.as_deref(),
            Some(SYNTHETIC_MARKETPLACE)
        );
        assert_eq!(
            metric(&preview, "ordered_product_sales").value_numeric,
            Decimal::from(200)
        );
        assert_eq!(
            metric(&preview, "conversion_rate").value_numeric,
            Decimal::from(10)
        );
        assert_eq!(
            metric(&preview, "buy_box_percentage").value_numeric,
            Decimal::from(75)
        );
        assert_eq!(
            metric(&preview, "b2b_revenue_share").value_numeric,
            Decimal::from(20)
        );
        assert_eq!(preview.snapshot.summary["row_count"], 2);
        assert!(!preview.confirmation_required);
        assert!(preview
            .snapshot
            .comparability_key
            .contains("currency=EUR:timezone=synthetic-test-timezone"));
        assert_eq!(preview.snapshot.summary["currency_code"], "EUR");
        assert_eq!(preview.snapshot.summary["timezone"], SYNTHETIC_TIMEZONE);
        assert_eq!(preview.snapshot.summary["data_freshness"], "2026-01-02");
    }

    #[test]
    fn parses_semicolon_csv_with_locale_decimal_and_aliases() {
        // SYNTHETIC TEST DATA ONLY.
        let fixture = format!(
            "Date;Marketplace ID;Ordered Product Sales;Currency;Units Ordered;Sessions - Total;Page Views - Total;Unit Session Percentage;Buy Box Percentage;B2B Sales;B2B Units\n\
             2026-02-10;{SYNTHETIC_MARKETPLACE};\"1.234,56\";EUR;20;200;300;\"10,00%\";\"75,00%\";\"246,91\";4\n"
        );
        let preview = parse_manual_sales_and_traffic(
            fixture.as_bytes(),
            &metadata("2026-02-10", "2026-02-10"),
        )
        .unwrap();
        assert_eq!(preview.format, ManualReportFormat::Csv);
        assert_eq!(preview.date_granularity, "DAY");
        assert_eq!(
            metric(&preview, "ordered_product_sales").value_numeric,
            Decimal::new(123456, 2)
        );
        assert_eq!(
            metric(&preview, "b2b_units_ordered").value_numeric,
            Decimal::from(4)
        );
    }

    #[test]
    fn parses_bom_tsv_with_german_aliases_and_unambiguous_date() {
        // SYNTHETIC TEST DATA ONLY.
        let fixture = format!(
            "\u{feff}Datum\tMarktplatz-ID\tUmsatz bestellter Produkte\tWährung\tBestellte Einheiten\tSitzungen\tSeitenaufrufe\tProzentsatz der Einheiten pro Sitzung\tBuy Box Percentage\n\
             13/02/2026\t{SYNTHETIC_MARKETPLACE}\t€2.345,67\tEUR\t30\t300\t450\t10,00%\t66,00%\n"
        );
        let preview = parse_manual_sales_and_traffic(
            fixture.as_bytes(),
            &metadata("2026-02-13", "2026-02-13"),
        )
        .unwrap();
        assert_eq!(preview.format, ManualReportFormat::Tsv);
        assert_eq!(preview.currency_code.as_deref(), Some("EUR"));
        assert_eq!(
            metric(&preview, "ordered_product_sales").value_numeric,
            Decimal::new(234567, 2)
        );
        assert!(preview
            .missing_fields
            .contains(&"b2b_ordered_product_sales".to_owned()));
    }

    #[test]
    fn rejects_malformed_json_schema_without_partial_result() {
        let fixture = br#"{"reportSpecification":{"marketplaceIds":["SYNTHETIC-TEST"]},"salesAndTrafficByDate":[]}"#;
        let error =
            parse_manual_sales_and_traffic(fixture, &ManualImportMetadata::default()).unwrap_err();
        assert!(
            matches!(error, ManualImportError::MissingField(field) if field == "reportSpecification.reportType")
        );
    }

    #[test]
    fn rejects_flat_schema_without_sales_and_traffic_signature() {
        // SYNTHETIC TEST DATA ONLY.
        let fixture = b"Ordered Product Sales,Units Ordered\n10.00,1\n";
        let error =
            parse_manual_sales_and_traffic(fixture, &ManualImportMetadata::default()).unwrap_err();
        assert!(
            matches!(error, ManualImportError::MissingField(field) if field.contains("traffic column"))
        );
    }

    #[test]
    fn rejects_oversized_report_before_format_parsing() {
        let fixture = vec![b'x'; MAX_MANUAL_REPORT_BYTES + 1];
        let error =
            parse_manual_sales_and_traffic(&fixture, &ManualImportMetadata::default()).unwrap_err();
        assert!(matches!(error, ManualImportError::TooLarge { .. }));
    }

    #[test]
    fn rejects_pii_headers_fail_closed() {
        let fixture = b"Date,Buyer Email,Ordered Product Sales,Units Ordered\n2026-03-01,synthetic@example.invalid,10.00,1\n";
        let error = parse_manual_sales_and_traffic(fixture, &metadata("2026-03-01", "2026-03-01"))
            .unwrap_err();
        assert!(matches!(error, ManualImportError::PiiHeader(header) if header == "Buyer Email"));
    }

    #[test]
    fn rejects_nested_json_pii_keys_fail_closed() {
        // SYNTHETIC TEST DATA ONLY.
        let fixture = format!(
            r#"{{"reportSpecification":{{"reportType":"GET_SALES_AND_TRAFFIC_REPORT",
              "reportOptions":{{"dateGranularity":"DAY","asinGranularity":"CHILD"}},
              "dataStartTime":"2026-03-01","dataEndTime":"2026-03-01",
              "marketplaceIds":["{SYNTHETIC_MARKETPLACE}"]}},
              "salesAndTrafficByDate":[{{"date":"2026-03-01",
                "salesByDate":{{"orderedProductSales":{{"amount":"10.00","currencyCode":"EUR"}},"unitsOrdered":1}},
                "syntheticEnvelope":{{"buyerEmail":"synthetic@example.invalid"}}}}]}}"#
        );
        let error =
            parse_manual_sales_and_traffic(fixture.as_bytes(), &ManualImportMetadata::default())
                .unwrap_err();
        assert!(
            matches!(error, ManualImportError::PiiHeader(path) if path.ends_with("syntheticEnvelope.buyerEmail"))
        );
    }

    #[test]
    fn flat_preview_exposes_missing_confirmations_without_partial_failure() {
        // SYNTHETIC TEST DATA ONLY. The flat export intentionally omits all confirmable metadata.
        let fixture = b"Ordered Product Sales,Units Ordered,Sessions - Total,Page Views - Total\n10.00,1,10,20\n";
        let preview =
            parse_manual_sales_and_traffic(fixture, &ManualImportMetadata::default()).unwrap();

        assert!(preview.confirmation_required);
        assert_eq!(preview.marketplace_id, None);
        assert_eq!(preview.period_start, None);
        assert_eq!(preview.currency_code, None);
        for field in [
            "marketplace_id",
            "period_start",
            "period_end",
            "reporting_timezone",
            "currency_code",
        ] {
            assert!(preview.missing_fields.contains(&field.to_owned()));
            assert_eq!(
                preview.metadata_provenance[field],
                MetadataProvenance::Missing
            );
        }
        assert!(preview.ensure_ready_for_import().is_err());
        assert!(preview
            .snapshot
            .comparability_key
            .contains("confirmation-required"));
    }

    #[test]
    fn operator_confirmations_only_fill_absent_flat_metadata() {
        // SYNTHETIC TEST DATA ONLY.
        let fixture = "Ordered Product Sales;Units Ordered;Sitzungen – Gesamt;Seitenaufrufe – Gesamt\n10,00;1;10;20\n";
        let preview = parse_manual_sales_and_traffic(
            fixture.as_bytes(),
            &metadata("2026-03-10", "2026-03-10"),
        )
        .unwrap();

        assert!(!preview.confirmation_required);
        preview.ensure_ready_for_import().unwrap();
        for field in [
            "marketplace_id",
            "period_start",
            "period_end",
            "reporting_timezone",
            "currency_code",
        ] {
            assert_eq!(
                preview.metadata_provenance[field],
                MetadataProvenance::OperatorConfirmed
            );
            assert!(preview.operator_confirmed.contains(&field.to_owned()));
        }
        assert!(preview
            .warnings
            .iter()
            .any(|warning| warning.starts_with("operator_confirmed:")));
    }

    #[test]
    fn operator_metadata_cannot_override_flat_source_values() {
        // SYNTHETIC TEST DATA ONLY.
        let fixture = format!(
            "Date,Marketplace ID,Ordered Product Sales,Currency,Units Ordered,Sessions\n2026-03-20,{SYNTHETIC_MARKETPLACE},10.00,EUR,1,10\n"
        );
        let mut context = metadata("2026-03-20", "2026-03-20");
        context.marketplace_id = Some("SYNTHETIC-DIFFERENT-MARKETPLACE".to_owned());
        let error = parse_manual_sales_and_traffic(fixture.as_bytes(), &context).unwrap_err();
        assert!(
            matches!(error, ManualImportError::MetadataMismatch { field, .. } if field == "marketplace_id")
        );
    }

    #[test]
    fn rejects_conflicting_json_currencies() {
        // SYNTHETIC TEST DATA ONLY.
        let fixture = format!(
            r#"{{"reportSpecification":{{"reportType":"GET_SALES_AND_TRAFFIC_REPORT",
              "reportOptions":{{"dateGranularity":"DAY","asinGranularity":"CHILD"}},
              "dataStartTime":"2026-04-01","dataEndTime":"2026-04-02",
              "marketplaceIds":["{SYNTHETIC_MARKETPLACE}"]}},
              "salesAndTrafficByDate":[
                {{"date":"2026-04-01","salesByDate":{{"orderedProductSales":{{"amount":"10.00","currencyCode":"EUR"}},"unitsOrdered":1}}}},
                {{"date":"2026-04-02","salesByDate":{{"orderedProductSales":{{"amount":"12.00","currencyCode":"USD"}},"unitsOrdered":1}}}}
              ],"salesAndTrafficByAsin":[]}}"#
        );
        let error = parse_manual_sales_and_traffic(
            fixture.as_bytes(),
            &metadata("2026-04-01", "2026-04-02"),
        )
        .unwrap_err();
        assert!(matches!(error, ManualImportError::CurrencyConflict { .. }));
    }

    #[test]
    fn accepts_official_week_bucket_and_unit_session_percentage_above_one_hundred() {
        // Mirrors edge cases in Amazon's official Sales and Traffic JSON schema:
        // the week bucket may start before dataStartTime and units/session may exceed 100%.
        let fixture = format!(
            r#"{{"reportSpecification":{{"reportType":"GET_SALES_AND_TRAFFIC_REPORT",
              "reportOptions":{{"dateGranularity":"WEEK"}},
              "dataStartTime":"2026-06-11","dataEndTime":"2026-06-14",
              "marketplaceIds":["{SYNTHETIC_MARKETPLACE}"]}},
              "salesAndTrafficByDate":[{{"date":"2026-06-06",
                "salesByDate":{{"orderedProductSales":{{"amount":"30.00","currencyCode":"EUR"}},"unitsOrdered":3}},
                "trafficByDate":{{"sessions":1,"pageViews":1,"unitSessionPercentage":300,"buyBoxPercentage":100}}}}],
              "salesAndTrafficByAsin":[]}}"#
        );
        let preview = parse_manual_sales_and_traffic(
            fixture.as_bytes(),
            &metadata("2026-06-11", "2026-06-14"),
        )
        .unwrap();

        assert_eq!(preview.date_granularity, "WEEK");
        assert_eq!(preview.asin_granularity, "PARENT");
        assert_eq!(
            metric(&preview, "conversion_rate").value_numeric,
            Decimal::from(300)
        );
    }

    #[test]
    fn rejects_buy_box_percentage_above_one_hundred() {
        let fixture = format!(
            r#"{{"reportSpecification":{{"reportType":"GET_SALES_AND_TRAFFIC_REPORT",
              "reportOptions":{{"dateGranularity":"DAY","asinGranularity":"CHILD"}},
              "dataStartTime":"2026-06-20","dataEndTime":"2026-06-20",
              "marketplaceIds":["{SYNTHETIC_MARKETPLACE}"]}},
              "salesAndTrafficByDate":[{{"date":"2026-06-20",
                "salesByDate":{{"orderedProductSales":{{"amount":"10.00","currencyCode":"EUR"}},"unitsOrdered":1}},
                "trafficByDate":{{"sessions":1,"pageViews":1,"buyBoxPercentage":100.01}}}}],
              "salesAndTrafficByAsin":[]}}"#
        );
        let error = parse_manual_sales_and_traffic(
            fixture.as_bytes(),
            &metadata("2026-06-20", "2026-06-20"),
        )
        .unwrap_err();

        assert!(
            matches!(error, ManualImportError::InvalidField { field, .. } if field.ends_with("buyBoxPercentage"))
        );
    }

    #[test]
    fn rejects_ambiguous_flat_dates() {
        let fixture = format!(
            "Date,Marketplace ID,Ordered Product Sales,Currency,Units Ordered,Sessions\n01/02/2026,{SYNTHETIC_MARKETPLACE},10.00,EUR,1,10\n"
        );
        let error = parse_manual_sales_and_traffic(
            fixture.as_bytes(),
            &metadata("2026-01-02", "2026-01-02"),
        )
        .unwrap_err();
        assert!(
            matches!(error, ManualImportError::InvalidField { reason, .. } if reason.contains("ambiguous date"))
        );
    }

    #[test]
    fn raw_hash_is_deterministic_and_covers_original_bytes() {
        let fixture = format!(
            "Date,Marketplace ID,Ordered Product Sales,Currency,Units Ordered,Sessions\n2026-05-01,{SYNTHETIC_MARKETPLACE},10.25,EUR,2,20\n"
        );
        let context = metadata("2026-05-01", "2026-05-01");
        let first = parse_manual_sales_and_traffic(fixture.as_bytes(), &context).unwrap();
        let second = parse_manual_sales_and_traffic(fixture.as_bytes(), &context).unwrap();
        assert_eq!(first.raw_sha256, second.raw_sha256);
        assert_eq!(first.raw_sha256, sha256_hex(fixture.as_bytes()));
        let changed =
            parse_manual_sales_and_traffic(fixture.replace("10.25", "10.26").as_bytes(), &context)
                .unwrap();
        assert_ne!(first.raw_sha256, changed.raw_sha256);
    }
}
