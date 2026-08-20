use axum::extract::DefaultBodyLimit;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use sqlx::PgPool;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/provider-secrets/status", get(provider_secrets_status))
        .route(
            "/provider-secrets/openai",
            post(configure_openai_secret).layer(DefaultBodyLimit::max(4 * 1024)),
        )
        .route(
            "/provider-secrets/amazon",
            post(configure_amazon_secret).layer(DefaultBodyLimit::max(12 * 1024)),
        )
}

async fn status(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<db::modules::AmazonPilotStatus>, StatusCode> {
    if user.role != "administrator" {
        return Err(StatusCode::FORBIDDEN);
    }
    db::modules::amazon_pilot_status(&state.pool)
        .await
        .map(Json)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigureOpenAiRequest {
    api_key: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigureAmazonRequest {
    lwa_client_id: String,
    lwa_client_secret: String,
    lwa_refresh_token: String,
    seller_id: String,
    marketplace_id: String,
    region: String,
    confirm_authorized: bool,
    confirm_read_only: bool,
}

async fn provider_secrets_status(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<crate::provider_secrets::ProviderSecretsStatus>, StatusCode> {
    require_pilot_admin(&state, &user).await?;
    state
        .provider_secrets
        .status()
        .await
        .map(Json)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

async fn configure_openai_secret(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<ConfigureOpenAiRequest>,
) -> Result<Json<crate::provider_secrets::ProviderStatus>, (StatusCode, Json<serde_json::Value>)> {
    require_pilot_admin(&state, &user)
        .await
        .map_err(provider_error_status)?;
    state
        .provider_secrets
        .configure_openai(input.api_key, user.id)
        .await
        .map(Json)
        .map_err(provider_secret_error)
}

async fn configure_amazon_secret(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<ConfigureAmazonRequest>,
) -> Result<Json<crate::provider_secrets::ProviderStatus>, (StatusCode, Json<serde_json::Value>)> {
    require_pilot_admin(&state, &user)
        .await
        .map_err(provider_error_status)?;
    state
        .provider_secrets
        .configure_amazon(
            crate::provider_secrets::ConfigureAmazonInput {
                refresh_token: input.lwa_refresh_token,
                client_id: input.lwa_client_id,
                client_secret: input.lwa_client_secret,
                seller_id: input.seller_id,
                marketplace_id: input.marketplace_id,
                region: input.region,
                confirm_authorized: input.confirm_authorized,
                confirm_read_only: input.confirm_read_only,
            },
            user.id,
        )
        .await
        .map(Json)
        .map_err(provider_secret_error)
}

async fn require_pilot_admin(state: &AppState, user: &db::users::User) -> Result<(), StatusCode> {
    if user.role != "administrator"
        || !db::modules::is_enabled(&state.pool, db::modules::AMAZON_READ_ONLY_PILOT)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

fn provider_error_status(status: StatusCode) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(json!({ "error": "pilot_secret_access_denied" })),
    )
}

fn provider_secret_error(
    error: crate::provider_secrets::ProviderSecretError,
) -> (StatusCode, Json<serde_json::Value>) {
    let (status, code) = match error {
        crate::provider_secrets::ProviderSecretError::InvalidInput => {
            (StatusCode::BAD_REQUEST, "provider_secret_invalid")
        }
        crate::provider_secrets::ProviderSecretError::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "provider_secret_store_unavailable",
        ),
        crate::provider_secrets::ProviderSecretError::Crypto
        | crate::provider_secrets::ProviderSecretError::Database(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "provider_secret_store_failed",
        ),
    };
    (status, Json(json!({ "error": code })))
}

pub async fn enforce_read_only(
    State(pool): State<PgPool>,
    request: Request,
    next: Next,
) -> Response {
    let pilot_enabled = db::modules::is_enabled(&pool, db::modules::AMAZON_READ_ONLY_PILOT)
        .await
        .unwrap_or(true);
    if pilot_enabled && !pilot_request_allowed(request.method(), request.uri().path()) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "pilot_read_only",
                "profile": "amazon-read-only",
            })),
        )
            .into_response();
    }
    next.run(request).await
}

fn pilot_request_allowed(method: &Method, path: &str) -> bool {
    let segments = path.trim_matches('/').split('/').collect::<Vec<_>>();
    if method == Method::GET
        && matches!(
            segments.as_slice(),
            ["api", "marketplace", "runs", _, "raw"] | ["api", "modules", _, "health"]
        )
    {
        return false;
    }
    if matches!(method, &Method::GET | &Method::HEAD | &Method::OPTIONS) {
        return true;
    }
    if method != Method::POST {
        return false;
    }
    if matches!(
        path,
        "/api/auth/login"
            | "/api/auth/pilot-session"
            | "/api/marketplace/connections"
            | "/api/marketplace/demo"
            | "/api/marketplace/imports"
            | "/api/marketplace/imports/preview"
            | "/api/pilot/provider-secrets/openai"
            | "/api/pilot/provider-secrets/amazon"
    ) {
        return true;
    }
    matches!(
        segments.as_slice(),
        ["api", "marketplace", "connections", _, "runs"]
            | ["api", "marketplace", "connections", _, "analyses"]
            | ["api", "marketplace", "strategy", "weekly"]
    )
}

pub(crate) fn anonymous_pilot_request_allowed(method: &Method, path: &str) -> bool {
    let path = path.trim_end_matches('/');
    let segments = path.trim_matches('/').split('/').collect::<Vec<_>>();
    if method == Method::GET {
        return matches!(
            path,
            "/api/auth/me"
                | "/api/modules"
                | "/api/pilot/status"
                | "/api/pilot/provider-secrets/status"
                | "/api/marketplace"
                | "/api/marketplace/strategy/status"
                | "/api/marketplace/strategy/weekly"
        ) || matches!(
            segments.as_slice(),
            ["api", "marketplace", "runs", _] | ["api", "marketplace", "analyses", _, "export"]
        );
    }
    if method != Method::POST {
        return false;
    }
    if matches!(
        path,
        "/api/marketplace/connections"
            | "/api/marketplace/demo"
            | "/api/marketplace/imports"
            | "/api/marketplace/imports/preview"
            | "/api/marketplace/strategy/weekly"
            | "/api/pilot/provider-secrets/openai"
            | "/api/pilot/provider-secrets/amazon"
    ) {
        return true;
    }
    matches!(
        segments.as_slice(),
        ["api", "marketplace", "connections", _, "runs"]
            | ["api", "marketplace", "connections", _, "analyses"]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::any;
    use tower::ServiceExt;

    #[sqlx::test(migrations = "../db/migrations")]
    async fn pilot_rejects_business_mutations_but_allows_manual_report_acquisition(pool: PgPool) {
        sqlx::query(
            "UPDATE essentials_modules SET enabled = true, state = 'enabled'
             WHERE module_id = 'pilot.amazon_read_only'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let app = Router::new()
            .fallback(any(|| async { StatusCode::NO_CONTENT }))
            .layer(axum::middleware::from_fn_with_state(
                pool,
                enforce_read_only,
            ));

        for (method, path) in [
            (Method::POST, "/api/articles/"),
            (Method::POST, "/api/sales-orders/"),
            (Method::POST, "/api/sales-orders/id/fulfill"),
            (Method::POST, "/api/exports/datev"),
            (Method::PUT, "/api/modules/payment.test"),
            (Method::PATCH, "/api/articles/id"),
            (Method::DELETE, "/api/customers/id"),
            (Method::POST, "/api/integrations/vendure/orders"),
            (Method::PUT, "/api/marketplace/connections/id/schedules"),
            (Method::GET, "/api/marketplace/runs/id/raw"),
            (Method::GET, "/api/modules/shipping.dhl/health"),
            (Method::POST, "/api/marketplace/analyses/id/strategy/run"),
            (Method::POST, "/api/marketplace/analyses/id/strategy"),
            (Method::POST, "/api/marketplace/strategy/weekly/run"),
            (Method::POST, "/api/marketplace/analyses/id/export"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CONFLICT, "{path}");
        }

        for path in [
            "/api/auth/login",
            "/api/auth/pilot-session",
            "/api/marketplace/connections",
            "/api/marketplace/demo",
            "/api/marketplace/connections/id/runs",
            "/api/marketplace/connections/id/analyses",
            "/api/marketplace/imports/preview",
            "/api/marketplace/imports",
            "/api/marketplace/strategy/weekly",
            "/api/pilot/provider-secrets/openai",
            "/api/pilot/provider-secrets/amazon",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NO_CONTENT, "{path}");
        }
    }

    #[test]
    fn anonymous_session_is_limited_to_exact_pilot_routes() {
        for (method, path) in [
            (Method::GET, "/api/auth/me"),
            (Method::GET, "/api/modules"),
            (Method::GET, "/api/pilot/status"),
            (Method::GET, "/api/pilot/provider-secrets/status"),
            (Method::GET, "/api/marketplace"),
            (
                Method::GET,
                "/api/marketplace/runs/00000000-0000-0000-0000-000000000000",
            ),
            (
                Method::GET,
                "/api/marketplace/analyses/00000000-0000-0000-0000-000000000000/export",
            ),
            (Method::POST, "/api/marketplace/imports/preview"),
            (Method::POST, "/api/marketplace/imports"),
            (Method::POST, "/api/marketplace/strategy/weekly"),
            (Method::POST, "/api/pilot/provider-secrets/openai"),
            (Method::POST, "/api/pilot/provider-secrets/amazon"),
        ] {
            assert!(anonymous_pilot_request_allowed(&method, path), "{path}");
        }

        for (method, path) in [
            (Method::GET, "/api/customers"),
            (Method::GET, "/api/invoices"),
            (Method::GET, "/api/company-settings"),
            (Method::GET, "/api/admin-center"),
            (Method::GET, "/api/marketplace/runs/id/raw"),
            (Method::GET, "/api/modules/payment.test/health"),
            (Method::POST, "/api/articles"),
            (Method::PUT, "/api/marketplace/connections/id/schedules"),
            (Method::DELETE, "/api/pilot/provider-secrets/openai"),
        ] {
            assert!(!anonymous_pilot_request_allowed(&method, path), "{path}");
        }
    }
}
