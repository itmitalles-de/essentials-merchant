use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
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
    headers: HeaderMap,
    Json(input): Json<ModuleUpdate>,
) -> Result<Json<db::modules::ModuleTransition>, StatusCode> {
    if user.role != "administrator" {
        return Err(StatusCode::FORBIDDEN);
    }
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty() && value.len() <= 200)
        .ok_or(StatusCode::BAD_REQUEST)?;
    db::modules::transition_state(
        &state.pool,
        user.id,
        &module_key,
        if input.enabled { "enabled" } else { "disabled" },
        idempotency_key,
    )
    .await
    .map(Json)
    .map_err(|error| match error {
        db::modules::ModuleChangeError::NotFound => StatusCode::NOT_FOUND,
        db::modules::ModuleChangeError::Required
        | db::modules::ModuleChangeError::NotInstalled
        | db::modules::ModuleChangeError::NeedsConfiguration
        | db::modules::ModuleChangeError::MissingDependency(_)
        | db::modules::ModuleChangeError::Conflict(_) => StatusCode::CONFLICT,
        db::modules::ModuleChangeError::Sqlx(_) => StatusCode::INTERNAL_SERVER_ERROR,
    })
}

async fn check_health(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(module_key): Path<String>,
) -> Result<Json<db::modules::ConnectorHealth>, StatusCode> {
    if user.role != "administrator" {
        return Err(StatusCode::FORBIDDEN);
    }
    let module = db::modules::module_by_identifier(&state.pool, &module_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if module.module_kind != "connector" {
        return Err(StatusCode::NOT_FOUND);
    }
    if matches!(
        module.module_id.as_str(),
        "payment.test" | "shipping.manual"
    ) {
        return db::modules::connector_health(&state.pool, &module.module_key)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .map(Json)
            .ok_or(StatusCode::NOT_FOUND);
    }
    let secret_reference_variable = match module.module_id.as_str() {
        "shipping.dhl" => "DHL_CONNECTOR_SECRET_REF",
        "shipping.dpd" => "DPD_CONNECTOR_SECRET_REF",
        _ => return Err(StatusCode::NOT_IMPLEMENTED),
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
        &module.module_key,
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
