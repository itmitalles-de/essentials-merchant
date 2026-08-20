use sqlx::PgPool;

use crate::marketplace::MarketplaceWorker;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt_secret: String,
    pub integration_auth: crate::integration_auth::IntegrationAuth,
    pub outbox_policy: db::commerce::OutboxPolicy,
    pub pdf_storage_dir: String,
    pub marketplace_worker: MarketplaceWorker,
    pub strategy_ai: crate::strategy_ai::StrategyAiClient,
}
