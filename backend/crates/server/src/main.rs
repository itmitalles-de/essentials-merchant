mod auth;
mod bootstrap;
mod config;
mod marketplace;
mod pdf_gen;
mod routes;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;

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

    std::fs::create_dir_all(&config.pdf_storage_dir)?;
    let insight_provider = marketplace::OpenAiCompatibleProvider::from_environment()?
        .map(|provider| Arc::new(provider) as Arc<dyn marketplace::InsightProvider>);
    let marketplace_worker = marketplace::MarketplaceWorker::new(
        Arc::new(marketplace::CompositeAmazonClient::new()?),
        insight_provider,
    );

    let state = AppState {
        pool,
        jwt_secret: config.jwt_secret,
        integration_secret: config.integration_secret,
        pdf_storage_dir: config.pdf_storage_dir,
        marketplace_worker: marketplace_worker.clone(),
    };

    let api = Router::new()
        .route("/health", get(health))
        .nest("/articles", routes::articles::router())
        .nest("/auth", routes::auth::router())
        .nest("/company-settings", routes::company_settings::router())
        .nest("/customers", routes::customers::router())
        .nest("/invoices", routes::invoices::router())
        .nest("/marketplace", routes::marketplace::router())
        .nest("/modules", routes::modules::router())
        .nest("/sales-orders", routes::sales_orders::router())
        .nest(
            "/integrations/vendure",
            routes::vendure_integration::router(),
        )
        .nest("/vat-rates", routes::vat_rates::router());

    let worker_pool = state.pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(error) = marketplace_worker.cycle(&worker_pool).await {
                tracing::error!(%error, "Marketplace Intelligence worker cycle failed");
            }
        }
    });

    let app = Router::new()
        .nest("/api", api)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::info!("Merchant server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({ "status": domain::health() }))
}
