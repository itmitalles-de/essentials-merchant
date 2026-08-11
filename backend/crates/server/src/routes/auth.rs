use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::{create_token, verify_password, AuthUser};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/me", get(me))
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
}

async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<TokenResponse>, (StatusCode, Json<Value>)> {
    let unauthorized = || {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Benutzername oder Passwort falsch" })),
        )
    };

    let user = db::users::find_by_username(&state.pool, &payload.username)
        .await
        .map_err(|_| unauthorized())?
        .ok_or_else(unauthorized)?;

    if !verify_password(&payload.password, &user.password_hash) {
        return Err(unauthorized());
    }

    let access_token = create_token(&user.username, &state.jwt_secret);
    Ok(Json(TokenResponse { access_token }))
}

#[derive(Serialize)]
struct MeResponse {
    username: String,
}

async fn me(AuthUser(user): AuthUser) -> Json<MeResponse> {
    Json(MeResponse {
        username: user.username,
    })
}
