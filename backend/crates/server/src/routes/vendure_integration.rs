use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::state::AppState;

const INTEGRATION_KEY_HEADER: &str = "x-shop-suite-integration-key";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orders", post(import_order))
        .route("/outbox/claim", post(claim_outbox))
        .route("/outbox/{id}/ack", post(acknowledge_outbox))
        .route("/outbox/{id}/retry", post(retry_outbox))
        .route("/mappings", post(record_mapping))
}

fn authorize(headers: &HeaderMap, state: &AppState) -> Result<(), StatusCode> {
    let supplied = headers
        .get(INTEGRATION_KEY_HEADER)
        .and_then(|value| value.to_str().ok());
    if supplied == Some(state.integration_secret.as_str()) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn import_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(event): Json<db::commerce::VendureOrderEvent>,
) -> Result<(StatusCode, Json<db::commerce::ImportResult>), (StatusCode, Json<Value>)> {
    authorize(&headers, &state)
        .map_err(|status| (status, Json(json!({ "error": "unauthorized" }))))?;
    match db::commerce::import_vendure_order(&state.pool, &event).await {
        Ok(result) => Ok((
            if result.duplicate {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            },
            Json(result),
        )),
        Err(db::commerce::ImportError::UnknownSku(sku)) => Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "unknown_sku", "sku": sku })),
        )),
        Err(db::commerce::ImportError::EmptyOrder(_)) => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "empty_order" })),
        )),
        Err(error) => {
            tracing::error!(%error, "Vendure order import failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "order_import_failed" })),
            ))
        }
    }
}

#[derive(Deserialize)]
struct ClaimInput {
    #[serde(default = "default_claim_limit")]
    limit: i64,
}

fn default_claim_limit() -> i64 {
    10
}

async fn claim_outbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ClaimInput>,
) -> Result<Json<Vec<db::commerce::OutboxEvent>>, StatusCode> {
    authorize(&headers, &state)?;
    db::commerce::claim_outbox(&state.pool, input.limit)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(%error, "could not claim integration outbox");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn acknowledge_outbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    match db::commerce::acknowledge_outbox(&state.pool, id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::CONFLICT),
        Err(error) => {
            tracing::error!(%error, %id, "could not acknowledge integration outbox event");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
struct RetryInput {
    error: String,
}

async fn retry_outbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<RetryInput>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    match db::commerce::retry_outbox(&state.pool, id, &input.error).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::CONFLICT),
        Err(error) => {
            tracing::error!(%error, %id, "could not retry integration outbox event");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
struct MappingInput {
    entity_type: String,
    internal_id: Uuid,
    external_id: String,
    #[serde(default)]
    metadata: Value,
}

async fn record_mapping(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<MappingInput>,
) -> Result<StatusCode, StatusCode> {
    authorize(&headers, &state)?;
    db::commerce::record_mapping(
        &state.pool,
        &input.entity_type,
        input.internal_id,
        &input.external_id,
        input.metadata,
    )
    .await
    .map(|_| StatusCode::NO_CONTENT)
    .map_err(|error| {
        tracing::error!(%error, "could not record Vendure mapping");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}
