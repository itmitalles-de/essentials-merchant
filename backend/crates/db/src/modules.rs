use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};
use thiserror::Error;
use uuid::Uuid;

/// Stable erplite compatibility key. The public manifest ID is
/// `marketplace.amazon_intelligence` and is resolved transparently.
pub const MARKETPLACE_INTELLIGENCE: &str = "marketplace_intelligence";
pub const COMMERCE_VENDURE: &str = "commerce_vendure";
pub const AMAZON_READ_ONLY_PILOT: &str = "pilot_amazon_read_only";

pub const AMAZON_PILOT_ENABLED_MODULES: &[&str] = &[
    "core.operations",
    "core.catalog",
    "core.inventory",
    "core.orders",
    "marketplace.amazon_intelligence",
    "intelligence.rules",
    "pilot.amazon_read_only",
];

pub const AMAZON_PILOT_MUTATION_MODULES: &[&str] = &[
    "commerce.vendure",
    "commerce.storefront",
    "payment.test",
    "shipping.manual",
    "shipping.dhl",
    "shipping.dpd",
    "accounting.invoices",
    "accounting.corrections",
    "export.datev",
    "custom.catalog",
];

const MODULE_COLUMNS: &str =
    "module_key, module_id, module_group, display_name, module_kind, version, state,
     enabled, required, dependencies, conflicts, compatibility,
     configuration_requirements, secret_requirements, api_boundaries,
     navigation_boundaries, jobs, webhooks, healthcheck, data_ownership,
     backup_restore, updated_at";
const MODULE_COLUMNS_QUALIFIED: &str =
    "module.module_key, module.module_id, module.module_group, module.display_name,
     module.module_kind, module.version, module.state, module.enabled, module.required,
     module.dependencies, module.conflicts, module.compatibility,
     module.configuration_requirements, module.secret_requirements, module.api_boundaries,
     module.navigation_boundaries, module.jobs, module.webhooks, module.healthcheck,
     module.data_ownership, module.backup_restore, module.updated_at";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EssentialsModule {
    /// Internal compatibility key; clients should use `module_id`.
    pub module_key: String,
    pub module_id: String,
    pub module_group: String,
    pub display_name: String,
    pub module_kind: String,
    pub version: String,
    pub state: String,
    pub enabled: bool,
    pub required: bool,
    pub dependencies: Vec<String>,
    pub conflicts: Vec<String>,
    pub compatibility: Value,
    pub configuration_requirements: Value,
    pub secret_requirements: Value,
    pub api_boundaries: Vec<String>,
    pub navigation_boundaries: Vec<String>,
    pub jobs: Vec<String>,
    pub webhooks: Vec<String>,
    pub healthcheck: Value,
    pub data_ownership: String,
    pub backup_restore: Value,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ConnectorHealth {
    pub module_key: String,
    pub module_id: String,
    pub configuration_valid: bool,
    pub health_status: String,
    pub checked_at: Option<DateTime<Utc>>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleTransition {
    pub module_id: String,
    pub state: String,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PilotBackupVerification {
    pub outcome: String,
    pub manifest_sha256: String,
    pub repository_revision: String,
    pub details: Value,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmazonPilotStatus {
    pub profile: &'static str,
    pub title: &'static str,
    pub enabled: bool,
    pub compliant: bool,
    pub active_modules: Vec<String>,
    pub disabled_modules: Vec<String>,
    pub mutation_modules: Vec<String>,
    pub unexpected_active_modules: Vec<String>,
    pub missing_required_modules: Vec<String>,
    pub automatic_schedules_enabled: i64,
    pub last_backup_verification: Option<PilotBackupVerification>,
}

#[derive(sqlx::FromRow)]
struct ModuleStateRow {
    module_key: String,
    module_id: String,
    module_kind: String,
    state: String,
    required: bool,
    dependencies: Vec<String>,
    conflicts: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ModuleChangeError {
    #[error("module not found")]
    NotFound,
    #[error("required modules cannot be disabled")]
    Required,
    #[error("module is not installed")]
    NotInstalled,
    #[error("connector configuration has not passed validation")]
    NeedsConfiguration,
    #[error("required dependency is not enabled: {0}")]
    MissingDependency(String),
    #[error("conflicting module is enabled: {0}")]
    Conflict(String),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub async fn list_catalog(pool: &PgPool) -> Result<Vec<EssentialsModule>, sqlx::Error> {
    sqlx::query_as::<_, EssentialsModule>(&format!(
        "SELECT {MODULE_COLUMNS} FROM essentials_modules
         WHERE catalog_visible ORDER BY module_group, module_id"
    ))
    .fetch_all(pool)
    .await
}

pub async fn module_by_identifier(
    pool: &PgPool,
    identifier: &str,
) -> Result<Option<EssentialsModule>, sqlx::Error> {
    sqlx::query_as::<_, EssentialsModule>(&format!(
        "SELECT {MODULE_COLUMNS} FROM essentials_modules
         WHERE module_key = $1 OR module_id = $1"
    ))
    .bind(identifier)
    .fetch_optional(pool)
    .await
}

/// Compatibility helper used by existing workers and tests. Administrative
/// changes must use `transition_state`, which validates and audits atomically.
pub async fn set_enabled(
    pool: &PgPool,
    module_key: &str,
    enabled: bool,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE essentials_modules
         SET enabled = $2, state = CASE WHEN $2 THEN 'enabled' ELSE 'disabled' END,
             updated_at = now()
         WHERE (module_key = $1 OR module_id = $1) AND NOT (required AND NOT $2)",
    )
    .bind(module_key)
    .bind(enabled)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn is_enabled(pool: &PgPool, identifier: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT enabled FROM essentials_modules WHERE module_key = $1 OR module_id = $1",
    )
    .bind(identifier)
    .fetch_optional(pool)
    .await
    .map(|enabled| enabled.unwrap_or(false))
}

/// Atomically applies the pilot through canonical module state. The deployment
/// selector is not consulted by request handlers; once applied, this persisted
/// module is the fail-closed runtime boundary.
pub async fn apply_amazon_read_only_profile(
    pool: &PgPool,
    actor_user_id: Uuid,
) -> Result<AmazonPilotStatus, sqlx::Error> {
    let enabled_modules = AMAZON_PILOT_ENABLED_MODULES
        .iter()
        .map(|module_id| (*module_id).to_owned())
        .collect::<Vec<_>>();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE essentials_modules
         SET enabled = required OR module_id = ANY($1),
             state = CASE
                 WHEN required OR module_id = ANY($1) THEN 'enabled'
                 WHEN state = 'not_installed' THEN 'not_installed'
                 ELSE 'disabled'
             END,
             updated_at = now()",
    )
    .bind(&enabled_modules)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "WITH held AS (
             UPDATE amazon_report_runs run
             SET status = 'failed', failure_code = 'pilot_manual_gate_required',
                 failure_message = 'Live run held during pilot activation; submit a new explicit manual request',
                 completed_at = now(), locked_at = NULL, updated_at = now()
             FROM amazon_connections connection
             WHERE run.connection_id = connection.id AND connection.mode = 'live'
               AND run.status IN ('queued', 'requesting', 'polling', 'downloading', 'parsing', 'analysing')
             RETURNING run.id
         )
         INSERT INTO amazon_report_run_events (run_id, status, message)
         SELECT id, 'failed', 'Live run held by Amazon read-only pilot activation' FROM held",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE amazon_report_schedules SET enabled = false, updated_at = now() WHERE enabled",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO administrative_audit_log
             (actor_user_id, action, target_type, target_id, idempotency_key, details)
         VALUES ($1, 'pilot.profile_apply', 'pilot_profile', 'amazon-read-only',
                 'pilot-profile:amazon-read-only:v1',
                 jsonb_build_object('active_modules', $2::text[],
                                    'automatic_schedules', false,
                                    'business_mutations', false))
         ON CONFLICT (action, idempotency_key) DO NOTHING",
    )
    .bind(actor_user_id)
    .bind(&enabled_modules)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    amazon_pilot_status(pool).await
}

pub async fn amazon_pilot_status(pool: &PgPool) -> Result<AmazonPilotStatus, sqlx::Error> {
    let modules: Vec<(String, bool, bool)> = sqlx::query_as(
        "SELECT module_id, enabled, required FROM essentials_modules ORDER BY module_id",
    )
    .fetch_all(pool)
    .await?;
    let active_modules = modules
        .iter()
        .filter(|(_, enabled, _)| *enabled)
        .map(|(module_id, _, _)| module_id.clone())
        .collect::<Vec<_>>();
    let disabled_modules = modules
        .iter()
        .filter(|(_, enabled, _)| !*enabled)
        .map(|(module_id, _, _)| module_id.clone())
        .collect::<Vec<_>>();
    let unexpected_active_modules = modules
        .iter()
        .filter(|(module_id, enabled, _required)| {
            *enabled && !AMAZON_PILOT_ENABLED_MODULES.contains(&module_id.as_str())
        })
        .map(|(module_id, _, _)| module_id.clone())
        .collect::<Vec<_>>();
    let missing_required_modules = AMAZON_PILOT_ENABLED_MODULES
        .iter()
        .filter(|expected| {
            !modules
                .iter()
                .any(|(module_id, enabled, _)| module_id == *expected && *enabled)
        })
        .map(|module_id| (*module_id).to_owned())
        .collect::<Vec<_>>();
    let automatic_schedules_enabled: i64 =
        sqlx::query_scalar("SELECT count(*) FROM amazon_report_schedules WHERE enabled")
            .fetch_one(pool)
            .await?;
    let last_backup_verification = sqlx::query_as::<_, PilotBackupVerification>(
        "SELECT outcome, manifest_sha256, repository_revision, details, verified_at
         FROM pilot_backup_verifications WHERE profile = 'amazon-read-only'
         ORDER BY verified_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    let enabled = active_modules
        .iter()
        .any(|module_id| module_id == "pilot.amazon_read_only");
    let mutation_modules_disabled = AMAZON_PILOT_MUTATION_MODULES.iter().all(|module_id| {
        !active_modules
            .iter()
            .any(|active| active.as_str() == *module_id)
    });
    let compliant = enabled
        && mutation_modules_disabled
        && unexpected_active_modules.is_empty()
        && missing_required_modules.is_empty()
        && automatic_schedules_enabled == 0;
    Ok(AmazonPilotStatus {
        profile: "amazon-read-only",
        title: "Essentials+ Merchant - Amazon Intelligence Pilot - Read-only",
        enabled,
        compliant,
        active_modules,
        disabled_modules,
        mutation_modules: AMAZON_PILOT_MUTATION_MODULES
            .iter()
            .map(|module_id| (*module_id).to_owned())
            .collect(),
        unexpected_active_modules,
        missing_required_modules,
        automatic_schedules_enabled,
        last_backup_verification,
    })
}

pub async fn user_can_access(
    pool: &PgPool,
    user_id: Uuid,
    user_role: &str,
    identifier: &str,
) -> Result<bool, sqlx::Error> {
    if user_role == "administrator" {
        return is_enabled(pool, identifier).await;
    }
    sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM essentials_modules module
             JOIN user_module_permissions permission ON permission.module_key = module.module_key
             WHERE (module.module_key = $1 OR module.module_id = $1) AND module.enabled
               AND permission.user_id = $2 AND permission.granted
         )",
    )
    .bind(identifier)
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
    sqlx::query_as::<_, EssentialsModule>(&format!(
        "SELECT {} FROM essentials_modules module
         JOIN user_module_permissions permission ON permission.module_key = module.module_key
         WHERE module.catalog_visible AND module.enabled
           AND permission.user_id = $1 AND permission.granted
         ORDER BY module.module_group, module.module_id",
        MODULE_COLUMNS_QUALIFIED
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn connector_health(
    pool: &PgPool,
    identifier: &str,
) -> Result<Option<ConnectorHealth>, sqlx::Error> {
    sqlx::query_as::<_, ConnectorHealth>(
        "SELECT health.module_key, module.module_id, health.configuration_valid,
                health.health_status, health.checked_at, health.message
         FROM connector_module_health health
         JOIN essentials_modules module ON module.module_key = health.module_key
         WHERE health.module_key = $1 OR module.module_id = $1",
    )
    .bind(identifier)
    .fetch_optional(pool)
    .await
}

pub async fn record_connector_health(
    pool: &PgPool,
    identifier: &str,
    configuration_valid: bool,
    health_status: &str,
    message: &str,
) -> Result<Option<ConnectorHealth>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let module_key: Option<String> = sqlx::query_scalar(
        "SELECT module_key FROM essentials_modules
         WHERE module_key = $1 OR module_id = $1 FOR UPDATE",
    )
    .bind(identifier)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(module_key) = module_key else {
        return Ok(None);
    };
    let result = sqlx::query(
        "UPDATE connector_module_health
         SET configuration_valid = $2, health_status = $3, checked_at = now(), message = $4
         WHERE module_key = $1",
    )
    .bind(&module_key)
    .bind(configuration_valid)
    .bind(health_status)
    .bind(sanitize_message(message))
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    sqlx::query(
        "UPDATE essentials_modules
         SET state = CASE
                 WHEN enabled AND $2 THEN 'enabled'
                 WHEN enabled AND NOT $2 THEN 'degraded'
                 WHEN $2 AND state = 'needs_configuration' THEN 'disabled'
                 WHEN NOT $2 AND state <> 'not_installed' THEN 'needs_configuration'
                 ELSE state
             END,
             enabled = CASE WHEN enabled THEN true ELSE false END,
             updated_at = now()
         WHERE module_key = $1",
    )
    .bind(&module_key)
    .bind(configuration_valid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    connector_health(pool, &module_key).await
}

pub async fn transition_state(
    pool: &PgPool,
    actor_user_id: Uuid,
    identifier: &str,
    target_state: &str,
    idempotency_key: &str,
) -> Result<ModuleTransition, ModuleChangeError> {
    if !matches!(target_state, "enabled" | "disabled") {
        return Err(ModuleChangeError::NotInstalled);
    }
    let mut tx = pool.begin().await?;
    let module: Option<ModuleStateRow> = sqlx::query_as(
        "SELECT module_key, module_id, module_kind, state, required, dependencies, conflicts
             FROM essentials_modules WHERE module_key = $1 OR module_id = $1 FOR UPDATE",
    )
    .bind(identifier)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(module) = module else {
        return Err(ModuleChangeError::NotFound);
    };
    let ModuleStateRow {
        module_key,
        module_id,
        module_kind,
        state: current_state,
        required,
        dependencies,
        conflicts,
    } = module;

    if let Some(existing_state) = existing_transition(&mut tx, idempotency_key).await? {
        tx.commit().await?;
        return Ok(ModuleTransition {
            module_id,
            state: existing_state,
            duplicate: true,
        });
    }
    if target_state == "disabled" && required {
        return Err(ModuleChangeError::Required);
    }
    if target_state == "enabled" && current_state == "not_installed" {
        return Err(ModuleChangeError::NotInstalled);
    }
    let mut effective_state = target_state.to_owned();
    if target_state == "enabled" && module_kind == "connector" {
        let health: Option<(bool, String)> = sqlx::query_as(
            "SELECT configuration_valid, health_status FROM connector_module_health
             WHERE module_key = $1",
        )
        .bind(&module_key)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((configured, health_status)) = health else {
            return Err(ModuleChangeError::NeedsConfiguration);
        };
        if !configured {
            return Err(ModuleChangeError::NeedsConfiguration);
        }
        if health_status != "healthy" {
            effective_state = "degraded".to_owned();
        }
    }
    if target_state == "enabled" {
        for dependency in dependencies {
            if !enabled_in_transaction(&mut tx, &dependency).await? {
                return Err(ModuleChangeError::MissingDependency(dependency));
            }
        }
        for conflict in conflicts {
            if enabled_in_transaction(&mut tx, &conflict).await? {
                return Err(ModuleChangeError::Conflict(conflict));
            }
        }
    } else {
        let dependent: Option<String> = sqlx::query_scalar(
            "SELECT module_id FROM essentials_modules
             WHERE enabled AND $1 = ANY(dependencies) ORDER BY module_id LIMIT 1 FOR UPDATE",
        )
        .bind(&module_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(dependent) = dependent {
            return Err(ModuleChangeError::MissingDependency(dependent));
        }
    }

    sqlx::query(
        "UPDATE essentials_modules SET state = $2,
             enabled = $2 IN ('enabled', 'degraded'), updated_at = now()
         WHERE module_key = $1",
    )
    .bind(&module_key)
    .bind(&effective_state)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO administrative_audit_log
             (actor_user_id, action, target_type, target_id, idempotency_key, details)
         VALUES ($1, 'module.state_change', 'module', $2, $3, $4)",
    )
    .bind(actor_user_id)
    .bind(&module_id)
    .bind(idempotency_key)
    .bind(json!({ "previous_state": current_state, "new_state": effective_state.clone() }))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(ModuleTransition {
        module_id,
        state: effective_state,
        duplicate: false,
    })
}

async fn existing_transition(
    tx: &mut Transaction<'_, Postgres>,
    idempotency_key: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT details->>'new_state' FROM administrative_audit_log
         WHERE action = 'module.state_change' AND idempotency_key = $1",
    )
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await
}

async fn enabled_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    module_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query_scalar(
            "SELECT enabled FROM essentials_modules WHERE module_id = $1 FOR UPDATE",
        )
        .bind(module_id)
        .fetch_optional(&mut **tx)
        .await?
        .unwrap_or(false),
    )
}

fn sanitize_message(message: &str) -> String {
    message
        .replace(['\r', '\n'], " ")
        .chars()
        .take(512)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn admin(pool: &PgPool) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO users (username, password_hash, role)
             VALUES ('module-admin', 'synthetic', 'administrator') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn canonical_ids_preserve_legacy_keys_and_required_modules(pool: PgPool) {
        let marketplace = module_by_identifier(&pool, "marketplace.amazon_intelligence")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(marketplace.module_key, MARKETPLACE_INTELLIGENCE);
        assert!(!marketplace.enabled);
        let actor = admin(&pool).await;
        let error = transition_state(&pool, actor, "core.catalog", "disabled", "required-disable")
            .await
            .unwrap_err();
        assert!(matches!(error, ModuleChangeError::Required));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn connector_needs_configuration_and_state_changes_are_idempotent(pool: PgPool) {
        let actor = admin(&pool).await;
        let error = transition_state(&pool, actor, "shipping.dhl", "enabled", "enable-dhl")
            .await
            .unwrap_err();
        assert!(matches!(error, ModuleChangeError::NeedsConfiguration));

        let first = transition_state(
            &pool,
            actor,
            "marketplace.amazon_intelligence",
            "enabled",
            "enable-amazon",
        )
        .await
        .unwrap();
        let duplicate = transition_state(
            &pool,
            actor,
            "marketplace.amazon_intelligence",
            "enabled",
            "enable-amazon",
        )
        .await
        .unwrap();
        assert!(!first.duplicate);
        assert!(duplicate.duplicate);
        assert!(is_enabled(&pool, MARKETPLACE_INTELLIGENCE).await.unwrap());
        let audits: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM administrative_audit_log
             WHERE action = 'module.state_change' AND idempotency_key = 'enable-amazon'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audits, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn users_only_see_enabled_and_granted_modules(pool: PgPool) {
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (username, password_hash, role)
             VALUES ('module-user', 'synthetic', 'user') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO user_module_permissions (user_id, module_key)
             VALUES ($1, 'core_catalog'), ($1, 'marketplace_intelligence')",
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
        let visible = visible_for_user(&pool, user_id, "user").await.unwrap();
        assert!(visible
            .iter()
            .any(|module| module.module_id == "core.catalog"));
        assert!(!visible
            .iter()
            .any(|module| module.module_id == "marketplace.amazon_intelligence"));
        assert!(
            !user_can_access(&pool, user_id, "user", "marketplace.amazon_intelligence")
                .await
                .unwrap()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn amazon_pilot_profile_is_persisted_fail_closed_and_idempotent(pool: PgPool) {
        let actor = admin(&pool).await;
        let connection_id: Uuid = sqlx::query_scalar(
            "INSERT INTO amazon_connections
                 (seller_id, region, secret_ref, granted_roles, mode)
             VALUES ('SYNTHETIC-PILOT', 'eu', 'fixture:pilot', ARRAY['Brand Analytics'], 'fixture')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO amazon_report_schedules
                 (connection_id, marketplace_id, report_type, interval_seconds, enabled)
             VALUES ($1, 'SYNTHETIC-MARKETPLACE', 'GET_SALES_AND_TRAFFIC_REPORT', 900, true)",
        )
        .bind(connection_id)
        .execute(&pool)
        .await
        .unwrap();
        let live_connection_id: Uuid = sqlx::query_scalar(
            "INSERT INTO amazon_connections
                 (seller_id, region, secret_ref, granted_roles, mode)
             VALUES ('SYNTHETIC-LIVE-HOLD', 'eu', 'pilot_seller', ARRAY['Brand Analytics'], 'live')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let held_run_id: Uuid = sqlx::query_scalar(
            "INSERT INTO amazon_report_runs
                 (connection_id, marketplace_id, report_type, trigger_source, idempotency_key)
             VALUES ($1, 'SYNTHETIC-MARKETPLACE', 'GET_SALES_AND_TRAFFIC_REPORT', 'manual',
                     'synthetic-live-run-held-by-profile') RETURNING id",
        )
        .bind(live_connection_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let first = apply_amazon_read_only_profile(&pool, actor).await.unwrap();
        let second = apply_amazon_read_only_profile(&pool, actor).await.unwrap();
        assert!(first.enabled && first.compliant);
        assert!(second.enabled && second.compliant);
        assert_eq!(second.automatic_schedules_enabled, 0);
        for module_id in AMAZON_PILOT_MUTATION_MODULES {
            assert!(!is_enabled(&pool, module_id).await.unwrap(), "{module_id}");
        }
        for module_id in AMAZON_PILOT_ENABLED_MODULES {
            assert!(is_enabled(&pool, module_id).await.unwrap(), "{module_id}");
        }
        assert_eq!(
            module_by_identifier(&pool, "custom.catalog")
                .await
                .unwrap()
                .unwrap()
                .state,
            "not_installed"
        );
        let held: (String, Option<String>) =
            sqlx::query_as("SELECT status, failure_code FROM amazon_report_runs WHERE id = $1")
                .bind(held_run_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(held.0, "failed");
        assert_eq!(held.1.as_deref(), Some("pilot_manual_gate_required"));
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM administrative_audit_log
             WHERE action = 'pilot.profile_apply'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit_count, 1);

        sqlx::query(
            "UPDATE essentials_modules
             SET required = true, enabled = true, state = 'enabled'
             WHERE module_id = 'custom.catalog'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let future_required = apply_amazon_read_only_profile(&pool, actor).await.unwrap();
        assert!(!future_required.compliant);
        assert_eq!(
            future_required.unexpected_active_modules,
            vec!["custom.catalog".to_owned()]
        );
    }
}
