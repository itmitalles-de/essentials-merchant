mod auth;
mod bootstrap;
mod config;
mod routes;
mod state;

use std::net::SocketAddr;

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = Config::from_env();

    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;
    bootstrap::seed_admin(&pool, &config).await?;

    let state = AppState {
        pool,
        jwt_secret: config.jwt_secret,
    };

    let api = Router::new()
        .route("/health", get(health))
        .nest("/auth", routes::auth::router())
        .nest("/company-settings", routes::company_settings::router())
        .nest("/customers", routes::customers::router())
        .nest("/invoices", routes::invoices::router())
        .nest("/vat-rates", routes::vat_rates::router());

    let app = Router::new()
        .nest("/api", api)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::info!("erplite server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({ "status": domain::health() }))
}
