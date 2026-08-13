use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::state::AppState;

const IDEMPOTENCY_HEADER: &str = "idempotency-key";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_diagnostics))
        .route("/events/{source}/{event_id}/requeue", post(requeue))
}

async fn get_diagnostics(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<db::commerce::IntegrationDiagnostics>, StatusCode> {
    require_admin(&user)?;
    require_integration_module(&state).await?;
    db::commerce::integration_diagnostics(&state.pool)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(%error, "could not load integration diagnostics");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn requeue(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    headers: HeaderMap,
    Path((source, event_id)): Path<(String, String)>,
) -> Result<(StatusCode, Json<db::commerce::RequeueResult>), (StatusCode, Json<Value>)> {
    require_admin(&user).map_err(|status| (status, Json(json!({ "error": "forbidden" }))))?;
    require_integration_module(&state)
        .await
        .map_err(|status| (status, Json(json!({ "error": "module_disabled" }))))?;
    let idempotency_key = headers
        .get(IDEMPOTENCY_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "idempotency_key_required" })),
            )
        })?;
    db::commerce::manually_requeue(&state.pool, user.id, &source, &event_id, idempotency_key)
        .await
        .map(|result| (StatusCode::ACCEPTED, Json(result)))
        .map_err(|error| match error {
            db::commerce::RequeueError::NotFound => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "event_not_found" })),
            ),
            db::commerce::RequeueError::NotDead => (
                StatusCode::CONFLICT,
                Json(json!({ "error": "event_not_dead" })),
            ),
            db::commerce::RequeueError::UnsupportedSource => (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid_requeue_request" })),
            ),
            db::commerce::RequeueError::Sqlx(error) => {
                tracing::error!(%error, "could not requeue integration event");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "requeue_failed" })),
                )
            }
        })
}

fn require_admin(user: &db::users::User) -> Result<(), StatusCode> {
    (user.role == "administrator")
        .then_some(())
        .ok_or(StatusCode::FORBIDDEN)
}

async fn require_integration_module(state: &AppState) -> Result<(), StatusCode> {
    db::modules::is_enabled(&state.pool, db::modules::COMMERCE_VENDURE)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .then_some(())
        .ok_or(StatusCode::CONFLICT)
}
