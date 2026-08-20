-- Optional, operator-triggered OpenAI strategy assessments for an existing
-- deterministic Amazon analysis. Only the validated structured assessment and
-- non-sensitive request metadata are stored; prompts and provider payloads are
-- intentionally excluded.

CREATE TABLE amazon_ai_strategy_assessments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    analysis_id UUID NOT NULL REFERENCES amazon_analysis_results(id) ON DELETE RESTRICT,
    payload_sha256 TEXT NOT NULL CHECK (payload_sha256 ~ '^[0-9a-f]{64}$'),
    model_name TEXT NOT NULL CHECK (length(trim(model_name)) BETWEEN 1 AND 80),
    prompt_version TEXT NOT NULL CHECK (length(trim(prompt_version)) BETWEEN 1 AND 80),
    result JSONB NOT NULL CHECK (jsonb_typeof(result) = 'object'),
    provider_request_id_redacted TEXT
        CHECK (provider_request_id_redacted IS NULL OR provider_request_id_redacted ~ '^[0-9a-f]{12}$'),
    input_tokens BIGINT CHECK (input_tokens IS NULL OR input_tokens >= 0),
    output_tokens BIGINT CHECK (output_tokens IS NULL OR output_tokens >= 0),
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (analysis_id, payload_sha256, model_name, prompt_version)
);

CREATE INDEX idx_amazon_ai_strategy_assessments_analysis
    ON amazon_ai_strategy_assessments (analysis_id, created_at DESC);

CREATE FUNCTION prevent_amazon_ai_strategy_assessment_mutation() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'Amazon AI strategy assessments are immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_amazon_ai_strategy_assessment_mutation
    BEFORE UPDATE OR DELETE ON amazon_ai_strategy_assessments
    FOR EACH ROW EXECUTE FUNCTION prevent_amazon_ai_strategy_assessment_mutation();

UPDATE essentials_modules
SET compatibility = jsonb_set(compatibility, '{schema_min}', '17'::jsonb),
    updated_at = now()
WHERE module_id IN ('marketplace.amazon_intelligence', 'pilot.amazon_read_only');
