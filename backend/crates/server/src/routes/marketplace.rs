use axum::body::{Body, Bytes};
use axum::extract::DefaultBodyLimit;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

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
    db::marketplace::overview(&state.pool)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
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
