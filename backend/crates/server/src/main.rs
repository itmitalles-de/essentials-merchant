use std::net::SocketAddr;

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use sqlx::PgPool;

#[derive(Clone)]
struct AppState {
    #[allow(dead_code)]
    pool: PgPool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = db::connect(&database_url).await?;
    db::migrate(&pool).await?;

    let state = AppState { pool };

    let api = Router::new().route("/health", get(health));

    let app = Router::new().nest("/api", api).with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::info!("erplite server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({ "status": domain::health() }))
}
