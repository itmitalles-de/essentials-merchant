use sqlx::PgPool;

use crate::marketplace::MarketplaceWorker;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt_secret: String,
    pub integration_secret: String,
    pub pdf_storage_dir: String,
    pub marketplace_worker: MarketplaceWorker,
}
