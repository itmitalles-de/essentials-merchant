use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(overview))
        .route("/connections", post(upsert_connection))
        .route("/demo", post(create_demo))
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
) -> Result<Response, StatusCode> {
    require_marketplace(&state, &user, false).await?;
    let analysis = db::marketplace::analysis_result(&state.pool, analysis_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let export = crate::marketplace::pii_safe_analysis_export(&analysis.result);
    let content = serde_json::to_vec_pretty(&json!({
        "analysis_id": analysis.id,
        "strategy": analysis.strategy,
        "ruleset_version": analysis.prompt_version,
        "payload_sha256": analysis.payload_sha256,
        "created_at": analysis.created_at,
        "result": export,
    }))
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=marketplace-analysis-{analysis_id}.json"),
        )
        .body(Body::from(content))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
