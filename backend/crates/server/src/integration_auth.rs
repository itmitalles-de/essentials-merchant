use axum::http::{HeaderMap, Method, StatusCode};
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const KEY_ID_HEADER: &str = "x-essentials-key-id";
const TIMESTAMP_HEADER: &str = "x-essentials-timestamp";
const NONCE_HEADER: &str = "x-essentials-nonce";
const SIGNATURE_HEADER: &str = "x-essentials-signature";

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct IntegrationKey {
    pub id: String,
    secret: String,
}

impl IntegrationKey {
    pub fn new(id: String, secret: String) -> anyhow::Result<Self> {
        if id.trim().is_empty() {
            anyhow::bail!("integration key id must not be empty");
        }
        if secret.len() < 16 {
            anyhow::bail!("integration HMAC secret must contain at least 16 characters");
        }
        Ok(Self { id, secret })
    }
}

#[derive(Clone)]
pub struct IntegrationAuth {
    current: IntegrationKey,
    previous: Option<IntegrationKey>,
    max_clock_skew_seconds: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum IntegrationAuthError {
    #[error("missing or malformed integration authentication headers")]
    Malformed,
    #[error("integration request timestamp is outside the accepted window")]
    Expired,
    #[error("unknown integration key id")]
    UnknownKey,
    #[error("invalid integration request signature")]
    InvalidSignature,
    #[error("integration request nonce was already used")]
    Replay,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl IntegrationAuthError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Replay => StatusCode::CONFLICT,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Malformed | Self::Expired | Self::UnknownKey | Self::InvalidSignature => {
                StatusCode::UNAUTHORIZED
            }
        }
    }
}

impl IntegrationAuth {
    pub fn new(
        current: IntegrationKey,
        previous: Option<IntegrationKey>,
        max_clock_skew_seconds: i64,
    ) -> Self {
        Self {
            current,
            previous,
            max_clock_skew_seconds: max_clock_skew_seconds.clamp(30, 900),
        }
    }

    pub async fn verify(
        &self,
        pool: &PgPool,
        headers: &HeaderMap,
        method: &Method,
        path: &str,
        body: &[u8],
    ) -> Result<(), IntegrationAuthError> {
        let key_id = header(headers, KEY_ID_HEADER)?;
        let timestamp = header(headers, TIMESTAMP_HEADER)?
            .parse::<i64>()
            .map_err(|_| IntegrationAuthError::Malformed)?;
        let nonce = header(headers, NONCE_HEADER)?;
        let signature = header(headers, SIGNATURE_HEADER)?;

        if !(16..=128).contains(&nonce.len())
            || !nonce
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(IntegrationAuthError::Malformed);
        }
        if (Utc::now().timestamp() - timestamp).abs() > self.max_clock_skew_seconds {
            return Err(IntegrationAuthError::Expired);
        }

        let key = self.key(key_id).ok_or(IntegrationAuthError::UnknownKey)?;
        verify_signature(
            &key.secret,
            method.as_str(),
            path,
            timestamp,
            nonce,
            body,
            signature,
        )?;

        let request_time = chrono::DateTime::from_timestamp(timestamp, 0)
            .ok_or(IntegrationAuthError::Malformed)?;
        let inserted = sqlx::query(
            "INSERT INTO integration_request_nonces (key_id, nonce, request_timestamp)
             VALUES ($1, $2, $3)
             ON CONFLICT (key_id, nonce) DO NOTHING",
        )
        .bind(key_id)
        .bind(nonce)
        .bind(request_time)
        .execute(pool)
        .await?
        .rows_affected();
        if inserted == 0 {
            return Err(IntegrationAuthError::Replay);
        }

        sqlx::query(
            "DELETE FROM integration_request_nonces
             WHERE created_at < now() - make_interval(secs => $1)",
        )
        .bind((self.max_clock_skew_seconds * 2) as f64)
        .execute(pool)
        .await?;
        Ok(())
    }

    fn key(&self, key_id: &str) -> Option<&IntegrationKey> {
        if self.current.id == key_id {
            Some(&self.current)
        } else {
            self.previous.as_ref().filter(|key| key.id == key_id)
        }
    }
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, IntegrationAuthError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or(IntegrationAuthError::Malformed)
}

fn canonical_request(method: &str, path: &str, timestamp: i64, nonce: &str, body: &[u8]) -> String {
    let body_hash = hex::encode(Sha256::digest(body));
    format!("{method}\n{path}\n{timestamp}\n{nonce}\n{body_hash}")
}

fn verify_signature(
    secret: &str,
    method: &str,
    path: &str,
    timestamp: i64,
    nonce: &str,
    body: &[u8],
    signature: &str,
) -> Result<(), IntegrationAuthError> {
    let supplied = hex::decode(signature).map_err(|_| IntegrationAuthError::Malformed)?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| IntegrationAuthError::InvalidSignature)?;
    mac.update(canonical_request(method, path, timestamp, nonce, body).as_bytes());
    mac.verify_slice(&supplied)
        .map_err(|_| IntegrationAuthError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature(
        secret: &str,
        method: &str,
        path: &str,
        timestamp: i64,
        nonce: &str,
        body: &[u8],
    ) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(canonical_request(method, path, timestamp, nonce, body).as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn signatures_cover_method_path_timestamp_nonce_and_body() {
        let timestamp = 1_700_000_000;
        let expected = signature(
            "synthetic-current-secret-123456",
            "POST",
            "/api/integrations/vendure/orders",
            timestamp,
            "nonce-1234567890",
            br#"{"order":"synthetic"}"#,
        );
        assert!(verify_signature(
            "synthetic-current-secret-123456",
            "POST",
            "/api/integrations/vendure/orders",
            timestamp,
            "nonce-1234567890",
            br#"{"order":"synthetic"}"#,
            &expected,
        )
        .is_ok());
        assert!(verify_signature(
            "synthetic-current-secret-123456",
            "POST",
            "/api/integrations/vendure/orders",
            timestamp,
            "nonce-1234567890",
            br#"{"order":"changed"}"#,
            &expected,
        )
        .is_err());
    }

    #[test]
    fn current_and_previous_rotation_keys_are_selected_by_id() {
        let auth = IntegrationAuth::new(
            IntegrationKey::new("current".into(), "synthetic-current-secret-123456".into())
                .unwrap(),
            Some(
                IntegrationKey::new("previous".into(), "synthetic-old-secret-123456789".into())
                    .unwrap(),
            ),
            300,
        );
        assert_eq!(
            auth.key("current").map(|key| key.id.as_str()),
            Some("current")
        );
        assert_eq!(
            auth.key("previous").map(|key| key.id.as_str()),
            Some("previous")
        );
        assert!(auth.key("retired").is_none());
    }

    fn signed_headers(
        key_id: &str,
        secret: &str,
        timestamp: i64,
        nonce: &str,
        body: &[u8],
    ) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(KEY_ID_HEADER, key_id.parse().unwrap());
        headers.insert(TIMESTAMP_HEADER, timestamp.to_string().parse().unwrap());
        headers.insert(NONCE_HEADER, nonce.parse().unwrap());
        headers.insert(
            SIGNATURE_HEADER,
            signature(
                secret,
                "POST",
                "/api/integrations/vendure/orders",
                timestamp,
                nonce,
                body,
            )
            .parse()
            .unwrap(),
        );
        headers
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn accepts_rotation_key_once_and_rejects_replay_and_expiry(pool: PgPool) {
        let auth = IntegrationAuth::new(
            IntegrationKey::new("current".into(), "synthetic-current-secret-123456".into())
                .unwrap(),
            Some(
                IntegrationKey::new("previous".into(), "synthetic-old-secret-123456789".into())
                    .unwrap(),
            ),
            300,
        );
        let body = br#"{"order":"synthetic"}"#;
        let now = Utc::now().timestamp();
        let old_headers = signed_headers(
            "previous",
            "synthetic-old-secret-123456789",
            now,
            "nonce-previous-123456",
            body,
        );
        auth.verify(
            &pool,
            &old_headers,
            &Method::POST,
            "/api/integrations/vendure/orders",
            body,
        )
        .await
        .unwrap();
        assert!(matches!(
            auth.verify(
                &pool,
                &old_headers,
                &Method::POST,
                "/api/integrations/vendure/orders",
                body,
            )
            .await,
            Err(IntegrationAuthError::Replay)
        ));

        let expired = signed_headers(
            "current",
            "synthetic-current-secret-123456",
            now - 301,
            "nonce-expired-1234567",
            body,
        );
        assert!(matches!(
            auth.verify(
                &pool,
                &expired,
                &Method::POST,
                "/api/integrations/vendure/orders",
                body,
            )
            .await,
            Err(IntegrationAuthError::Expired)
        ));
    }
}
