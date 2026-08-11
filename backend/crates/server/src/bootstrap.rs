use sqlx::PgPool;

use crate::auth::hash_password;
use crate::config::Config;

pub async fn seed_admin(pool: &PgPool, config: &Config) -> anyhow::Result<()> {
    if db::users::find_by_username(pool, &config.admin_username)
        .await?
        .is_none()
    {
        let password_hash = hash_password(&config.admin_password);
        db::users::create(pool, &config.admin_username, &password_hash).await?;
        tracing::info!(username = %config.admin_username, "seeded admin user");
    }
    Ok(())
}
