use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::{create_pilot_token, create_token, verify_password, AuthUser};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/pilot-session", post(pilot_session))
        .route("/me", get(me))
}

async fn pilot_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<TokenResponse>, (StatusCode, Json<Value>)> {
    let denied = || {
        (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "pilot_session_unavailable" })),
        )
    };
    if !state.mantle_pilot_no_login
        || headers
            .get("x-mantle-pilot-proxy")
            .and_then(|value| value.to_str().ok())
            != Some("v1")
        || !same_origin_request(&headers)
    {
        return Err(denied());
    }
    let user = db::users::find_by_username(&state.pool, &state.pilot_admin_username)
        .await
        .map_err(|_| denied())?
        .filter(|user| user.role == "administrator")
        .ok_or_else(denied)?;
    let access_token = create_pilot_token(&user.username, &state.jwt_secret);
    Ok(Json(TokenResponse { access_token }))
}

fn same_origin_request(headers: &HeaderMap) -> bool {
    if headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == "cross-site")
    {
        return false;
    }
    let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok()) else {
        return true;
    };
    let Some(host) = headers.get("host").and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let Ok(origin) = reqwest::Url::parse(origin) else {
        return false;
    };
    let request_host = host.split(':').next().unwrap_or_default();
    matches!(origin.scheme(), "http" | "https")
        && origin.username().is_empty()
        && origin.password().is_none()
        && origin
            .host_str()
            .is_some_and(|origin_host| origin_host.eq_ignore_ascii_case(request_host))
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
    if state.mantle_pilot_no_login {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "interactive_login_disabled" })),
        ));
    }
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
    role: String,
}

async fn me(AuthUser(user): AuthUser) -> Json<MeResponse> {
    Json(MeResponse {
        username: user.username,
        role: user.role,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn pilot_session_rejects_cross_site_origin() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "host",
            HeaderValue::from_static("ai-marketing.mantle-climbing.de"),
        );
        headers.insert(
            "origin",
            HeaderValue::from_static("https://attacker.example"),
        );
        headers.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(!same_origin_request(&headers));
    }

    #[test]
    fn pilot_session_accepts_same_host_and_non_browser_operator() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "host",
            HeaderValue::from_static("ai-marketing.mantle-climbing.de"),
        );
        headers.insert(
            "origin",
            HeaderValue::from_static("https://ai-marketing.mantle-climbing.de"),
        );
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        assert!(same_origin_request(&headers));

        headers.remove("origin");
        headers.remove("sec-fetch-site");
        assert!(same_origin_request(&headers));
    }
}
