use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{module_key}", put(set_enabled))
        .route("/{module_key}/health", get(check_health))
}

#[derive(Deserialize)]
struct ModuleUpdate {
    enabled: bool,
}

async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Vec<db::modules::EssentialsModule>>, StatusCode> {
    db::modules::visible_for_user(&state.pool, user.id, &user.role)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn set_enabled(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(module_key): Path<String>,
    Json(input): Json<ModuleUpdate>,
) -> Result<StatusCode, StatusCode> {
    if user.role != "administrator" {
        return Err(StatusCode::FORBIDDEN);
    }
    db::modules::set_enabled(&state.pool, &module_key, input.enabled)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .then_some(StatusCode::NO_CONTENT)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn check_health(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(module_key): Path<String>,
) -> Result<Json<db::modules::ConnectorHealth>, StatusCode> {
    if user.role != "administrator" {
        return Err(StatusCode::FORBIDDEN);
    }
    let secret_reference_variable = match module_key.as_str() {
        "connector_dhl" => "DHL_CONNECTOR_SECRET_REF",
        "connector_dpd" => "DPD_CONNECTOR_SECRET_REF",
        _ => return Err(StatusCode::NOT_FOUND),
    };
    let configured = std::env::var(secret_reference_variable)
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    let message = if configured {
        "Secret reference configured; live delivery health is intentionally not probed by this catalog check."
    } else {
        "No secret reference is configured."
    };
    db::modules::record_connector_health(
        &state.pool,
        &module_key,
        configured,
        if configured {
            "degraded"
        } else {
            "not_configured"
        },
        message,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map(Json)
    .ok_or(StatusCode::NOT_FOUND)
}
