pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub admin_username: String,
    pub admin_password: String,
    pub integration_auth: crate::integration_auth::IntegrationAuth,
    pub outbox_policy: db::commerce::OutboxPolicy,
    pub pdf_storage_dir: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET must be set"),
            admin_username: std::env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".into()),
            admin_password: std::env::var("ADMIN_PASSWORD").expect("ADMIN_PASSWORD must be set"),
            integration_auth: integration_auth_from_env(),
            outbox_policy: db::commerce::OutboxPolicy {
                lease_seconds: env_i32("INTEGRATION_LEASE_SECONDS", 300),
                retry_base_seconds: env_i32("INTEGRATION_RETRY_BASE_SECONDS", 2),
                retry_max_seconds: env_i32("INTEGRATION_RETRY_MAX_SECONDS", 3_600),
                max_attempts: env_i32("INTEGRATION_MAX_ATTEMPTS", 20),
            },
            pdf_storage_dir: std::env::var("PDF_STORAGE_DIR")
                .unwrap_or_else(|_| "/data/invoices".into()),
        }
    }
}

fn integration_auth_from_env() -> crate::integration_auth::IntegrationAuth {
    let current = crate::integration_auth::IntegrationKey::new(
        std::env::var("INTEGRATION_KEY_ID").unwrap_or_else(|_| "current".into()),
        std::env::var("INTEGRATION_SECRET").expect("INTEGRATION_SECRET must be set"),
    )
    .expect("INTEGRATION_KEY_ID and INTEGRATION_SECRET must define a valid HMAC key");
    let previous = match (
        std::env::var("INTEGRATION_PREVIOUS_KEY_ID")
            .ok()
            .filter(|value| !value.trim().is_empty()),
        std::env::var("INTEGRATION_PREVIOUS_SECRET")
            .ok()
            .filter(|value| !value.trim().is_empty()),
    ) {
        (Some(id), Some(secret)) => Some(
            crate::integration_auth::IntegrationKey::new(id, secret)
                .expect("previous integration HMAC key must be valid"),
        ),
        (None, None) => None,
        _ => panic!("previous integration key id and secret must be configured together"),
    };
    crate::integration_auth::IntegrationAuth::new(
        current,
        previous,
        i64::from(env_i32("INTEGRATION_MAX_CLOCK_SKEW_SECONDS", 300)),
    )
}

fn env_i32(name: &str, default: i32) -> i32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}
