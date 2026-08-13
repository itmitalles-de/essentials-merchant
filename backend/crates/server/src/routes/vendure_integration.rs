use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, OriginalUri, Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orders", post(import_order))
        .route("/module-status", post(module_status))
        .route("/outbox/claim", post(claim_outbox))
        .route("/outbox/{id}/ack", post(acknowledge_outbox))
        .route("/outbox/{id}/retry", post(retry_outbox))
        .route("/mappings", post(record_mapping))
        .route("/diagnostics", post(record_diagnostics))
        .route("/commands/claim", post(claim_commands))
        .route("/commands/{id}/complete", post(complete_command))
        .layer(DefaultBodyLimit::max(256 * 1024))
}

async fn import_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Result<(StatusCode, Json<db::commerce::ImportResult>), (StatusCode, Json<Value>)> {
    let event = authenticate_and_parse(&state, &headers, &method, uri.path(), &body).await?;
    require_commerce_enabled(&state).await?;
    trigger_test_failpoint("before_inbox_commit");
    match db::commerce::import_vendure_order(&state.pool, &event).await {
        Ok(result) => {
            trigger_test_failpoint("after_inbox_commit");
            Ok((
                if result.duplicate {
                    StatusCode::OK
                } else {
                    StatusCode::CREATED
                },
                Json(result),
            ))
        }
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
    method: Method,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Result<Json<Vec<db::commerce::OutboxEvent>>, (StatusCode, Json<Value>)> {
    let input: ClaimInput =
        authenticate_and_parse(&state, &headers, &method, uri.path(), &body).await?;
    if !commerce_enabled(&state).await? {
        return Ok(Json(Vec::new()));
    }
    db::commerce::claim_outbox_with_policy(&state.pool, input.limit, state.outbox_policy)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(%error, "could not claim integration outbox");
            internal_error()
        })
}

async fn acknowledge_outbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    authenticate(&state, &headers, &method, uri.path(), &body).await?;
    match db::commerce::acknowledge_outbox(&state.pool, id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "event_not_processing" })),
        )),
        Err(error) => {
            tracing::error!(%error, %id, "could not acknowledge integration outbox event");
            Err(internal_error())
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
    method: Method,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let input: RetryInput =
        authenticate_and_parse(&state, &headers, &method, uri.path(), &body).await?;
    match db::commerce::retry_outbox_with_policy(&state.pool, id, &input.error, state.outbox_policy)
        .await
    {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "event_not_processing" })),
        )),
        Err(error) => {
            tracing::error!(%error, %id, "could not retry integration outbox event");
            Err(internal_error())
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
    method: Method,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let input: MappingInput =
        authenticate_and_parse(&state, &headers, &method, uri.path(), &body).await?;
    require_commerce_enabled(&state).await?;
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
        internal_error()
    })
}

async fn record_diagnostics(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let report: db::commerce::RemoteDiagnosticsReport =
        authenticate_and_parse(&state, &headers, &method, uri.path(), &body).await?;
    db::commerce::record_remote_diagnostics(&state.pool, "vendure", &report)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|error| {
            tracing::error!(%error, "could not record redacted Vendure diagnostics");
            internal_error()
        })
}

async fn claim_commands(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Result<Json<Vec<db::commerce::IntegrationAdminCommand>>, (StatusCode, Json<Value>)> {
    let input: ClaimInput =
        authenticate_and_parse(&state, &headers, &method, uri.path(), &body).await?;
    if !commerce_enabled(&state).await? {
        return Ok(Json(Vec::new()));
    }
    db::commerce::claim_admin_commands(
        &state.pool,
        "vendure",
        input.limit,
        state.outbox_policy.lease_seconds,
    )
    .await
    .map(Json)
    .map_err(|error| {
        tracing::error!(%error, "could not claim integration admin commands");
        internal_error()
    })
}

async fn module_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    authenticate(&state, &headers, &method, uri.path(), &body).await?;
    let enabled = commerce_enabled(&state).await?;
    let payment_test_enabled = module_enabled(&state, "payment.test").await?;
    let shipping_manual_enabled = module_enabled(&state, "shipping.manual").await?;
    Ok(Json(json!({
        "module_id": "commerce.vendure",
        "enabled": enabled,
        "payment_test_enabled": payment_test_enabled,
        "shipping_manual_enabled": shipping_manual_enabled,
    })))
}

async fn module_enabled(
    state: &AppState,
    module_id: &str,
) -> Result<bool, (StatusCode, Json<Value>)> {
    db::modules::is_enabled(&state.pool, module_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, %module_id, "could not resolve module state");
            internal_error()
        })
}

async fn commerce_enabled(state: &AppState) -> Result<bool, (StatusCode, Json<Value>)> {
    module_enabled(state, db::modules::COMMERCE_VENDURE).await
}

async fn require_commerce_enabled(state: &AppState) -> Result<(), (StatusCode, Json<Value>)> {
    if commerce_enabled(state).await? {
        Ok(())
    } else {
        Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "module_disabled", "module_id": "commerce.vendure" })),
        ))
    }
}

#[derive(Deserialize)]
struct CompleteCommandInput {
    error: Option<String>,
}

async fn complete_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let input: CompleteCommandInput =
        authenticate_and_parse(&state, &headers, &method, uri.path(), &body).await?;
    match db::commerce::complete_admin_command(&state.pool, id, input.error.as_deref()).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "command_not_processing" })),
        )),
        Err(error) => {
            tracing::error!(%error, %id, "could not complete integration admin command");
            Err(internal_error())
        }
    }
}

async fn authenticate_and_parse<T: DeserializeOwned>(
    state: &AppState,
    headers: &HeaderMap,
    method: &Method,
    path: &str,
    body: &[u8],
) -> Result<T, (StatusCode, Json<Value>)> {
    authenticate(state, headers, method, path, body).await?;
    serde_json::from_slice(body).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_json" })),
        )
    })
}

async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
    method: &Method,
    path: &str,
    body: &[u8],
) -> Result<(), (StatusCode, Json<Value>)> {
    state
        .integration_auth
        .verify(&state.pool, headers, method, path, body)
        .await
        .map_err(|error| {
            if matches!(
                error,
                crate::integration_auth::IntegrationAuthError::Database(_)
            ) {
                tracing::error!("integration request authentication persistence failed");
            }
            (
                error.status(),
                Json(json!({ "error": "integration_authentication_failed" })),
            )
        })
}

fn internal_error() -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "integration_operation_failed" })),
    )
}

fn trigger_test_failpoint(name: &str) {
    if std::env::var("APP_ENV").ok().as_deref() != Some("test") {
        return;
    }
    let enabled = std::env::var("INTEGRATION_TEST_FAILPOINTS").unwrap_or_default();
    if !enabled.split(',').map(str::trim).any(|value| value == name) {
        return;
    }
    let marker = format!("/tmp/essentials-integration-failpoint-{name}");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker)
    {
        Ok(_) => {
            tracing::warn!(%name, "triggering one-shot integration test failpoint");
            std::process::exit(70);
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => tracing::error!(%error, %name, "could not create test failpoint marker"),
    }
}
