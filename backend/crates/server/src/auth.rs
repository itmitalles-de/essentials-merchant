use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::{FromRef, FromRequestParts};
use axum::http::{header, request::Parts, StatusCode};
use axum::Json;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::AppState;
use db::users::User;

const TOKEN_EXPIRY_DAYS: i64 = 7;

pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hashing should not fail")
        .to_string()
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: i64,
}

pub fn create_token(username: &str, secret: &str) -> String {
    let exp = (Utc::now() + Duration::days(TOKEN_EXPIRY_DAYS)).timestamp();
    let claims = Claims {
        sub: username.to_string(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("jwt encoding should not fail")
}

fn decode_token(token: &str, secret: &str) -> Option<String> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|data| data.claims.sub)
}

/// Extractor that verifies the bearer token and loads the current user, so
/// handlers only need `AuthUser` in their signature to require authentication.
pub struct AuthUser(pub User);

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let unauthorized = || {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Nicht angemeldet" })),
            )
        };

        let header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(unauthorized)?;
        let token = header.strip_prefix("Bearer ").ok_or_else(unauthorized)?;

        let app_state = AppState::from_ref(state);
        let username = decode_token(token, &app_state.jwt_secret).ok_or_else(unauthorized)?;

        let user = db::users::find_by_username(&app_state.pool, &username)
            .await
            .map_err(|_| unauthorized())?
            .ok_or_else(unauthorized)?;

        Ok(AuthUser(user))
    }
}
