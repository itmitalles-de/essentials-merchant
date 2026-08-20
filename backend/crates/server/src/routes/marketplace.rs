use axum::body::{Body, Bytes};
use axum::extract::DefaultBodyLimit;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(overview))
        .route("/connections", post(upsert_connection))
        .route("/demo", post(create_demo))
        .route(
            "/imports/preview",
            post(preview_manual_import).layer(DefaultBodyLimit::max(
                crate::manual_import::MAX_MANUAL_REPORT_BYTES,
            )),
        )
        .route(
            "/imports",
            post(execute_manual_import).layer(DefaultBodyLimit::max(
                crate::manual_import::MAX_MANUAL_REPORT_BYTES,
            )),
        )
        .route(
            "/imports/ads/preview",
            post(preview_manual_ads_import).layer(DefaultBodyLimit::max(
                crate::manual_import::MAX_MANUAL_REPORT_BYTES,
            )),
        )
        .route(
            "/imports/ads",
            post(execute_manual_ads_import).layer(DefaultBodyLimit::max(
                crate::manual_import::MAX_MANUAL_REPORT_BYTES,
            )),
        )
        .route("/connections/{connection_id}/runs", post(create_run))
        .route(
            "/connections/{connection_id}/schedules",
            put(upsert_schedule),
        )
        .route(
            "/connections/{connection_id}/analyses",
            post(create_total_analysis),
        )
        .route("/runs/{run_id}", get(run_detail))
        .route("/runs/{run_id}/raw", get(raw_document))
        .route("/strategy/status", get(strategy_status))
        .route(
            "/product-mappings",
            get(product_mappings).post(store_product_mapping),
        )
        .route(
            "/strategy/knowledge",
            get(business_knowledge_status)
                .post(import_business_knowledge)
                .layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route(
            "/strategy/weekly",
            get(weekly_strategy_preview)
                .post(create_weekly_strategy_assessment)
                .layer(DefaultBodyLimit::max(2 * 1024)),
        )
        .route("/analyses/{analysis_id}/export", get(export_analysis))
}

async fn require_marketplace(
    state: &AppState,
    user: &db::users::User,
    _action: bool,
) -> Result<(), StatusCode> {
    if !db::modules::is_enabled(&state.pool, db::modules::MARKETPLACE_INTELLIGENCE)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        return Err(StatusCode::CONFLICT);
    }
    if user.role == "administrator" {
        return Ok(());
    }
    db::modules::user_can_access(
        &state.pool,
        user.id,
        &user.role,
        db::modules::MARKETPLACE_INTELLIGENCE,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .then_some(())
    .ok_or(StatusCode::FORBIDDEN)
}

async fn overview(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<db::marketplace::MarketplaceOverview>, StatusCode> {
    require_marketplace(&state, &user, false).await?;
    let mut overview = db::marketplace::overview(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let gui_amazon_configured = state
        .provider_secrets
        .status()
        .await
        .map(|status| status.amazon.configured)
        .unwrap_or(false);
    if gui_amazon_configured {
        for connection in &mut overview.connections {
            if connection.mode == "live" && connection.enabled {
                connection.credential_configured = true;
            }
        }
    }
    Ok(Json(overview))
}

async fn upsert_connection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<db::marketplace::AmazonConnectionInput>,
) -> Result<Json<db::marketplace::AmazonConnectionSummary>, StatusCode> {
    require_marketplace(&state, &user, true).await?;
    if user.role != "administrator" {
        return Err(StatusCode::FORBIDDEN);
    }
    let pilot_enabled = db::modules::is_enabled(&state.pool, db::modules::AMAZON_READ_ONLY_PILOT)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if pilot_enabled
        && input.mode == "live"
        && (input.marketplace_ids.len() != 1 || input.secret_ref != "pilot_seller")
    {
        return Err(StatusCode::PRECONDITION_FAILED);
    }
    db::marketplace::upsert_connection(&state.pool, &input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::BAD_REQUEST)
}

async fn create_demo(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<db::marketplace::AmazonConnectionSummary>, StatusCode> {
    require_marketplace(&state, &user, true).await?;
    if user.role != "administrator" {
        return Err(StatusCode::FORBIDDEN);
    }
    db::marketplace::create_demo_connection(&state.pool)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug, Default, Deserialize)]
struct ManualImportQuery {
    filename: String,
    timezone: String,
    confirm_hash: Option<String>,
    confirm_marketplace_id: Option<String>,
    confirm_currency_code: Option<String>,
    confirm_period_start: Option<String>,
    confirm_period_end: Option<String>,
    confirm_granularity: Option<String>,
    confirm_report_type: Option<String>,
    confirm_attribution_window_days: Option<u16>,
}

type ManualApiError = (StatusCode, Json<Value>);

fn manual_api_error(status: StatusCode, message: impl Into<String>) -> ManualApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn validated_timezone(value: &str) -> Result<String, ManualApiError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic())
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '/' | '_' | '-' | '+')
        })
    {
        return Err(manual_api_error(
            StatusCode::BAD_REQUEST,
            "timezone must be a bounded IANA-style name such as Europe/Berlin",
        ));
    }
    Ok(value.to_owned())
}

fn parse_confirmation_date(
    value: Option<&str>,
    field: &str,
) -> Result<Option<NaiveDate>, ManualApiError> {
    value
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
                manual_api_error(
                    StatusCode::BAD_REQUEST,
                    format!("{field} must use YYYY-MM-DD"),
                )
            })
        })
        .transpose()
}

fn manual_metadata(
    query: &ManualImportQuery,
) -> Result<crate::manual_import::ManualImportMetadata, ManualApiError> {
    Ok(crate::manual_import::ManualImportMetadata {
        marketplace_id: query.confirm_marketplace_id.clone(),
        period_start: parse_confirmation_date(
            query.confirm_period_start.as_deref(),
            "confirm_period_start",
        )?,
        period_end: parse_confirmation_date(
            query.confirm_period_end.as_deref(),
            "confirm_period_end",
        )?,
        reporting_timezone: Some(validated_timezone(&query.timezone)?),
        currency_code: query.confirm_currency_code.clone(),
    })
}

fn manual_ads_metadata(
    query: &ManualImportQuery,
) -> Result<crate::manual_import::ManualAdsImportMetadata, ManualApiError> {
    Ok(crate::manual_import::ManualAdsImportMetadata {
        marketplace_id: query.confirm_marketplace_id.clone(),
        period_start: parse_confirmation_date(
            query.confirm_period_start.as_deref(),
            "confirm_period_start",
        )?,
        period_end: parse_confirmation_date(
            query.confirm_period_end.as_deref(),
            "confirm_period_end",
        )?,
        reporting_timezone: Some(validated_timezone(&query.timezone)?),
        currency_code: query.confirm_currency_code.clone(),
        attribution_window_days: query.confirm_attribution_window_days,
    })
}

fn format_name(format: crate::manual_import::ManualReportFormat) -> &'static str {
    match format {
        crate::manual_import::ManualReportFormat::Json => "json",
        crate::manual_import::ManualReportFormat::Csv => "csv",
        crate::manual_import::ManualReportFormat::Tsv => "tsv",
    }
}

fn validate_filename(
    filename: &str,
    format: crate::manual_import::ManualReportFormat,
) -> Result<(), ManualApiError> {
    if filename.is_empty()
        || filename.len() > 255
        || filename.contains(['/', '\\'])
        || filename.chars().any(char::is_control)
    {
        return Err(manual_api_error(
            StatusCode::BAD_REQUEST,
            "filename must be a plain bounded name",
        ));
    }
    let extension = filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .ok_or_else(|| {
            manual_api_error(
                StatusCode::BAD_REQUEST,
                "report filename needs an extension",
            )
        })?;
    let matches = matches!(
        (extension.as_str(), format),
        ("json", crate::manual_import::ManualReportFormat::Json)
            | ("csv", crate::manual_import::ManualReportFormat::Csv)
            | ("tsv", crate::manual_import::ManualReportFormat::Tsv)
    );
    if !matches {
        return Err(manual_api_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "filename extension does not match the detected report bytes",
        ));
    }
    Ok(())
}

fn preview_json(preview: &crate::manual_import::ManualImportPreview) -> Value {
    json!({
        "sha256": preview.raw_sha256,
        "raw_bytes": preview.raw_bytes,
        "detected_format": format_name(preview.format),
        "report_type": preview.report_type,
        "parser_version": preview.parser_version,
        "marketplace_id": preview.marketplace_id.clone().unwrap_or_default(),
        "period_start": preview.period_start.map(|value| value.to_string()).unwrap_or_default(),
        "period_end": preview.period_end.map(|value| value.to_string()).unwrap_or_default(),
        "granularity": preview.snapshot.granularity,
        "date_granularity": preview.date_granularity,
        "asin_granularity": preview.asin_granularity,
        "timezone": preview.reporting_timezone.clone().unwrap_or_default(),
        "timezone_source_note": preview.timezone_source_note,
        "currency_code": preview.currency_code.clone().unwrap_or_default(),
        "data_freshness": preview.period_end.map(|value| value.to_string()),
        "confirmation_required": preview.confirmation_required,
        "operator_confirmed": preview.operator_confirmed,
        "metadata_provenance": preview.metadata_provenance,
        "missing_fields": preview.missing_fields,
        "warnings": preview.warnings,
        "metrics": preview.snapshot.metrics.iter()
            .filter(|metric| metric.dimension_type == "catalog")
            .map(|metric| json!({
                "metric_name": metric.metric_name,
                "dimension_type": metric.dimension_type,
                "dimension_key": metric.dimension_key,
                "value_numeric": metric.value_numeric.to_string(),
                "unit": metric.unit,
                "currency_code": metric.currency_code,
            }))
            .collect::<Vec<_>>(),
    })
}

async fn preview_manual_import(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(query): Query<ManualImportQuery>,
    raw: Bytes,
) -> Result<Json<Value>, ManualApiError> {
    require_marketplace(&state, &user, true)
        .await
        .map_err(|status| manual_api_error(status, "Marketplace Intelligence is not available"))?;
    let metadata = manual_metadata(&query)?;
    let preview = crate::manual_import::parse_manual_sales_and_traffic(&raw, &metadata)
        .map_err(|error| manual_api_error(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    validate_filename(&query.filename, preview.format)?;
    Ok(Json(preview_json(&preview)))
}

async fn execute_manual_import(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(query): Query<ManualImportQuery>,
    raw: Bytes,
) -> Result<Json<Value>, ManualApiError> {
    require_marketplace(&state, &user, true)
        .await
        .map_err(|status| manual_api_error(status, "Marketplace Intelligence is not available"))?;
    let expected_hash = query.confirm_hash.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "confirm_hash is required",
        )
    })?;
    let expected_granularity = query.confirm_granularity.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "confirm_granularity is required",
        )
    })?;
    let expected_report_type = query.confirm_report_type.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "confirm_report_type is required",
        )
    })?;
    if query
        .confirm_marketplace_id
        .as_deref()
        .is_none_or(str::is_empty)
        || query
            .confirm_currency_code
            .as_deref()
            .is_none_or(str::is_empty)
        || query.confirm_period_start.is_none()
        || query.confirm_period_end.is_none()
    {
        return Err(manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "marketplace, currency, period, report type, granularity and hash must be confirmed",
        ));
    }
    let metadata = manual_metadata(&query)?;
    let preview = crate::manual_import::parse_manual_sales_and_traffic(&raw, &metadata)
        .map_err(|error| manual_api_error(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    validate_filename(&query.filename, preview.format)?;
    preview
        .ensure_ready_for_import()
        .map_err(|error| manual_api_error(StatusCode::PRECONDITION_REQUIRED, error.to_string()))?;
    if expected_hash != preview.raw_sha256
        || expected_report_type != preview.report_type
        || expected_granularity != preview.snapshot.granularity
    {
        return Err(manual_api_error(
            StatusCode::PRECONDITION_FAILED,
            "confirmed hash, report type, or granularity does not match the validated report",
        ));
    }
    let marketplace_id = preview.marketplace_id.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "marketplace confirmation is missing",
        )
    })?;
    if marketplace_id.len() > 64 {
        return Err(manual_api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "marketplace identifier exceeds the storage boundary",
        ));
    }
    let timezone = preview.reporting_timezone.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "timezone confirmation is missing",
        )
    })?;
    let currency = preview.currency_code.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "currency confirmation is missing",
        )
    })?;
    let content_type = match preview.format {
        crate::manual_import::ManualReportFormat::Json => "application/json",
        crate::manual_import::ManualReportFormat::Csv => "text/csv",
        crate::manual_import::ManualReportFormat::Tsv => "text/tab-separated-values",
    };
    let stored = db::marketplace::store_manual_import(
        &state.pool,
        &db::marketplace::ManualImportStoreInput {
            uploaded_by: user.id,
            raw_sha256: &preview.raw_sha256,
            raw_content: &raw,
            content_type,
            detected_format: format_name(preview.format),
            marketplace_id,
            report_type: &preview.report_type,
            date_granularity: &preview.date_granularity,
            source_timezone: timezone,
            currency_code: Some(currency),
            parsed: &preview.snapshot,
        },
    )
    .await
    .map_err(|error| match error {
        db::marketplace::ManualImportStoreError::InvalidInput(message) => {
            manual_api_error(StatusCode::UNPROCESSABLE_ENTITY, message)
        }
        db::marketplace::ManualImportStoreError::MetadataConflict => manual_api_error(
            StatusCode::CONFLICT,
            "identical report bytes already exist with different confirmed metadata",
        ),
        db::marketplace::ManualImportStoreError::DuplicatePeriod => manual_api_error(
            StatusCode::CONFLICT,
            "a different report is already archived for this exact marketplace and comparison period",
        ),
        db::marketplace::ManualImportStoreError::Database(_) => manual_api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "validated report could not be committed atomically",
        ),
    })?;
    if let Err(error) = state.marketplace_worker.cycle(&state.pool).await {
        tracing::warn!(%error, run_id = %stored.run_id, "manual import analysis will retry asynchronously");
    }
    let analysis_id = db::marketplace::analysis_result_for_job(&state.pool, stored.analysis_job_id)
        .await
        .map_err(|_| {
            manual_api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "imported report detail could not be loaded",
            )
        })?
        .map(|analysis| analysis.id);
    Ok(Json(json!({
        "outcome": if stored.imported { "imported" } else { "already_imported" },
        "run_id": stored.run_id,
        "analysis_id": analysis_id,
        "comparison_generated": stored.comparison_generated,
        "preview": preview_json(&preview),
    })))
}

fn ads_preview_json(preview: &crate::manual_import::ManualAdsImportPreview) -> Value {
    json!({
        "sha256": preview.raw_sha256,
        "raw_bytes": preview.raw_bytes,
        "detected_format": format_name(preview.format),
        "report_type": preview.report_type,
        "parser_version": preview.parser_version,
        "marketplace_id": preview.marketplace_id.clone().unwrap_or_default(),
        "period_start": preview.period_start.map(|value| value.to_string()).unwrap_or_default(),
        "period_end": preview.period_end.map(|value| value.to_string()).unwrap_or_default(),
        "granularity": preview.snapshot.granularity,
        "date_granularity": preview.date_granularity,
        "timezone": preview.reporting_timezone.clone().unwrap_or_default(),
        "currency_code": preview.currency_code.clone().unwrap_or_default(),
        "data_freshness": preview.period_end.map(|value| value.to_string()),
        "ad_product": "SPONSORED_PRODUCTS",
        "report_level": "campaign",
        "attribution_window_days": preview.attribution_window_days,
        "confirmation_required": preview.confirmation_required,
        "operator_confirmed": preview.operator_confirmed,
        "metadata_provenance": preview.metadata_provenance,
        "missing_fields": preview.missing_fields,
        "warnings": preview.warnings,
        "metrics": preview.snapshot.metrics.iter()
            .filter(|metric| metric.dimension_type == "catalog")
            .map(|metric| json!({
                "metric_name": metric.metric_name,
                "dimension_type": metric.dimension_type,
                "dimension_key": metric.dimension_key,
                "value_numeric": metric.value_numeric.to_string(),
                "unit": metric.unit,
                "currency_code": metric.currency_code,
            }))
            .collect::<Vec<_>>(),
    })
}

async fn preview_manual_ads_import(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(query): Query<ManualImportQuery>,
    raw: Bytes,
) -> Result<Json<Value>, ManualApiError> {
    require_marketplace(&state, &user, true)
        .await
        .map_err(|status| manual_api_error(status, "Marketplace Intelligence is not available"))?;
    let metadata = manual_ads_metadata(&query)?;
    let preview = crate::manual_import::parse_manual_ads_campaign(&raw, &metadata)
        .map_err(|error| manual_api_error(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    validate_filename(&query.filename, preview.format)?;
    Ok(Json(ads_preview_json(&preview)))
}

async fn execute_manual_ads_import(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(query): Query<ManualImportQuery>,
    raw: Bytes,
) -> Result<Json<Value>, ManualApiError> {
    require_marketplace(&state, &user, true)
        .await
        .map_err(|status| manual_api_error(status, "Marketplace Intelligence is not available"))?;
    let expected_hash = query.confirm_hash.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "confirm_hash is required",
        )
    })?;
    let expected_granularity = query.confirm_granularity.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "confirm_granularity is required",
        )
    })?;
    let expected_report_type = query.confirm_report_type.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "confirm_report_type is required",
        )
    })?;
    if query
        .confirm_marketplace_id
        .as_deref()
        .is_none_or(str::is_empty)
        || query
            .confirm_currency_code
            .as_deref()
            .is_none_or(str::is_empty)
        || query.confirm_period_start.is_none()
        || query.confirm_period_end.is_none()
        || query.confirm_attribution_window_days.is_none()
    {
        return Err(manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "marketplace, currency, period, attribution window, report type, granularity and hash must be confirmed",
        ));
    }
    let metadata = manual_ads_metadata(&query)?;
    let preview = crate::manual_import::parse_manual_ads_campaign(&raw, &metadata)
        .map_err(|error| manual_api_error(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    validate_filename(&query.filename, preview.format)?;
    preview
        .ensure_ready_for_import()
        .map_err(|error| manual_api_error(StatusCode::PRECONDITION_REQUIRED, error.to_string()))?;
    if expected_hash != preview.raw_sha256
        || expected_report_type != preview.report_type
        || expected_granularity != preview.snapshot.granularity
    {
        return Err(manual_api_error(
            StatusCode::PRECONDITION_FAILED,
            "confirmed hash, report type, or granularity does not match the validated report",
        ));
    }
    let marketplace_id = preview.marketplace_id.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "marketplace confirmation is missing",
        )
    })?;
    let timezone = preview.reporting_timezone.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "timezone confirmation is missing",
        )
    })?;
    let currency = preview.currency_code.as_deref().ok_or_else(|| {
        manual_api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "currency confirmation is missing",
        )
    })?;
    let content_type = match preview.format {
        crate::manual_import::ManualReportFormat::Json => "application/json",
        crate::manual_import::ManualReportFormat::Csv => "text/csv",
        crate::manual_import::ManualReportFormat::Tsv => "text/tab-separated-values",
    };
    let stored = db::marketplace::store_manual_import(
        &state.pool,
        &db::marketplace::ManualImportStoreInput {
            uploaded_by: user.id,
            raw_sha256: &preview.raw_sha256,
            raw_content: &raw,
            content_type,
            detected_format: format_name(preview.format),
            marketplace_id,
            report_type: &preview.report_type,
            date_granularity: &preview.date_granularity,
            source_timezone: timezone,
            currency_code: Some(currency),
            parsed: &preview.snapshot,
        },
    )
    .await
    .map_err(|error| match error {
        db::marketplace::ManualImportStoreError::InvalidInput(message) => {
            manual_api_error(StatusCode::UNPROCESSABLE_ENTITY, message)
        }
        db::marketplace::ManualImportStoreError::MetadataConflict => manual_api_error(
            StatusCode::CONFLICT,
            "identical report bytes already exist with different confirmed metadata",
        ),
        db::marketplace::ManualImportStoreError::DuplicatePeriod => manual_api_error(
            StatusCode::CONFLICT,
            "a different report is already archived for this exact marketplace and comparison period",
        ),
        db::marketplace::ManualImportStoreError::Database(_) => manual_api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "validated report could not be committed atomically",
        ),
    })?;
    if let Err(error) = state.marketplace_worker.cycle(&state.pool).await {
        tracing::warn!(%error, run_id = %stored.run_id, "manual Ads import analysis will retry asynchronously");
    }
    let analysis_id = db::marketplace::analysis_result_for_job(&state.pool, stored.analysis_job_id)
        .await
        .map_err(|_| {
            manual_api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "imported Ads report detail could not be loaded",
            )
        })?
        .map(|analysis| analysis.id);
    Ok(Json(json!({
        "outcome": if stored.imported { "imported" } else { "already_imported" },
        "run_id": stored.run_id,
        "analysis_id": analysis_id,
        "comparison_generated": stored.comparison_generated,
        "preview": ads_preview_json(&preview),
    })))
}

async fn create_run(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(connection_id): Path<Uuid>,
    Json(input): Json<db::marketplace::CreateAmazonReportRunInput>,
) -> Result<Json<db::marketplace::AmazonReportRun>, StatusCode> {
    require_marketplace(&state, &user, true).await?;
    let connection = db::marketplace::get_connection(&state.pool, connection_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if connection.mode == "live" && user.role != "administrator" {
        return Err(StatusCode::FORBIDDEN);
    }
    if !connection.enabled
        || !db::marketplace::marketplace_exists(&state.pool, connection_id, &input.marketplace_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        || !db::marketplace::report_type_is_allowed_for_connection(&connection, &input.report_type)
        || !db::marketplace::report_options_are_supported(&input.report_type, &input.report_options)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let pilot_enabled = db::modules::is_enabled(&state.pool, db::modules::AMAZON_READ_ONLY_PILOT)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if pilot_enabled
        && connection.mode == "live"
        && !crate::marketplace::pilot_live_request_is_safe(&state.pool, &connection, &input)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        return Err(StatusCode::PRECONDITION_FAILED);
    }
    let run = db::marketplace::create_manual_run(&state.pool, connection_id, &input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state
        .marketplace_worker
        .cycle(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    db::marketplace::get_run_detail(&state.pool, run.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(|detail| Json(detail.run))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn upsert_schedule(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(connection_id): Path<Uuid>,
    Json(input): Json<db::marketplace::AmazonReportScheduleInput>,
) -> Result<Json<db::marketplace::AmazonReportSchedule>, StatusCode> {
    require_marketplace(&state, &user, true).await?;
    if user.role != "administrator" {
        return Err(StatusCode::FORBIDDEN);
    }
    let connection = db::marketplace::get_connection(&state.pool, connection_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let definition =
        db::marketplace::report_definition(&input.report_type).ok_or(StatusCode::BAD_REQUEST)?;
    if !connection.enabled
        || !definition.schedule_supported
        || !db::marketplace::marketplace_exists(&state.pool, connection_id, &input.marketplace_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        || !db::marketplace::report_type_is_allowed_for_connection(&connection, &input.report_type)
        || !db::marketplace::report_options_are_supported(&input.report_type, &input.report_options)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    db::marketplace::upsert_schedule(&state.pool, connection_id, &input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::BAD_REQUEST)
}

#[derive(Deserialize)]
struct TotalAnalysisInput {
    marketplace_id: String,
    report_type: String,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
}

async fn create_total_analysis(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(connection_id): Path<Uuid>,
    Json(input): Json<TotalAnalysisInput>,
) -> Result<Json<db::marketplace::AnalysisJob>, StatusCode> {
    require_marketplace(&state, &user, true).await?;
    if input.period_start >= input.period_end
        || db::marketplace::report_definition(&input.report_type)
            .is_none_or(|definition| !definition.analysis_capable)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let job = db::marketplace::create_total_analysis(
        &state.pool,
        connection_id,
        &input.marketplace_id,
        &input.report_type,
        input.period_start,
        input.period_end,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state
        .marketplace_worker
        .cycle(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(job))
}

async fn run_detail(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(run_id): Path<Uuid>,
) -> Result<Json<db::marketplace::AmazonRunDetail>, StatusCode> {
    require_marketplace(&state, &user, false).await?;
    db::marketplace::get_run_detail(&state.pool, run_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn raw_document(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(run_id): Path<Uuid>,
) -> Result<Response, StatusCode> {
    require_marketplace(&state, &user, false).await?;
    if user.role != "administrator" {
        return Err(StatusCode::FORBIDDEN);
    }
    let document = db::marketplace::raw_document(&state.pool, run_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let content_type = document
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_owned());
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&content_type)
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=amazon-report.raw",
        )
        .body(Body::from(document.content))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug, Serialize)]
struct WeeklyStrategyView {
    anchor_analysis_id: Option<Uuid>,
    current_payload_sha256: Option<String>,
    assessment_payload_sha256: Option<String>,
    status: crate::strategy_ai::StrategyAiStatus,
    can_run: bool,
    block_reason: Option<&'static str>,
    week_start: NaiveDate,
    next_available_at: DateTime<Utc>,
    source_analysis_count: usize,
    product_observed_count: usize,
    product_mapped_count: usize,
    product_context_count: usize,
    business_knowledge_imported: bool,
    business_knowledge_source_count: usize,
    business_knowledge_entry_count: usize,
    business_knowledge_sha256: Option<String>,
    previous_run_context: bool,
    cached: bool,
    assessment: Option<Value>,
    assessment_week_start: Option<NaiveDate>,
    assessment_model: Option<String>,
    assessment_prompt_version: Option<String>,
    provider_request_id_redacted: Option<String>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrategyAssessmentRequest {
    confirmed_payload_sha256: String,
    confirmed_aggregate_only: bool,
}

#[derive(Debug, Serialize)]
struct BusinessKnowledgeView {
    imported: bool,
    cached: bool,
    version: &'static str,
    source_manifest_sha256: Option<String>,
    content_sha256: Option<String>,
    source_count: usize,
    entry_count: usize,
    created_at: Option<DateTime<Utc>>,
    raw_documents_stored: bool,
    mutable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BusinessKnowledgeImportRequest {
    knowledge: crate::strategy_ai::BusinessKnowledge,
    confirmed_business_only: bool,
    confirmed_no_secrets_or_pii: bool,
}

#[derive(Debug, Serialize)]
struct ProductMappingView {
    coverage: db::marketplace::ProductMappingCoverage,
    mappings: Vec<ProductMappingItemView>,
    observed: Vec<db::marketplace::ObservedAmazonProduct>,
}

#[derive(Debug, Serialize)]
struct ProductMappingItemView {
    id: Uuid,
    connection_id: Uuid,
    marketplace_id: String,
    child_asin: String,
    revision: i32,
    brand: String,
    product_family: String,
    variant: String,
    pack_size: Option<String>,
    sku: Option<String>,
    evidence_source: String,
    enabled: bool,
    created_at: DateTime<Utc>,
}

impl From<db::marketplace::AmazonProductMapping> for ProductMappingItemView {
    fn from(mapping: db::marketplace::AmazonProductMapping) -> Self {
        Self {
            id: mapping.id,
            connection_id: mapping.connection_id,
            marketplace_id: mapping.marketplace_id,
            child_asin: mapping.child_asin,
            revision: mapping.revision,
            brand: mapping.brand,
            product_family: mapping.product_family,
            variant: mapping.variant,
            pack_size: mapping.pack_size,
            sku: mapping.sku,
            evidence_source: mapping.evidence_source,
            enabled: mapping.enabled,
            created_at: mapping.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductMappingRequest {
    connection_id: Uuid,
    marketplace_id: String,
    child_asin: String,
    brand: String,
    product_family: String,
    variant: String,
    pack_size: Option<String>,
    sku: Option<String>,
    evidence_source: String,
    enabled: bool,
    confirmed_business_mapping: bool,
}

async fn product_mappings(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<ProductMappingView>, StatusCode> {
    require_marketplace(&state, &user, false).await?;
    if user.role != "administrator" {
        return Err(StatusCode::FORBIDDEN);
    }
    let (coverage, mappings, observed) = tokio::try_join!(
        db::marketplace::product_mapping_coverage(&state.pool),
        db::marketplace::active_product_mappings(&state.pool),
        db::marketplace::observed_products(&state.pool),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ProductMappingView {
        coverage,
        mappings: mappings.into_iter().map(Into::into).collect(),
        observed,
    }))
}

async fn store_product_mapping(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<ProductMappingRequest>,
) -> Result<Json<Value>, StatusCode> {
    require_marketplace(&state, &user, true).await?;
    if user.role != "administrator" || !input.confirmed_business_mapping {
        return Err(StatusCode::FORBIDDEN);
    }
    let connection = db::marketplace::get_connection(&state.pool, input.connection_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if connection.mode != "live"
        || !connection.enabled
        || !db::marketplace::marketplace_exists(
            &state.pool,
            input.connection_id,
            input.marketplace_id.trim(),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let child_asin = input.child_asin.trim().to_ascii_uppercase();
    let product_family = input.product_family.trim();
    let variant = input.variant.trim();
    let pack_size = normalized_optional_mapping_value(input.pack_size.as_deref(), 40)?;
    let sku = normalized_optional_sku(input.sku.as_deref())?;
    if !valid_child_asin(&child_asin)
        || !matches!(
            input.brand.as_str(),
            "mantle" | "sphagnum" | "shared" | "other"
        )
        || !matches!(
            input.evidence_source.as_str(),
            "mantle_wiki" | "seller_central" | "operator_confirmed"
        )
        || !valid_mapping_value(product_family, 80)
        || !valid_mapping_value(variant, 120)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !db::marketplace::observed_product_exists(
        &state.pool,
        input.connection_id,
        input.marketplace_id.trim(),
        &child_asin,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let stored = db::marketplace::store_product_mapping_revision(
        &state.pool,
        &db::marketplace::AmazonProductMappingInput {
            connection_id: input.connection_id,
            marketplace_id: input.marketplace_id.trim(),
            child_asin: &child_asin,
            brand: &input.brand,
            product_family,
            variant,
            pack_size: pack_size.as_deref(),
            sku: sku.as_deref(),
            evidence_source: &input.evidence_source,
            enabled: input.enabled,
            confirmed_by: user.id,
        },
    )
    .await
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    let mapping = ProductMappingItemView::from(stored.0);
    Ok(Json(json!({
        "mapping": mapping,
        "outcome": if stored.1 { "stored" } else { "unchanged" },
        "amazon_mutation": false,
    })))
}

fn valid_child_asin(value: &str) -> bool {
    value.len() == 10
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn valid_mapping_value(value: &str, max_chars: usize) -> bool {
    let lower = value.to_ascii_lowercase();
    !value.is_empty()
        && value.chars().count() <= max_chars
        && !value.contains('@')
        && ![
            "http://",
            "https://",
            "sk-",
            "api_key",
            "secret",
            "token",
            "ignore previous",
            "system prompt",
            "assistant:",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
        && value.chars().all(|character| {
            character.is_alphanumeric()
                || character.is_whitespace()
                || "-_./()&+%,®".contains(character)
        })
}

fn normalized_optional_mapping_value(
    value: Option<&str>,
    max_chars: usize,
) -> Result<Option<String>, StatusCode> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            valid_mapping_value(value, max_chars)
                .then(|| value.to_owned())
                .ok_or(StatusCode::BAD_REQUEST)
        })
        .transpose()
}

fn normalized_optional_sku(value: Option<&str>) -> Result<Option<String>, StatusCode> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            (value.len() <= 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_./".contains(&byte)))
            .then(|| value.to_owned())
            .ok_or(StatusCode::BAD_REQUEST)
        })
        .transpose()
}

struct StrategyRouteError {
    status: StatusCode,
    code: &'static str,
    retry_after_seconds: Option<u64>,
}

impl StrategyRouteError {
    fn new(status: StatusCode, code: &'static str) -> Self {
        Self {
            status,
            code,
            retry_after_seconds: None,
        }
    }
}

impl IntoResponse for StrategyRouteError {
    fn into_response(self) -> Response {
        let mut body = json!({ "error": self.code });
        if let Some(retry_after_seconds) = self.retry_after_seconds {
            body["retry_after_seconds"] = json!(retry_after_seconds);
        }
        let mut response = (self.status, Json(body)).into_response();
        if let Some(retry_after_seconds) = self.retry_after_seconds {
            if let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

async fn require_strategy_admin(
    state: &AppState,
    user: &db::users::User,
) -> Result<(), StrategyRouteError> {
    require_marketplace(state, user, false)
        .await
        .map_err(|status| StrategyRouteError::new(status, "marketplace_unavailable"))?;
    if user.role != "administrator" {
        return Err(StrategyRouteError::new(
            StatusCode::FORBIDDEN,
            "administrator_required",
        ));
    }
    Ok(())
}

async fn strategy_status(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<crate::strategy_ai::StrategyAiStatus>, StrategyRouteError> {
    require_strategy_admin(&state, &user).await?;
    let provider_key_configured = state
        .provider_secrets
        .openai_api_key()
        .await
        .map_err(|_| {
            StrategyRouteError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "provider_secret_store_failed",
            )
        })?
        .is_some();
    Ok(Json(
        state
            .strategy_ai
            .status_with_provider_key(provider_key_configured),
    ))
}

fn business_knowledge_view(
    record: Option<&db::marketplace::MantleBusinessKnowledge>,
    cached: bool,
) -> BusinessKnowledgeView {
    BusinessKnowledgeView {
        imported: record.is_some(),
        cached,
        version: crate::strategy_ai::BUSINESS_KNOWLEDGE_VERSION,
        source_manifest_sha256: record.map(|value| value.source_manifest_sha256.clone()),
        content_sha256: record.map(|value| value.content_sha256.clone()),
        source_count: record.map_or(0, |value| value.source_count.max(0) as usize),
        entry_count: record.map_or(0, |value| value.entry_count.max(0) as usize),
        created_at: record.map(|value| value.created_at),
        raw_documents_stored: false,
        mutable: false,
    }
}

async fn business_knowledge_status(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<BusinessKnowledgeView>, StrategyRouteError> {
    require_strategy_admin(&state, &user).await?;
    let record = db::marketplace::mantle_business_knowledge(&state.pool)
        .await
        .map_err(|_| {
            StrategyRouteError::new(StatusCode::INTERNAL_SERVER_ERROR, "database_error")
        })?;
    Ok(Json(business_knowledge_view(record.as_ref(), false)))
}

async fn import_business_knowledge(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(request): Json<BusinessKnowledgeImportRequest>,
) -> Result<Json<BusinessKnowledgeView>, StrategyRouteError> {
    require_strategy_admin(&state, &user).await?;
    if !request.confirmed_business_only || !request.confirmed_no_secrets_or_pii {
        return Err(StrategyRouteError::new(
            StatusCode::PRECONDITION_FAILED,
            "business_knowledge_confirmation_required",
        ));
    }
    let prepared =
        crate::strategy_ai::prepare_business_knowledge(request.knowledge).map_err(|error| {
            match error {
                crate::strategy_ai::StrategyAiError::PayloadTooLarge => StrategyRouteError::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "business_knowledge_too_large",
                ),
                _ => StrategyRouteError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "business_knowledge_invalid",
                ),
            }
        })?;
    let input = db::marketplace::StoreMantleBusinessKnowledge {
        source_manifest_sha256: &prepared.source_manifest_sha256,
        content_sha256: &prepared.content_sha256,
        source_count: prepared.source_count as i32,
        entry_count: prepared.entry_count as i32,
        knowledge: &prepared.value,
        created_by: user.id,
    };
    let (stored, was_inserted) =
        db::marketplace::store_mantle_business_knowledge(&state.pool, &input)
            .await
            .map_err(|_| {
                StrategyRouteError::new(StatusCode::INTERNAL_SERVER_ERROR, "database_error")
            })?;
    if stored.content_sha256 != prepared.content_sha256 {
        return Err(StrategyRouteError::new(
            StatusCode::CONFLICT,
            "business_knowledge_already_imported",
        ));
    }
    Ok(Json(business_knowledge_view(Some(&stored), !was_inserted)))
}

struct WeeklyStrategyContext {
    week_start: NaiveDate,
    next_available_at: DateTime<Utc>,
    anchor_analysis_id: Option<Uuid>,
    prepared: Option<crate::strategy_ai::PreparedStrategyInput>,
    business_knowledge: Option<db::marketplace::MantleBusinessKnowledge>,
    product_coverage: db::marketplace::ProductMappingCoverage,
    previous: Option<db::marketplace::AiStrategyAssessment>,
    current: Option<db::marketplace::AiStrategyAssessment>,
}

async fn load_weekly_strategy_context(
    state: &AppState,
) -> Result<WeeklyStrategyContext, StrategyRouteError> {
    let (week_start, next_available_at) =
        db::marketplace::current_mantle_strategy_week(&state.pool)
            .await
            .map_err(|_| {
                StrategyRouteError::new(StatusCode::INTERNAL_SERVER_ERROR, "database_error")
            })?;
    let analyses = db::marketplace::recent_analysis_results_for_strategy(&state.pool)
        .await
        .map_err(|_| {
            StrategyRouteError::new(StatusCode::INTERNAL_SERVER_ERROR, "database_error")
        })?;
    let previous =
        db::marketplace::latest_ai_strategy_assessment_before_week(&state.pool, week_start)
            .await
            .map_err(|_| {
                StrategyRouteError::new(StatusCode::INTERNAL_SERVER_ERROR, "database_error")
            })?;
    let current = db::marketplace::ai_strategy_assessment_for_week(&state.pool, week_start)
        .await
        .map_err(|_| {
            StrategyRouteError::new(StatusCode::INTERNAL_SERVER_ERROR, "database_error")
        })?;
    let business_knowledge = db::marketplace::mantle_business_knowledge(&state.pool)
        .await
        .map_err(|_| {
            StrategyRouteError::new(StatusCode::INTERNAL_SERVER_ERROR, "database_error")
        })?;
    let (product_metrics, product_coverage) = tokio::try_join!(
        db::marketplace::recent_product_strategy_metrics(&state.pool, 13, 24),
        db::marketplace::product_mapping_coverage(&state.pool),
    )
    .map_err(|_| StrategyRouteError::new(StatusCode::INTERNAL_SERVER_ERROR, "database_error"))?;
    let results = analyses
        .iter()
        .map(|analysis| analysis.result.clone())
        .collect::<Vec<_>>();
    let prepared = match crate::strategy_ai::prepare_weekly_strategy_input_with_product_context(
        &results,
        previous.as_ref().map(|record| &record.result),
        business_knowledge.as_ref().map(|record| &record.knowledge),
        &product_metrics,
        Some(&product_coverage),
    ) {
        Ok(prepared) => Some(prepared),
        Err(crate::strategy_ai::StrategyAiError::InvalidResponse) => None,
        Err(error) => {
            return Err(match error {
                crate::strategy_ai::StrategyAiError::PayloadTooLarge => StrategyRouteError::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "aggregate_payload_too_large",
                ),
                _ => StrategyRouteError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "aggregate_payload_invalid",
                ),
            })
        }
    };
    Ok(WeeklyStrategyContext {
        week_start,
        next_available_at,
        anchor_analysis_id: analyses.first().map(|analysis| analysis.id),
        prepared,
        business_knowledge,
        product_coverage,
        previous,
        current,
    })
}

fn weekly_strategy_view(
    context: &WeeklyStrategyContext,
    cached: bool,
    status: crate::strategy_ai::StrategyAiStatus,
) -> WeeklyStrategyView {
    let displayed = context.current.as_ref().or(context.previous.as_ref());
    let source_analysis_count = context
        .prepared
        .as_ref()
        .and_then(|prepared| prepared.payload.get("analyses"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let product_context_count = context
        .prepared
        .as_ref()
        .and_then(|prepared| prepared.payload.pointer("/product_evidence/products"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let block_reason = if context.current.is_some() {
        Some("weekly_limit_reached")
    } else if context.business_knowledge.is_none() {
        Some("business_knowledge_missing")
    } else if context.prepared.is_none() {
        Some("no_analysis_data")
    } else {
        status.reason
    };
    WeeklyStrategyView {
        anchor_analysis_id: context.anchor_analysis_id,
        current_payload_sha256: context
            .prepared
            .as_ref()
            .map(|prepared| prepared.payload_sha256.clone()),
        assessment_payload_sha256: displayed.map(|record| record.payload_sha256.clone()),
        can_run: block_reason.is_none(),
        block_reason,
        week_start: context.week_start,
        next_available_at: context.next_available_at,
        source_analysis_count,
        product_observed_count: context.product_coverage.observed_products.max(0) as usize,
        product_mapped_count: context.product_coverage.enabled_mapped_products.max(0) as usize,
        product_context_count,
        business_knowledge_imported: context.business_knowledge.is_some(),
        business_knowledge_source_count: context
            .business_knowledge
            .as_ref()
            .map_or(0, |value| value.source_count.max(0) as usize),
        business_knowledge_entry_count: context
            .business_knowledge
            .as_ref()
            .map_or(0, |value| value.entry_count.max(0) as usize),
        business_knowledge_sha256: context
            .business_knowledge
            .as_ref()
            .map(|value| value.content_sha256.clone()),
        previous_run_context: context
            .prepared
            .as_ref()
            .and_then(|prepared| prepared.payload.get("previous_ai_run"))
            .is_some_and(|previous| !previous.is_null()),
        status,
        cached,
        assessment: displayed.map(|record| record.result.clone()),
        assessment_week_start: displayed.and_then(|record| record.week_start),
        assessment_model: displayed.map(|record| record.model_name.clone()),
        assessment_prompt_version: displayed.map(|record| record.prompt_version.clone()),
        provider_request_id_redacted: displayed
            .and_then(|record| record.provider_request_id_redacted.clone()),
        input_tokens: displayed.and_then(|record| record.input_tokens),
        output_tokens: displayed.and_then(|record| record.output_tokens),
        created_at: displayed.map(|record| record.created_at),
    }
}

async fn weekly_strategy_preview(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<WeeklyStrategyView>, StrategyRouteError> {
    require_strategy_admin(&state, &user).await?;
    let context = load_weekly_strategy_context(&state).await?;
    let cached = context.current.is_some();
    let provider_key_configured = state
        .provider_secrets
        .openai_api_key()
        .await
        .map_err(|_| {
            StrategyRouteError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "provider_secret_store_failed",
            )
        })?
        .is_some();
    let status = state
        .strategy_ai
        .status_with_provider_key(provider_key_configured);
    Ok(Json(weekly_strategy_view(&context, cached, status)))
}

async fn create_weekly_strategy_assessment(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(request): Json<StrategyAssessmentRequest>,
) -> Result<Json<WeeklyStrategyView>, StrategyRouteError> {
    require_strategy_admin(&state, &user).await?;
    let mut context = load_weekly_strategy_context(&state).await?;
    let provider_api_key = state
        .provider_secrets
        .openai_api_key()
        .await
        .map_err(|_| {
            StrategyRouteError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "provider_secret_store_failed",
            )
        })?
        .map(Zeroizing::new);
    let status = state
        .strategy_ai
        .status_with_provider_key(provider_api_key.is_some());
    if context.current.is_some() {
        return Ok(Json(weekly_strategy_view(&context, true, status)));
    }
    if context.business_knowledge.is_none() {
        return Err(StrategyRouteError::new(
            StatusCode::PRECONDITION_FAILED,
            "business_knowledge_missing",
        ));
    }
    let prepared = context.prepared.as_ref().ok_or_else(|| {
        StrategyRouteError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "aggregate_payload_invalid",
        )
    })?;
    if !request.confirmed_aggregate_only
        || request.confirmed_payload_sha256 != prepared.payload_sha256
    {
        return Err(StrategyRouteError::new(
            StatusCode::PRECONDITION_FAILED,
            "aggregate_confirmation_mismatch",
        ));
    }
    let analysis_id = context.anchor_analysis_id.ok_or_else(|| {
        StrategyRouteError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "aggregate_payload_invalid",
        )
    })?;
    let completion = state
        .strategy_ai
        .assess_with_api_key(
            prepared,
            &crate::strategy_ai::safety_identifier(user.id),
            provider_api_key.as_deref().map(String::as_str),
        )
        .await
        .map_err(strategy_provider_error)?;
    let result = serde_json::to_value(&completion.assessment).map_err(|_| {
        StrategyRouteError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "assessment_serialization_failed",
        )
    })?;
    let input = db::marketplace::StoreAiStrategyAssessment {
        analysis_id,
        payload_sha256: &prepared.payload_sha256,
        model_name: state.strategy_ai.model(),
        prompt_version: crate::strategy_ai::STRATEGY_PROMPT_VERSION,
        result: &result,
        provider_request_id_redacted: completion.provider_request_id_redacted.as_deref(),
        input_tokens: completion.input_tokens,
        output_tokens: completion.output_tokens,
        week_start: Some(context.week_start),
        previous_assessment_id: context.previous.as_ref().map(|record| record.id),
        created_by: user.id,
    };
    let (stored, was_inserted) = db::marketplace::store_ai_strategy_assessment(&state.pool, &input)
        .await
        .map_err(|_| {
            StrategyRouteError::new(StatusCode::INTERNAL_SERVER_ERROR, "database_error")
        })?;
    context.current = Some(stored);
    Ok(Json(weekly_strategy_view(&context, !was_inserted, status)))
}

fn strategy_provider_error(error: crate::strategy_ai::StrategyAiError) -> StrategyRouteError {
    use crate::strategy_ai::StrategyAiError;

    match error {
        StrategyAiError::NotConfigured => {
            StrategyRouteError::new(StatusCode::SERVICE_UNAVAILABLE, "openai_not_configured")
        }
        StrategyAiError::Busy => {
            StrategyRouteError::new(StatusCode::CONFLICT, "strategy_assessment_busy")
        }
        StrategyAiError::PayloadTooLarge => {
            StrategyRouteError::new(StatusCode::PAYLOAD_TOO_LARGE, "aggregate_payload_too_large")
        }
        StrategyAiError::AuthenticationFailed => {
            StrategyRouteError::new(StatusCode::BAD_GATEWAY, "openai_authentication_failed")
        }
        StrategyAiError::RateLimited {
            retry_after_seconds,
        } => StrategyRouteError {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "openai_rate_limited",
            retry_after_seconds,
        },
        StrategyAiError::Refused => {
            StrategyRouteError::new(StatusCode::UNPROCESSABLE_ENTITY, "openai_refused")
        }
        StrategyAiError::InvalidResponse => {
            StrategyRouteError::new(StatusCode::BAD_GATEWAY, "openai_invalid_response")
        }
        StrategyAiError::InvalidResearchResponse => {
            StrategyRouteError::new(StatusCode::BAD_GATEWAY, "openai_research_invalid_response")
        }
        StrategyAiError::InvalidAssessmentResponse => StrategyRouteError::new(
            StatusCode::BAD_GATEWAY,
            "openai_assessment_invalid_response",
        ),
        StrategyAiError::InvalidAssessmentJson => {
            StrategyRouteError::new(StatusCode::BAD_GATEWAY, "openai_assessment_invalid_json")
        }
        StrategyAiError::InvalidAssessmentSources => {
            StrategyRouteError::new(StatusCode::BAD_GATEWAY, "openai_assessment_invalid_sources")
        }
        StrategyAiError::InvalidAssessmentValidation => StrategyRouteError::new(
            StatusCode::BAD_GATEWAY,
            "openai_assessment_validation_failed",
        ),
        StrategyAiError::ResearchUnavailable => {
            StrategyRouteError::new(StatusCode::BAD_GATEWAY, "openai_research_unavailable")
        }
        StrategyAiError::AssessmentUnavailable => {
            StrategyRouteError::new(StatusCode::BAD_GATEWAY, "openai_assessment_unavailable")
        }
        StrategyAiError::ProviderUnavailable => {
            StrategyRouteError::new(StatusCode::BAD_GATEWAY, "openai_unavailable")
        }
    }
}

async fn export_analysis(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(analysis_id): Path<Uuid>,
    Query(query): Query<AnalysisExportQuery>,
) -> Result<Response, StatusCode> {
    require_marketplace(&state, &user, false).await?;
    let analysis = db::marketplace::analysis_result(&state.pool, analysis_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let export = crate::marketplace::pii_safe_analysis_export(&analysis.result);
    let envelope = json!({
        "analysis_id": analysis.id,
        "strategy": analysis.strategy,
        "ruleset_version": analysis.prompt_version,
        "payload_sha256": analysis.payload_sha256,
        "created_at": analysis.created_at,
        "result": export,
    });
    let format = query.format.as_deref().unwrap_or("json");
    let (content, content_type, extension) = match format {
        "json" => (
            serde_json::to_vec_pretty(&envelope).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            "application/json; charset=utf-8",
            "json",
        ),
        "markdown" => (
            analysis_markdown(&envelope).into_bytes(),
            "text/markdown; charset=utf-8",
            "md",
        ),
        "csv" => (
            analysis_csv(&envelope).into_bytes(),
            "text/csv; charset=utf-8",
            "csv",
        ),
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=marketplace-analysis-{analysis_id}.{extension}"),
        )
        .body(Body::from(content))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Default, Deserialize)]
struct AnalysisExportQuery {
    format: Option<String>,
}

fn text_value(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(serde_json::Value::String(value)) => value.clone(),
        Some(serde_json::Value::Null) | None => String::new(),
        Some(value) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn analysis_markdown(envelope: &serde_json::Value) -> String {
    let result = &envelope["result"];
    let context = &result["context"];
    let mut output = format!(
        "# Amazon Sales and Traffic analysis\n\n- Analysis: `{}`\n- Period: {} to {}\n- Marketplace: {}\n- Report type: {}\n- Granularity: {}\n- Parser: {}\n- Data freshness: {}\n- Currency: {}\n\n",
        text_value(envelope.get("analysis_id")),
        text_value(context.get("period_start")),
        text_value(context.get("period_end")),
        text_value(context.get("marketplace")),
        text_value(context.get("report_type")),
        text_value(context.get("granularity")),
        text_value(context.get("parser_version")),
        text_value(context.get("data_freshness")),
        text_value(context.get("currency")),
    );
    for (heading, field) in [
        ("Facts", "facts"),
        ("Supported derivations", "derived_observations"),
        ("Period changes", "changes_since_last_run"),
        ("Hypotheses", "hypotheses"),
        ("Possible measures", "options"),
        ("Missing evidence", "missing_evidence"),
        ("Open questions", "open_questions"),
    ] {
        output.push_str(&format!("## {heading}\n\n"));
        let values = result
            .get(field)
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        if values.is_empty() {
            output.push_str("- None recorded.\n\n");
        } else {
            for value in values {
                output.push_str(&format!("- {}\n", text_value(Some(&value))));
            }
            output.push('\n');
        }
    }
    output.push_str("## Uncertainty\n\n");
    output.push_str(&text_value(result.get("uncertainty")));
    output.push_str(
        "\n\n> This export contains aggregate summaries only and cannot make Amazon changes.\n",
    );
    output
}

fn csv_cell(value: impl AsRef<str>) -> String {
    format!("\"{}\"", value.as_ref().replace('"', "\"\""))
}

fn analysis_csv(envelope: &serde_json::Value) -> String {
    let result = &envelope["result"];
    let mut rows = vec![[
        "classification",
        "metric_or_item",
        "current",
        "previous",
        "delta",
        "percent_change",
        "trend",
        "unit",
        "currency",
        "uncertainty",
    ]
    .iter()
    .map(csv_cell)
    .collect::<Vec<_>>()
    .join(",")];
    for field in [
        "period_start",
        "period_end",
        "marketplace",
        "report_type",
        "granularity",
        "parser_version",
        "data_freshness",
        "source_timezone",
        "currency",
        "missing_fields",
    ] {
        let value = result.get("context").and_then(|context| context.get(field));
        rows.push(
            [
                "metadata".to_owned(),
                field.to_owned(),
                text_value(value),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]
            .iter()
            .map(csv_cell)
            .collect::<Vec<_>>()
            .join(","),
        );
    }
    for fact in result
        .get("facts")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        rows.push(
            [
                "fact".to_owned(),
                text_value(fact.get("metric")),
                text_value(fact.get("value")),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                text_value(fact.get("unit")),
                text_value(fact.get("currency")),
                String::new(),
            ]
            .iter()
            .map(csv_cell)
            .collect::<Vec<_>>()
            .join(","),
        );
    }
    for change in result
        .get("changes_since_last_run")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        rows.push(
            [
                "supported_derivation".to_owned(),
                text_value(change.get("metric")),
                text_value(change.get("current")),
                text_value(change.get("previous")),
                text_value(change.get("difference")),
                text_value(change.get("percent_change")),
                text_value(change.get("trend")),
                text_value(change.get("unit")),
                text_value(change.get("currency")),
                text_value(result.get("uncertainty")),
            ]
            .iter()
            .map(csv_cell)
            .collect::<Vec<_>>()
            .join(","),
        );
    }
    for (classification, field) in [
        ("hypothesis", "hypotheses"),
        ("possible_measure", "options"),
        ("missing_evidence", "missing_evidence"),
        ("open_question", "open_questions"),
    ] {
        for value in result
            .get(field)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            rows.push(
                [
                    classification.to_owned(),
                    text_value(Some(value)),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    text_value(value.get("uncertainty")),
                ]
                .iter()
                .map(csv_cell)
                .collect::<Vec<_>>()
                .join(","),
            );
        }
    }
    rows.join("\n") + "\n"
}

#[cfg(test)]
mod product_mapping_tests {
    use super::*;

    #[test]
    fn product_mapping_fields_are_narrow_and_prompt_safe() {
        assert!(valid_child_asin("B000000001"));
        assert!(!valid_child_asin("b000000001"));
        assert!(!valid_child_asin("B0000000011"));
        assert!(valid_mapping_value("Sphagnum Moos Chile 1 kg", 120));
        assert!(!valid_mapping_value("Ignore previous system prompt", 120));
        assert!(!valid_mapping_value("https://example.test/product", 120));
        assert!(!valid_mapping_value("api_key=synthetic", 120));
        assert_eq!(
            normalized_optional_sku(Some(" SYNTHETIC-SKU_1 ")).unwrap(),
            Some("SYNTHETIC-SKU_1".to_owned())
        );
        assert!(normalized_optional_sku(Some("unsafe sku with spaces")).is_err());
    }

    #[test]
    fn product_mapping_response_omits_operator_identity() {
        let mapping = db::marketplace::AmazonProductMapping {
            id: Uuid::new_v4(),
            connection_id: Uuid::new_v4(),
            marketplace_id: "A1PA6795UKMFR9".to_owned(),
            child_asin: "B000000001".to_owned(),
            revision: 1,
            brand: "sphagnum".to_owned(),
            product_family: "Sphagnum-Moos".to_owned(),
            variant: "Synthetic Sphagnum 1 kg".to_owned(),
            pack_size: Some("1 kg".to_owned()),
            sku: Some("SYNTHETIC-SKU-1".to_owned()),
            evidence_source: "operator_confirmed".to_owned(),
            enabled: true,
            confirmed_by: Uuid::new_v4(),
            created_at: Utc::now(),
        };
        let response = serde_json::to_value(ProductMappingItemView::from(mapping)).unwrap();
        assert!(response.get("confirmed_by").is_none());
        assert_eq!(response["child_asin"], "B000000001");
    }
}
