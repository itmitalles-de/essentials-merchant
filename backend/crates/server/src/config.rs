pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub admin_username: String,
    pub admin_password: String,
    pub integration_auth: crate::integration_auth::IntegrationAuth,
    pub outbox_policy: db::commerce::OutboxPolicy,
    pub pdf_storage_dir: String,
    pub module_profile: Option<ModuleProfile>,
    pub mantle_pilot_no_login: bool,
    pub marketplace_worker_interval_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleProfile {
    AmazonReadOnly,
}

impl Config {
    pub fn from_env() -> Self {
        let module_profile = module_profile_from_env();
        let mantle_pilot_no_login = env_bool("MANTLE_PILOT_NO_LOGIN", false);
        if mantle_pilot_no_login && module_profile != Some(ModuleProfile::AmazonReadOnly) {
            panic!("MANTLE_PILOT_NO_LOGIN requires ESSENTIALS_MODULE_PROFILE=amazon-read-only");
        }
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
            module_profile,
            mantle_pilot_no_login,
            marketplace_worker_interval_seconds: env_u64("MARKETPLACE_WORKER_INTERVAL_SECONDS", 30)
                .clamp(1, 3_600),
        }
    }
}

fn module_profile_from_env() -> Option<ModuleProfile> {
    match std::env::var("ESSENTIALS_MODULE_PROFILE")
        .unwrap_or_default()
        .trim()
    {
        "" => None,
        "amazon-read-only" => Some(ModuleProfile::AmazonReadOnly),
        value => panic!("unsupported ESSENTIALS_MODULE_PROFILE: {value}"),
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

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name).ok().as_deref().map(str::trim) {
        None | Some("") => default,
        Some("1" | "true" | "yes" | "on") => true,
        Some("0" | "false" | "no" | "off") => false,
        Some(_) => panic!("{name} must be a boolean"),
    }
}
