-- One curated, immutable Mantle/Sphagnum business-context baseline may be
-- imported for the internal weekly strategy tool. Source documents and raw
-- notes are deliberately not stored: only bounded, reviewed knowledge items
-- plus their file hashes and provenance references cross this boundary.

CREATE TABLE mantle_business_knowledge (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    scope TEXT NOT NULL UNIQUE CHECK (scope = 'mantle_sphagnum'),
    source_manifest_sha256 TEXT NOT NULL
        CHECK (source_manifest_sha256 ~ '^[0-9a-f]{64}$'),
    content_sha256 TEXT NOT NULL UNIQUE
        CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    source_count INTEGER NOT NULL CHECK (source_count BETWEEN 2 AND 32),
    entry_count INTEGER NOT NULL CHECK (entry_count BETWEEN 1 AND 80),
    knowledge JSONB NOT NULL CHECK (jsonb_typeof(knowledge) = 'object'),
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE FUNCTION prevent_mantle_business_knowledge_mutation() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'Mantle business knowledge baseline is immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_mantle_business_knowledge_mutation
    BEFORE UPDATE OR DELETE ON mantle_business_knowledge
    FOR EACH ROW EXECUTE FUNCTION prevent_mantle_business_knowledge_mutation();

UPDATE essentials_modules
SET compatibility = jsonb_set(compatibility, '{schema_min}', '21'::jsonb),
    updated_at = now()
WHERE module_id IN ('marketplace.amazon_intelligence', 'pilot.amazon_read_only');
