use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

pub const MARKETPLACE_INTELLIGENCE: &str = "marketplace_intelligence";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EssentialsModule {
    pub module_key: String,
    pub module_group: String,
    pub display_name: String,
    pub module_kind: String,
    pub enabled: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ConnectorHealth {
    pub module_key: String,
    pub configuration_valid: bool,
    pub health_status: String,
    pub checked_at: Option<DateTime<Utc>>,
    pub message: Option<String>,
}

pub async fn list_catalog(pool: &PgPool) -> Result<Vec<EssentialsModule>, sqlx::Error> {
    sqlx::query_as::<_, EssentialsModule>(
        "SELECT module_key, module_group, display_name, module_kind, enabled, updated_at
         FROM essentials_modules ORDER BY module_group, module_key",
    )
    .fetch_all(pool)
    .await
}

pub async fn set_enabled(
    pool: &PgPool,
    module_key: &str,
    enabled: bool,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE essentials_modules SET enabled = $2, updated_at = now() WHERE module_key = $1",
    )
    .bind(module_key)
    .bind(enabled)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn is_enabled(pool: &PgPool, module_key: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT enabled FROM essentials_modules WHERE module_key = $1")
        .bind(module_key)
        .fetch_optional(pool)
        .await
        .map(|enabled| enabled.unwrap_or(false))
}

pub async fn user_can_access(
    pool: &PgPool,
    user_id: Uuid,
    user_role: &str,
    module_key: &str,
) -> Result<bool, sqlx::Error> {
    if user_role == "administrator" {
        return Ok(true);
    }
    sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM essentials_modules module
             JOIN user_module_permissions permission ON permission.module_key = module.module_key
             WHERE module.module_key = $1 AND module.enabled
               AND permission.user_id = $2 AND permission.granted
         )",
    )
    .bind(module_key)
    .bind(user_id)
    .fetch_one(pool)
    .await
}

pub async fn visible_for_user(
    pool: &PgPool,
    user_id: Uuid,
    user_role: &str,
) -> Result<Vec<EssentialsModule>, sqlx::Error> {
    if user_role == "administrator" {
        return list_catalog(pool).await;
    }
    sqlx::query_as::<_, EssentialsModule>(
        "SELECT module.module_key, module.module_group, module.display_name, module.module_kind,
                module.enabled, module.updated_at
         FROM essentials_modules module
         JOIN user_module_permissions permission ON permission.module_key = module.module_key
         WHERE module.enabled AND permission.user_id = $1 AND permission.granted
         ORDER BY module.module_group, module.module_key",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn connector_health(
    pool: &PgPool,
    module_key: &str,
) -> Result<Option<ConnectorHealth>, sqlx::Error> {
    sqlx::query_as::<_, ConnectorHealth>(
        "SELECT module_key, configuration_valid, health_status, checked_at, message
         FROM connector_module_health WHERE module_key = $1",
    )
    .bind(module_key)
    .fetch_optional(pool)
    .await
}

pub async fn record_connector_health(
    pool: &PgPool,
    module_key: &str,
    configuration_valid: bool,
    health_status: &str,
    message: &str,
) -> Result<Option<ConnectorHealth>, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE connector_module_health
         SET configuration_valid = $2, health_status = $3, checked_at = now(), message = $4
         WHERE module_key = $1",
    )
    .bind(module_key)
    .bind(configuration_valid)
    .bind(health_status)
    .bind(message)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    connector_health(pool, module_key).await
}
