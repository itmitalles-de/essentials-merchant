use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};

use crate::auth::AuthUser;
use crate::state::AppState;
use db::company_settings::{CompanySettings, CompanySettingsUpdate};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_settings).put(update_settings))
}

async fn get_settings(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
) -> Result<Json<CompanySettings>, StatusCode> {
    db::company_settings::get(&state.pool)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn update_settings(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Json(payload): Json<CompanySettingsUpdate>,
) -> Result<Json<CompanySettings>, StatusCode> {
    db::company_settings::update(&state.pool, &payload)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
