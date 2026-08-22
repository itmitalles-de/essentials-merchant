use std::sync::Arc;

use chrono::{DateTime, Utc};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

const MASTER_KEY_BYTES: usize = 32;
const MAX_ENCRYPTED_BYTES: usize = 16 * 1024;

#[derive(Clone)]
pub struct ProviderSecretStore {
    pool: PgPool,
    key: Option<Arc<[u8; MASTER_KEY_BYTES]>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderSecretError {
    #[error("provider secret storage is not configured")]
    Unavailable,
    #[error("provider credential input is invalid")]
    InvalidInput,
    #[error("provider credential encryption failed")]
    Crypto,
    #[error("provider credential storage is unavailable")]
    Database(#[from] sqlx::Error),
}

#[derive(Serialize)]
struct OpenAiSecretRef<'a> {
    api_key: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenAiSecret {
    api_key: String,
}

#[derive(Serialize)]
struct AmazonSecretRef<'a> {
    refresh_token: &'a str,
    client_id: &'a str,
    client_secret: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AmazonSecret {
    refresh_token: String,
    client_id: String,
    client_secret: String,
}

pub struct AmazonProviderCredentials {
    pub refresh_token: String,
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStatus {
    pub configured: bool,
    pub configured_fields: Vec<String>,
    pub read_only_approved: bool,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderSecretsStatus {
    pub storage_available: bool,
    pub values_are_write_only: bool,
    pub openai: ProviderStatus,
    pub amazon: ProviderStatus,
}

pub struct ConfigureAmazonInput {
    pub refresh_token: String,
    pub client_id: String,
    pub client_secret: String,
    pub seller_id: String,
    pub marketplace_id: String,
    pub region: String,
    pub confirm_authorized: bool,
    pub confirm_read_only: bool,
}

impl ProviderSecretStore {
    pub fn from_env(pool: PgPool, required: bool) -> anyhow::Result<Self> {
        let raw = std::env::var("PILOT_SECRETS_KEY")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let key = match raw {
            Some(raw) => {
                if raw.len() != MASTER_KEY_BYTES * 2
                    || !raw.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    anyhow::bail!("PILOT_SECRETS_KEY must contain exactly 32 hex-encoded bytes");
                }
                let decoded = hex::decode(&raw)
                    .map_err(|_| anyhow::anyhow!("PILOT_SECRETS_KEY is invalid"))?;
                let key: [u8; MASTER_KEY_BYTES] = decoded
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("PILOT_SECRETS_KEY is invalid"))?;
                Some(Arc::new(key))
            }
            None if required => anyhow::bail!(
                "PILOT_SECRETS_KEY must be set when the Mantle no-login pilot is enabled"
            ),
            None => None,
        };
        Ok(Self { pool, key })
    }

    #[cfg(test)]
    pub fn for_test(pool: PgPool) -> Self {
        Self {
            pool,
            key: Some(Arc::new([0x42; MASTER_KEY_BYTES])),
        }
    }

    pub fn available(&self) -> bool {
        self.key.is_some()
    }

    pub async fn status(&self) -> Result<ProviderSecretsStatus, ProviderSecretError> {
        let rows = db::provider_secrets::statuses(&self.pool).await?;
        let status_for = |provider: &str| {
            rows.iter().find(|row| row.provider == provider).map_or(
                ProviderStatus {
                    configured: false,
                    configured_fields: Vec::new(),
                    read_only_approved: false,
                    updated_at: None,
                },
                |row| ProviderStatus {
                    configured: true,
                    configured_fields: row.configured_fields.clone(),
                    read_only_approved: row.read_only_approved,
                    updated_at: Some(row.updated_at),
                },
            )
        };
        Ok(ProviderSecretsStatus {
            storage_available: self.available(),
            values_are_write_only: true,
            openai: status_for(db::provider_secrets::OPENAI_PROVIDER),
            amazon: status_for(db::provider_secrets::AMAZON_PROVIDER),
        })
    }

    pub async fn configure_openai(
        &self,
        api_key: String,
        actor_user_id: Uuid,
    ) -> Result<ProviderStatus, ProviderSecretError> {
        let api_key = Zeroizing::new(api_key);
        validate_openai_key(&api_key)?;
        let plaintext = serde_json::to_vec(&OpenAiSecretRef { api_key: &api_key })
            .map_err(|_| ProviderSecretError::Crypto)?;
        let (mut ciphertext, nonce) =
            self.encrypt(db::provider_secrets::OPENAI_PROVIDER, plaintext)?;
        let fields = vec!["api_key".to_owned()];
        let result = db::provider_secrets::store_openai(
            &self.pool,
            &db::provider_secrets::StoreEncryptedSecret {
                provider: db::provider_secrets::OPENAI_PROVIDER,
                ciphertext: &ciphertext,
                nonce: &nonce,
                configured_fields: &fields,
                context_sha256: None,
                read_only_approved: false,
                actor_user_id,
            },
        )
        .await;
        ciphertext.zeroize();
        let status = result?;
        Ok(ProviderStatus {
            configured: true,
            configured_fields: status.configured_fields,
            read_only_approved: false,
            updated_at: Some(status.updated_at),
        })
    }

    pub async fn configure_amazon(
        &self,
        input: ConfigureAmazonInput,
        actor_user_id: Uuid,
    ) -> Result<ProviderStatus, ProviderSecretError> {
        let refresh_token = Zeroizing::new(input.refresh_token);
        let client_id = Zeroizing::new(input.client_id);
        let client_secret = Zeroizing::new(input.client_secret);
        validate_amazon_secret(&refresh_token, &client_id, &client_secret)?;
        validate_identifier(&input.seller_id)?;
        validate_identifier(&input.marketplace_id)?;
        if !matches!(input.region.as_str(), "na" | "eu" | "fe")
            || !input.confirm_authorized
            || !input.confirm_read_only
        {
            return Err(ProviderSecretError::InvalidInput);
        }
        let plaintext = serde_json::to_vec(&AmazonSecretRef {
            refresh_token: &refresh_token,
            client_id: &client_id,
            client_secret: &client_secret,
        })
        .map_err(|_| ProviderSecretError::Crypto)?;
        let (mut ciphertext, nonce) =
            self.encrypt(db::provider_secrets::AMAZON_PROVIDER, plaintext)?;
        let fields = vec![
            "lwa_client_id".to_owned(),
            "lwa_client_secret".to_owned(),
            "lwa_refresh_token".to_owned(),
            "seller_id".to_owned(),
            "marketplace_id".to_owned(),
            "region".to_owned(),
        ];
        let context_sha256 =
            amazon_context_sha256(&input.seller_id, &input.region, &input.marketplace_id);
        let result = db::provider_secrets::store_amazon_and_connection(
            &self.pool,
            &db::provider_secrets::StoreEncryptedSecret {
                provider: db::provider_secrets::AMAZON_PROVIDER,
                ciphertext: &ciphertext,
                nonce: &nonce,
                configured_fields: &fields,
                context_sha256: Some(&context_sha256),
                read_only_approved: true,
                actor_user_id,
            },
            &db::provider_secrets::AmazonConnectionContext {
                seller_id: &input.seller_id,
                region: &input.region,
                marketplace_id: &input.marketplace_id,
            },
        )
        .await;
        ciphertext.zeroize();
        let status = result?;
        Ok(ProviderStatus {
            configured: true,
            configured_fields: status.configured_fields,
            read_only_approved: status.read_only_approved,
            updated_at: Some(status.updated_at),
        })
    }

    pub async fn openai_api_key(&self) -> Result<Option<String>, ProviderSecretError> {
        let Some(secret) = self
            .decrypt::<OpenAiSecret>(db::provider_secrets::OPENAI_PROVIDER)
            .await?
        else {
            return Ok(None);
        };
        validate_openai_key(&secret.api_key)?;
        Ok(Some(secret.api_key))
    }

    pub async fn amazon_credentials(
        &self,
    ) -> Result<Option<AmazonProviderCredentials>, ProviderSecretError> {
        let Some(secret) = self
            .decrypt::<AmazonSecret>(db::provider_secrets::AMAZON_PROVIDER)
            .await?
        else {
            return Ok(None);
        };
        validate_amazon_secret(
            &secret.refresh_token,
            &secret.client_id,
            &secret.client_secret,
        )?;
        Ok(Some(AmazonProviderCredentials {
            refresh_token: secret.refresh_token,
            client_id: secret.client_id,
            client_secret: secret.client_secret,
        }))
    }

    async fn decrypt<T: for<'de> Deserialize<'de>>(
        &self,
        provider: &str,
    ) -> Result<Option<T>, ProviderSecretError> {
        let Some(row) = db::provider_secrets::load(&self.pool, provider).await? else {
            return Ok(None);
        };
        if row.provider != provider || row.nonce.len() != NONCE_LEN {
            return Err(ProviderSecretError::Crypto);
        }
        let key = self.key.as_ref().ok_or(ProviderSecretError::Unavailable)?;
        let unbound =
            UnboundKey::new(&AES_256_GCM, key.as_ref()).map_err(|_| ProviderSecretError::Crypto)?;
        let key = LessSafeKey::new(unbound);
        let nonce_bytes: [u8; NONCE_LEN] = row
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_| ProviderSecretError::Crypto)?;
        let mut ciphertext = row.ciphertext;
        let plaintext = key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce_bytes),
                Aad::from(provider.as_bytes()),
                &mut ciphertext,
            )
            .map_err(|_| ProviderSecretError::Crypto)?;
        let decoded = serde_json::from_slice(plaintext).map_err(|_| ProviderSecretError::Crypto);
        ciphertext.zeroize();
        decoded.map(Some)
    }

    fn encrypt(
        &self,
        provider: &str,
        mut plaintext: Vec<u8>,
    ) -> Result<(Vec<u8>, [u8; NONCE_LEN]), ProviderSecretError> {
        if plaintext.is_empty() || plaintext.len() > MAX_ENCRYPTED_BYTES - AES_256_GCM.tag_len() {
            plaintext.zeroize();
            return Err(ProviderSecretError::InvalidInput);
        }
        let key = self.key.as_ref().ok_or(ProviderSecretError::Unavailable)?;
        let unbound =
            UnboundKey::new(&AES_256_GCM, key.as_ref()).map_err(|_| ProviderSecretError::Crypto)?;
        let key = LessSafeKey::new(unbound);
        let mut nonce = [0_u8; NONCE_LEN];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| ProviderSecretError::Crypto)?;
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(provider.as_bytes()),
            &mut plaintext,
        )
        .map_err(|_| ProviderSecretError::Crypto)?;
        Ok((plaintext, nonce))
    }
}

pub fn amazon_context_sha256(seller_id: &str, region: &str, marketplace_id: &str) -> String {
    let canonical = format!("mantle-amazon-read-only-v1\0{seller_id}\0{region}\0{marketplace_id}");
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn validate_openai_key(value: &str) -> Result<(), ProviderSecretError> {
    if !(30..=512).contains(&value.len())
        || !value.starts_with("sk-")
        || value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(ProviderSecretError::InvalidInput);
    }
    Ok(())
}

fn validate_amazon_secret(
    refresh_token: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<(), ProviderSecretError> {
    let valid = |value: &str, minimum: usize, maximum: usize| {
        (minimum..=maximum).contains(&value.len())
            && !value
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
    };
    if !valid(refresh_token, 20, 4096)
        || !valid(client_id, 10, 512)
        || !valid(client_secret, 10, 1024)
    {
        return Err(ProviderSecretError::InvalidInput);
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<(), ProviderSecretError> {
    if !(2..=64).contains(&value.len())
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(ProviderSecretError::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amazon_approval_hash_is_stable_and_context_bound() {
        let first = amazon_context_sha256("seller-a", "eu", "market-a");
        assert_eq!(first.len(), 64);
        assert_eq!(first, amazon_context_sha256("seller-a", "eu", "market-a"));
        assert_ne!(first, amazon_context_sha256("seller-b", "eu", "market-a"));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn provider_values_round_trip_encrypted_and_status_stays_write_only(pool: PgPool) {
        let user = db::users::create(&pool, "synthetic-secret-operator", "unused-hash")
            .await
            .unwrap();
        let store = ProviderSecretStore::for_test(pool.clone());
        let openai_key = format!(
            "{}{}",
            "sk-proj-", "synthetic-value-that-never-leaves-the-test"
        );
        let openai_status = store
            .configure_openai(openai_key.clone(), user.id)
            .await
            .unwrap();
        assert!(openai_status.configured);
        assert_eq!(
            store.openai_api_key().await.unwrap(),
            Some(openai_key.clone())
        );

        let amazon_refresh = "Atzr|synthetic-refresh-token-for-storage-test".to_owned();
        store
            .configure_amazon(
                ConfigureAmazonInput {
                    refresh_token: amazon_refresh.clone(),
                    client_id: "amzn1.application-oa2-client.synthetic".to_owned(),
                    client_secret: "synthetic-client-secret-value".to_owned(),
                    seller_id: "SYNTHETICSELLER".to_owned(),
                    marketplace_id: "SYNTHETICMARKET".to_owned(),
                    region: "eu".to_owned(),
                    confirm_authorized: true,
                    confirm_read_only: true,
                },
                user.id,
            )
            .await
            .unwrap();
        let amazon = store.amazon_credentials().await.unwrap().unwrap();
        assert_eq!(amazon.refresh_token, amazon_refresh);

        let encrypted = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT ciphertext FROM pilot_provider_secrets WHERE provider = 'openai'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!encrypted
            .windows(openai_key.len())
            .any(|window| window == openai_key.as_bytes()));

        let status_json = serde_json::to_string(&store.status().await.unwrap()).unwrap();
        assert!(!status_json.contains(&openai_key));
        assert!(!status_json.contains(&amazon_refresh));
        assert!(status_json.contains("configured_fields"));
        let live_connections: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM amazon_connections
             WHERE mode = 'live' AND secret_ref = 'pilot_seller' AND enabled",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(live_connections, 1);
    }
}
