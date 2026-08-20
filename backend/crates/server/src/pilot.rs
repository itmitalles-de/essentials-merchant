use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use sqlx::PgPool;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/status", get(status))
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
            | "/api/marketplace/connections"
            | "/api/marketplace/demo"
            | "/api/marketplace/imports"
            | "/api/marketplace/imports/preview"
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
            "/api/marketplace/connections",
            "/api/marketplace/demo",
            "/api/marketplace/connections/id/runs",
            "/api/marketplace/connections/id/analyses",
            "/api/marketplace/imports/preview",
            "/api/marketplace/imports",
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
}
