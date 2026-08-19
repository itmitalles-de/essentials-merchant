use sqlx::PgPool;

use crate::auth::hash_password;
use crate::config::Config;

pub async fn seed_admin(pool: &PgPool, config: &Config) -> anyhow::Result<db::users::User> {
    let user = if let Some(user) = db::users::find_by_username(pool, &config.admin_username).await?
    {
        user
    } else {
        let password_hash = hash_password(&config.admin_password);
        let user = db::users::create(pool, &config.admin_username, &password_hash).await?;
        tracing::info!(username = %config.admin_username, "seeded admin user");
        user
    };
    Ok(user)
}
