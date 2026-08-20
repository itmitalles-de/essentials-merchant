mod auth;
mod bootstrap;
mod config;
mod datev;
mod integration_auth;
mod manual_import;
mod marketplace;
mod pdf_gen;
mod pilot;
mod provider_secrets;
mod routes;
mod state;
mod strategy_ai;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::{middleware, routing::get, Json, Router};
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--healthcheck") {
        let address = "127.0.0.1:8000".parse()?;
        std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(2))?;
        return Ok(());
    }
    tracing_subscriber::fmt::init();

    let config = Config::from_env();

    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;
    let admin = bootstrap::seed_admin(&pool, &config).await?;
    if config.module_profile == Some(config::ModuleProfile::AmazonReadOnly) {
        let status = db::modules::apply_amazon_read_only_profile(&pool, admin.id).await?;
        if !status.compliant {
            anyhow::bail!("Amazon read-only module profile is not compliant");
        }
        tracing::info!(profile = status.profile, "applied persisted module profile");
    }

    std::fs::create_dir_all(&config.pdf_storage_dir)?;
    let provider_secrets = provider_secrets::ProviderSecretStore::from_env(
        pool.clone(),
        config.mantle_pilot_no_login,
    )?;
    let marketplace_worker = marketplace::MarketplaceWorker::new(Arc::new(
        marketplace::CompositeAmazonClient::new(provider_secrets.clone())?,
    ));
    let strategy_ai = strategy_ai::StrategyAiClient::from_env()?;

    let state = AppState {
        pool,
        jwt_secret: config.jwt_secret,
        integration_auth: config.integration_auth,
        outbox_policy: config.outbox_policy,
        pdf_storage_dir: config.pdf_storage_dir,
        marketplace_worker: marketplace_worker.clone(),
        strategy_ai,
        provider_secrets,
        mantle_pilot_no_login: config.mantle_pilot_no_login,
        pilot_admin_username: admin.username,
    };

    let api = Router::new()
        .route("/health", get(health))
        .route("/readiness", get(readiness))
        .nest("/articles", routes::articles::router())
        .nest("/auth", routes::auth::router())
        .nest("/company-settings", routes::company_settings::router())
        .nest("/customers", routes::customers::router())
        .nest("/exports", routes::exports::router())
        .nest("/invoices", routes::invoices::router())
        .nest(
            "/integration-diagnostics",
            routes::integration_diagnostics::router(),
        )
        .nest("/marketplace", routes::marketplace::router())
        .nest("/modules", routes::modules::router())
        .nest("/pilot", pilot::router())
        .nest("/sales-orders", routes::sales_orders::router())
        .nest(
            "/integrations/vendure",
            routes::vendure_integration::router(),
        )
        .nest("/vat-rates", routes::vat_rates::router());

    let worker_pool = state.pool.clone();
    let marketplace_worker_interval_seconds = config.marketplace_worker_interval_seconds;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            marketplace_worker_interval_seconds,
        ));
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
        .layer(middleware::from_fn_with_state(
            state.pool.clone(),
            pilot::enforce_read_only,
        ))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::info!("Essentials+ Merchant server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({ "status": domain::health() }))
}

async fn readiness(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .map(|_| Json(json!({ "status": "ready" })))
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}
