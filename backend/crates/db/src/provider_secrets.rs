//! Opaque persistence for provider credentials entered through the Mantle
//! pilot UI. Encryption and decryption stay in the server crate; this module
//! never receives plaintext secret fields.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub const OPENAI_PROVIDER: &str = "openai";
pub const AMAZON_PROVIDER: &str = "amazon";
pub const PILOT_AMAZON_SECRET_REF: &str = "pilot_seller";
pub const ENCRYPTION_ALGORITHM: &str = "AES-256-GCM-v1";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EncryptedProviderSecret {
    pub provider: String,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub configured_fields: Vec<String>,
    pub context_sha256: Option<String>,
    pub read_only_approved_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProviderSecretStatus {
    pub provider: String,
    pub configured_fields: Vec<String>,
    pub read_only_approved: bool,
    pub updated_at: DateTime<Utc>,
}

pub struct StoreEncryptedSecret<'a> {
    pub provider: &'a str,
    pub ciphertext: &'a [u8],
    pub nonce: &'a [u8],
    pub configured_fields: &'a [String],
    pub context_sha256: Option<&'a str>,
    pub read_only_approved: bool,
    pub actor_user_id: Uuid,
}

pub struct AmazonConnectionContext<'a> {
    pub seller_id: &'a str,
    pub region: &'a str,
    pub marketplace_id: &'a str,
}

pub async fn load(
    pool: &PgPool,
    provider: &str,
) -> Result<Option<EncryptedProviderSecret>, sqlx::Error> {
    sqlx::query_as::<_, EncryptedProviderSecret>(
        "SELECT provider, ciphertext, nonce, configured_fields, context_sha256,
                read_only_approved_at, updated_at
         FROM pilot_provider_secrets WHERE provider = $1",
    )
    .bind(provider)
    .fetch_optional(pool)
    .await
}

pub async fn statuses(pool: &PgPool) -> Result<Vec<ProviderSecretStatus>, sqlx::Error> {
    sqlx::query_as::<_, ProviderSecretStatus>(
        "SELECT provider, configured_fields,
                read_only_approved_at IS NOT NULL AS read_only_approved, updated_at
         FROM pilot_provider_secrets ORDER BY provider",
    )
    .fetch_all(pool)
    .await
}

pub async fn store_openai(
    pool: &PgPool,
    input: &StoreEncryptedSecret<'_>,
) -> Result<ProviderSecretStatus, sqlx::Error> {
    debug_assert_eq!(input.provider, OPENAI_PROVIDER);
    let mut tx = pool.begin().await?;
    let replaced = provider_exists(&mut tx, OPENAI_PROVIDER).await?;
    let status = upsert_secret(&mut tx, input).await?;
    append_audit(&mut tx, input, replaced).await?;
    tx.commit().await?;
    Ok(status)
}

pub async fn store_amazon_and_connection(
    pool: &PgPool,
    input: &StoreEncryptedSecret<'_>,
    connection: &AmazonConnectionContext<'_>,
) -> Result<ProviderSecretStatus, sqlx::Error> {
    debug_assert_eq!(input.provider, AMAZON_PROVIDER);
    let mut tx = pool.begin().await?;

    let active_runs = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)
         FROM amazon_report_runs run
         JOIN amazon_connections connection ON connection.id = run.connection_id
         WHERE connection.mode = 'live' AND connection.secret_ref = $1
           AND run.status IN ('queued', 'requesting', 'polling', 'downloading', 'parsing', 'analysing')",
    )
    .bind(PILOT_AMAZON_SECRET_REF)
    .fetch_one(&mut *tx)
    .await?;
    if active_runs != 0 {
        return Err(sqlx::Error::Protocol(
            "Amazon credentials cannot be rotated while a report acquisition is active".into(),
        ));
    }

    let replaced = provider_exists(&mut tx, AMAZON_PROVIDER).await?;
    let status = upsert_secret(&mut tx, input).await?;

    sqlx::query(
        "UPDATE amazon_connections
         SET enabled = false, updated_at = now()
         WHERE mode = 'live' AND secret_ref = $1",
    )
    .bind(PILOT_AMAZON_SECRET_REF)
    .execute(&mut *tx)
    .await?;

    let connection_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO amazon_connections
             (seller_id, region, secret_ref, granted_roles, mode, enabled)
         VALUES ($1, $2, $3, ARRAY['Brand Analytics'], 'live', true)
         ON CONFLICT (seller_id, region, secret_ref) DO UPDATE
         SET granted_roles = ARRAY['Brand Analytics'], mode = 'live', enabled = true,
             updated_at = now()
         RETURNING id",
    )
    .bind(connection.seller_id)
    .bind(connection.region)
    .bind(PILOT_AMAZON_SECRET_REF)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("UPDATE amazon_marketplaces SET enabled = false WHERE connection_id = $1")
        .bind(connection_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO amazon_marketplaces (connection_id, marketplace_id, enabled)
         VALUES ($1, $2, true)
         ON CONFLICT (connection_id, marketplace_id) DO UPDATE SET enabled = true",
    )
    .bind(connection_id)
    .bind(connection.marketplace_id)
    .execute(&mut *tx)
    .await?;

    append_audit(&mut tx, input, replaced).await?;
    tx.commit().await?;
    Ok(status)
}

pub async fn amazon_context_is_approved(
    pool: &PgPool,
    context_sha256: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM pilot_provider_secrets
             WHERE provider = 'amazon' AND context_sha256 = $1
               AND read_only_approved_at IS NOT NULL
         )",
    )
    .bind(context_sha256)
    .fetch_one(pool)
    .await
}

async fn provider_exists(
    tx: &mut Transaction<'_, Postgres>,
    provider: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pilot_provider_secrets WHERE provider = $1)")
        .bind(provider)
        .fetch_one(&mut **tx)
        .await
}

async fn upsert_secret(
    tx: &mut Transaction<'_, Postgres>,
    input: &StoreEncryptedSecret<'_>,
) -> Result<ProviderSecretStatus, sqlx::Error> {
    sqlx::query_as::<_, ProviderSecretStatus>(
        "INSERT INTO pilot_provider_secrets
             (provider, ciphertext, nonce, encryption_algorithm, key_version,
              configured_fields, context_sha256, read_only_approved_at, updated_by)
         VALUES ($1, $2, $3, $4, 1, $5, $6,
                 CASE WHEN $7 THEN now() ELSE NULL END, $8)
         ON CONFLICT (provider) DO UPDATE
         SET ciphertext = EXCLUDED.ciphertext, nonce = EXCLUDED.nonce,
             encryption_algorithm = EXCLUDED.encryption_algorithm,
             key_version = EXCLUDED.key_version,
             configured_fields = EXCLUDED.configured_fields,
             context_sha256 = EXCLUDED.context_sha256,
             read_only_approved_at = EXCLUDED.read_only_approved_at,
             updated_by = EXCLUDED.updated_by, updated_at = now()
         RETURNING provider, configured_fields,
                   read_only_approved_at IS NOT NULL AS read_only_approved, updated_at",
    )
    .bind(input.provider)
    .bind(input.ciphertext)
    .bind(input.nonce)
    .bind(ENCRYPTION_ALGORITHM)
    .bind(input.configured_fields)
    .bind(input.context_sha256)
    .bind(input.read_only_approved)
    .bind(input.actor_user_id)
    .fetch_one(&mut **tx)
    .await
}

async fn append_audit(
    tx: &mut Transaction<'_, Postgres>,
    input: &StoreEncryptedSecret<'_>,
    replaced: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO administrative_audit_log
             (actor_user_id, action, target_type, target_id, idempotency_key, details)
         VALUES ($1, 'pilot.provider_secret_configured', 'provider', $2, $3, $4)",
    )
    .bind(input.actor_user_id)
    .bind(input.provider)
    .bind(format!("provider-secret-{}", Uuid::new_v4()))
    .bind(json!({
        "provider": input.provider,
        "configured_fields": input.configured_fields,
        "replaced": replaced,
        "encryption_algorithm": ENCRYPTION_ALGORITHM,
        "read_only_approved": input.read_only_approved,
        "secret_values_recorded": false,
    }))
    .execute(&mut **tx)
    .await?;
    Ok(())
}
