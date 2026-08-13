use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt_secret: String,
    pub integration_secret: String,
    pub pdf_storage_dir: String,
}
