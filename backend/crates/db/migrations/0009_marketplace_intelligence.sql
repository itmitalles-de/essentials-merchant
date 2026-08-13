-- Marketplace Intelligence is deliberately isolated from sales-order imports.
-- It records aggregated seller reports only and never issues business-changing
-- Amazon API calls.

CREATE TABLE amazon_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    seller_id TEXT NOT NULL,
    region TEXT NOT NULL CHECK (region IN ('na', 'eu', 'fe')),
    -- A logical lookup key for an environment-managed secret. It is not a
    -- refresh token, OAuth client secret, or access token.
    secret_ref TEXT NOT NULL,
    granted_roles TEXT[] NOT NULL DEFAULT '{}',
    mode TEXT NOT NULL DEFAULT 'live' CHECK (mode IN ('live', 'fixture')),
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (seller_id, region, secret_ref)
);

-- Essentials Plus module contract. Existing users keep administrator access so
-- an upgrade cannot lock the current local administrator out of Core workflows.
ALTER TABLE users
    ADD COLUMN role TEXT NOT NULL DEFAULT 'administrator'
        CHECK (role IN ('administrator', 'user'));

CREATE TABLE essentials_modules (
    module_key TEXT PRIMARY KEY,
    module_group TEXT NOT NULL,
    display_name TEXT NOT NULL,
    module_kind TEXT NOT NULL CHECK (module_kind IN ('core', 'optional', 'connector')),
    enabled BOOLEAN NOT NULL DEFAULT false,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE user_module_permissions (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    module_key TEXT NOT NULL REFERENCES essentials_modules(module_key) ON DELETE RESTRICT,
    granted BOOLEAN NOT NULL DEFAULT true,
    PRIMARY KEY (user_id, module_key)
);

CREATE TABLE connector_module_health (
    module_key TEXT PRIMARY KEY REFERENCES essentials_modules(module_key) ON DELETE RESTRICT,
    configuration_valid BOOLEAN NOT NULL DEFAULT false,
    health_status TEXT NOT NULL DEFAULT 'not_configured'
        CHECK (health_status IN ('not_configured', 'healthy', 'degraded', 'failed')),
    checked_at TIMESTAMPTZ,
    message TEXT
);

INSERT INTO essentials_modules (module_key, module_group, display_name, module_kind, enabled)
VALUES
    ('core_operations', 'Operations', 'Merchant Core', 'core', true),
    ('marketplace_intelligence', 'Marketplace', 'Marketplace Intelligence', 'optional', false),
    ('connector_dhl', 'Shipping connectors', 'DHL connector', 'connector', false),
    ('connector_dpd', 'Shipping connectors', 'DPD connector', 'connector', false)
ON CONFLICT (module_key) DO NOTHING;

INSERT INTO connector_module_health (module_key)
VALUES ('connector_dhl'), ('connector_dpd')
ON CONFLICT (module_key) DO NOTHING;

CREATE TABLE amazon_marketplaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    connection_id UUID NOT NULL REFERENCES amazon_connections(id) ON DELETE RESTRICT,
    marketplace_id TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (connection_id, marketplace_id)
);

CREATE TABLE amazon_report_schedules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    connection_id UUID NOT NULL REFERENCES amazon_connections(id) ON DELETE RESTRICT,
    marketplace_id TEXT NOT NULL,
    report_type TEXT NOT NULL,
    report_options JSONB NOT NULL DEFAULT '{}'::jsonb,
    interval_seconds INTEGER NOT NULL CHECK (interval_seconds BETWEEN 900 AND 2678400),
    enabled BOOLEAN NOT NULL DEFAULT false,
    next_run_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_enqueued_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (connection_id, marketplace_id, report_type)
);

CREATE INDEX idx_amazon_report_schedules_due
    ON amazon_report_schedules (next_run_at)
    WHERE enabled;

CREATE TABLE amazon_report_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    connection_id UUID NOT NULL REFERENCES amazon_connections(id) ON DELETE RESTRICT,
    schedule_id UUID REFERENCES amazon_report_schedules(id) ON DELETE SET NULL,
    marketplace_id TEXT NOT NULL,
    report_type TEXT NOT NULL,
    data_start_time TIMESTAMPTZ,
    data_end_time TIMESTAMPTZ,
    report_options JSONB NOT NULL DEFAULT '{}'::jsonb,
    trigger_source TEXT NOT NULL CHECK (trigger_source IN ('manual', 'scheduled')),
    idempotency_key TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN (
        'queued', 'requesting', 'polling', 'downloading', 'parsing',
        'analysing', 'succeeded', 'archived', 'cancelled', 'fatal', 'failed'
    )),
    attempts INTEGER NOT NULL DEFAULT 0,
    poll_attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    amazon_report_id TEXT,
    amazon_report_document_id TEXT,
    failure_code TEXT,
    failure_message TEXT,
    requested_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_amazon_report_runs_claim
    ON amazon_report_runs (next_attempt_at, created_at)
    WHERE status IN ('queued', 'requesting', 'polling', 'downloading', 'parsing', 'analysing');
CREATE INDEX idx_amazon_report_runs_comparable
    ON amazon_report_runs (connection_id, marketplace_id, report_type, completed_at DESC)
    WHERE status = 'succeeded';

CREATE TABLE amazon_report_run_events (
    id BIGSERIAL PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES amazon_report_runs(id) ON DELETE RESTRICT,
    status TEXT NOT NULL,
    message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_amazon_report_run_events_run ON amazon_report_run_events (run_id, id);

CREATE TABLE amazon_report_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL UNIQUE REFERENCES amazon_report_runs(id) ON DELETE RESTRICT,
    amazon_report_document_id TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    content_type TEXT,
    compression_algorithm TEXT,
    raw_content BYTEA NOT NULL,
    downloaded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    parser_version TEXT,
    import_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (import_status IN ('pending', 'parsed', 'unsupported', 'failed')),
    import_error TEXT
);

CREATE FUNCTION prevent_amazon_report_raw_mutation() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.raw_content IS DISTINCT FROM OLD.raw_content OR NEW.sha256 IS DISTINCT FROM OLD.sha256 THEN
        RAISE EXCEPTION 'amazon report raw document is immutable';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_amazon_report_raw_mutation
    BEFORE UPDATE ON amazon_report_documents
    FOR EACH ROW EXECUTE FUNCTION prevent_amazon_report_raw_mutation();

CREATE TABLE amazon_metric_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL UNIQUE REFERENCES amazon_report_runs(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL REFERENCES amazon_connections(id) ON DELETE RESTRICT,
    marketplace_id TEXT NOT NULL,
    report_type TEXT NOT NULL,
    parser_version TEXT NOT NULL,
    period_start TIMESTAMPTZ,
    period_end TIMESTAMPTZ,
    granularity TEXT NOT NULL,
    comparability_key TEXT NOT NULL,
    summary JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_amazon_metric_snapshots_comparable
    ON amazon_metric_snapshots (connection_id, marketplace_id, report_type, comparability_key, created_at DESC);

CREATE TABLE amazon_normalized_metrics (
    id BIGSERIAL PRIMARY KEY,
    snapshot_id UUID NOT NULL REFERENCES amazon_metric_snapshots(id) ON DELETE RESTRICT,
    metric_name TEXT NOT NULL,
    dimension_type TEXT NOT NULL DEFAULT 'catalog',
    dimension_key TEXT NOT NULL DEFAULT '',
    value_numeric NUMERIC(20, 6) NOT NULL,
    unit TEXT NOT NULL,
    currency_code TEXT,
    evidence JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (snapshot_id, metric_name, dimension_type, dimension_key, unit, currency_code)
);

CREATE INDEX idx_amazon_normalized_metrics_snapshot ON amazon_normalized_metrics (snapshot_id, metric_name);

CREATE TABLE amazon_analysis_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID REFERENCES amazon_report_runs(id) ON DELETE SET NULL,
    connection_id UUID NOT NULL REFERENCES amazon_connections(id) ON DELETE RESTRICT,
    marketplace_id TEXT NOT NULL,
    report_type TEXT,
    analysis_type TEXT NOT NULL CHECK (analysis_type IN ('delta', 'total')),
    period_start TIMESTAMPTZ,
    period_end TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'processing', 'completed', 'failed')),
    locked_at TIMESTAMPTZ,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX idx_amazon_analysis_jobs_delta_per_run
    ON amazon_analysis_jobs (run_id, analysis_type)
    WHERE run_id IS NOT NULL AND analysis_type = 'delta';
CREATE INDEX idx_amazon_analysis_jobs_claim
    ON amazon_analysis_jobs (next_attempt_at, created_at)
    WHERE status IN ('queued', 'processing');

CREATE TABLE amazon_analysis_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL UNIQUE REFERENCES amazon_analysis_jobs(id) ON DELETE RESTRICT,
    strategy TEXT NOT NULL,
    model_name TEXT,
    prompt_version TEXT NOT NULL,
    payload_sha256 TEXT NOT NULL CHECK (payload_sha256 ~ '^[0-9a-f]{64}$'),
    result JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
