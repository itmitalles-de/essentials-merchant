-- The Amazon Intelligence pilot is expressed through the existing Essentials+
-- module registry. Runtime mutation gates read this persisted module state;
-- the deployment environment only selects which module profile is applied.

INSERT INTO essentials_modules (
    module_key, module_id, module_group, display_name, module_kind, enabled, state,
    required, dependencies, conflicts, compatibility, configuration_requirements,
    secret_requirements, api_boundaries, navigation_boundaries, jobs, webhooks,
    healthcheck, data_ownership, backup_restore, catalog_visible
)
VALUES (
    'pilot_amazon_read_only', 'pilot.amazon_read_only', 'Pilot',
    'Amazon Intelligence Pilot - Read-only', 'optional', false, 'disabled', false,
    ARRAY['marketplace.amazon_intelligence', 'intelligence.rules'], '{}',
    '{"product":"Essentials+ Merchant","profile":"amazon-read-only","schema_min":15}',
    '[]', '[]', ARRAY['/api'], ARRAY['/marketplace', '/admin-center'],
    ARRAY['amazon-report-worker', 'marketplace-analysis-worker'], '{}',
    '{"kind":"module_allowlist_and_runtime_write_gate","pii":"none"}',
    'core: pilot policy and redacted operational status',
    '{"preserve_on_disable":true,"included_in_backup":true}', true
)
ON CONFLICT (module_key) DO NOTHING;

CREATE TABLE amazon_transport_observations (
    id BIGSERIAL PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES amazon_report_runs(id) ON DELETE RESTRICT,
    operation TEXT NOT NULL CHECK (operation IN (
        'lwa_token_refresh', 'create_report', 'get_report',
        'get_report_document', 'download_report_document'
    )),
    request_id_redacted TEXT,
    rate_limit_limit TEXT,
    retry_after_seconds BIGINT,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (request_id_redacted IS NULL OR request_id_redacted ~ '^sha256:[0-9a-f]{12}$'),
    CHECK (rate_limit_limit IS NULL OR length(rate_limit_limit) <= 64),
    CHECK (retry_after_seconds IS NULL OR retry_after_seconds BETWEEN 0 AND 86400)
);

CREATE INDEX idx_amazon_transport_observations_run
    ON amazon_transport_observations (run_id, id);

CREATE FUNCTION prevent_amazon_transport_observation_mutation() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'amazon transport observations are immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_amazon_transport_observation_mutation
    BEFORE UPDATE OR DELETE ON amazon_transport_observations
    FOR EACH ROW EXECUTE FUNCTION prevent_amazon_transport_observation_mutation();

-- Parser metadata may be appended to an archived document, but the archive row
-- itself must never disappear. The existing trigger already prevents changing
-- raw_content and its hash.
CREATE FUNCTION prevent_amazon_report_document_delete() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'amazon report archive is immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_amazon_report_document_delete
    BEFORE DELETE ON amazon_report_documents
    FOR EACH ROW EXECUTE FUNCTION prevent_amazon_report_document_delete();

CREATE TABLE pilot_backup_verifications (
    id BIGSERIAL PRIMARY KEY,
    profile TEXT NOT NULL CHECK (profile = 'amazon-read-only'),
    outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed')),
    manifest_sha256 TEXT NOT NULL CHECK (manifest_sha256 ~ '^[0-9a-f]{64}$'),
    repository_revision TEXT NOT NULL CHECK (repository_revision ~ '^[0-9a-f]{40}$'),
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_pilot_backup_verifications_latest
    ON pilot_backup_verifications (profile, verified_at DESC);

CREATE FUNCTION prevent_pilot_backup_verification_mutation() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'pilot backup verification history is immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_pilot_backup_verification_mutation
    BEFORE UPDATE OR DELETE ON pilot_backup_verifications
    FOR EACH ROW EXECUTE FUNCTION prevent_pilot_backup_verification_mutation();
