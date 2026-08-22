-- Manual imports make the read-only Amazon analysis path useful without SP-API
-- credentials. The exact uploaded bytes remain in amazon_report_documents; this
-- table records only non-PII provenance needed for idempotency and operations.

CREATE TABLE amazon_manual_report_imports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL UNIQUE REFERENCES amazon_report_runs(id) ON DELETE RESTRICT,
    analysis_job_id UUID NOT NULL UNIQUE REFERENCES amazon_analysis_jobs(id) ON DELETE RESTRICT,
    raw_sha256 TEXT NOT NULL UNIQUE CHECK (raw_sha256 ~ '^[0-9a-f]{64}$'),
    detected_format TEXT NOT NULL CHECK (detected_format IN ('json', 'csv', 'tsv')),
    report_type TEXT NOT NULL CHECK (report_type = 'GET_SALES_AND_TRAFFIC_REPORT'),
    marketplace_id TEXT NOT NULL CHECK (length(trim(marketplace_id)) BETWEEN 1 AND 64),
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL CHECK (period_end >= period_start),
    granularity TEXT NOT NULL CHECK (granularity IN ('DAY', 'WEEK', 'MONTH', 'PERIOD')),
    source_timezone TEXT NOT NULL CHECK (length(trim(source_timezone)) BETWEEN 1 AND 64),
    currency_code TEXT NOT NULL CHECK (currency_code ~ '^[A-Z]{3}$'),
    parser_version TEXT NOT NULL CHECK (length(trim(parser_version)) BETWEEN 1 AND 64),
    comparability_key TEXT NOT NULL CHECK (length(trim(comparability_key)) BETWEEN 1 AND 512),
    uploaded_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (marketplace_id, report_type, period_start, period_end, comparability_key, parser_version)
);

CREATE INDEX idx_amazon_manual_report_imports_period
    ON amazon_manual_report_imports (marketplace_id, report_type, period_start, period_end);

CREATE FUNCTION prevent_amazon_manual_report_import_mutation() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'manual Amazon report import provenance is immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_amazon_manual_report_import_mutation
    BEFORE UPDATE OR DELETE ON amazon_manual_report_imports
    FOR EACH ROW EXECUTE FUNCTION prevent_amazon_manual_report_import_mutation();

ALTER TABLE amazon_analysis_jobs
    DROP CONSTRAINT amazon_analysis_jobs_analysis_type_check,
    ADD CONSTRAINT amazon_analysis_jobs_analysis_type_check
        CHECK (analysis_type IN ('delta', 'total', 'manual_comparison'));

-- The manual-upload pseudo connection cannot perform transport. Its fixture
-- mode and fixture-prefixed secret reference satisfy the transport fail-closed
-- invariant, while all of its runs are inserted directly after parsing.
INSERT INTO amazon_connections (
    seller_id, region, secret_ref, granted_roles, mode, enabled
)
VALUES (
    'manual-report-import', 'eu', 'fixture:manual-report-import',
    ARRAY['Brand Analytics'], 'fixture', true
)
ON CONFLICT (seller_id, region, secret_ref) DO NOTHING;

UPDATE essentials_modules
SET compatibility = jsonb_set(compatibility, '{schema_min}', '16'::jsonb),
    updated_at = now()
WHERE module_id IN ('marketplace.amazon_intelligence', 'pilot.amazon_read_only');
