use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, Response, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::auth::DatevExportUser;
use crate::datev::{self, DatevExportRequest};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/datev", post(export_datev))
}

async fn export_datev(
    State(state): State<AppState>,
    DatevExportUser(user): DatevExportUser,
    headers: HeaderMap,
    Json(input): Json<DatevExportRequest>,
) -> Result<Response<Body>, (StatusCode, Json<serde_json::Value>)> {
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty() && value.len() <= 200)
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "invalid_idempotency_key"))?;
    let entries =
        db::accounting::entries_for_period(&state.pool, input.period_start, input.period_end)
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "accounting_entries_unavailable",
                )
            })?;
    let payload = datev::render_booking_batch(&input, &entries)
        .map_err(|failure| error(StatusCode::UNPROCESSABLE_ENTITY, &failure.to_string()))?;
    let parameters = serde_json::to_vec(&input)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid_export_parameters"))?;
    let parameters_sha256 = hex::encode(Sha256::digest(parameters));
    let entry_ids = entries.iter().map(|entry| entry.id).collect::<Vec<_>>();
    let stored = db::accounting::store_export(
        &state.pool,
        &db::accounting::ExportBatch {
            actor_user_id: user.id,
            idempotency_key,
            period_start: input.period_start,
            period_end: input.period_end,
            parameters_sha256: &parameters_sha256,
            payload: &payload,
            entry_ids: &entry_ids,
        },
    )
    .await
    .map_err(|failure| match failure {
        db::accounting::ExportStoreError::IdempotencyConflict => {
            error(StatusCode::CONFLICT, "idempotency_conflict")
        }
        db::accounting::ExportStoreError::Sqlx(_) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "export_store_unavailable",
        ),
    })?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=EXTF_Buchungsstapel.csv",
        )
        .header("x-content-sha256", stored.payload_sha256)
        .header("x-idempotent-replay", stored.duplicate.to_string())
        .body(Body::from(stored.payload))
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "response_build_failed"))
}

fn error(status: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(json!({ "error": message })))
}
