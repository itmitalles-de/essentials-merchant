-- Versioned, local-only product classification for aggregate Amazon analysis.
-- This table grants no Amazon capability and stores neither report rows nor
-- customer data. Revisions are append-only so operator corrections remain
-- auditable without silently rewriting earlier classifications.

CREATE TABLE amazon_product_mapping_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    connection_id UUID NOT NULL,
    marketplace_id TEXT NOT NULL,
    child_asin TEXT NOT NULL CHECK (child_asin ~ '^[A-Z0-9]{10}$'),
    revision INTEGER NOT NULL CHECK (revision > 0),
    brand TEXT NOT NULL CHECK (brand IN ('mantle', 'sphagnum', 'shared', 'other')),
    product_family TEXT NOT NULL CHECK (
        char_length(product_family) BETWEEN 1 AND 80
        AND product_family !~ '[[:cntrl:]]'
    ),
    variant TEXT NOT NULL CHECK (
        char_length(variant) BETWEEN 1 AND 120
        AND variant !~ '[[:cntrl:]]'
    ),
    pack_size TEXT CHECK (
        pack_size IS NULL OR (
            char_length(pack_size) BETWEEN 1 AND 40
            AND pack_size !~ '[[:cntrl:]]'
        )
    ),
    sku TEXT CHECK (
        sku IS NULL OR (
            char_length(sku) BETWEEN 1 AND 64
            AND sku !~ '[[:cntrl:]]'
        )
    ),
    evidence_source TEXT NOT NULL CHECK (
        evidence_source IN ('mantle_wiki', 'seller_central', 'operator_confirmed')
    ),
    enabled BOOLEAN NOT NULL DEFAULT true,
    confirmed_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (connection_id, marketplace_id)
        REFERENCES amazon_marketplaces(connection_id, marketplace_id)
        ON DELETE RESTRICT,
    UNIQUE (connection_id, marketplace_id, child_asin, revision)
);

CREATE INDEX idx_amazon_product_mapping_scope
    ON amazon_product_mapping_revisions
       (connection_id, marketplace_id, child_asin, revision DESC);

CREATE FUNCTION prevent_amazon_product_mapping_revision_mutation() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'Amazon product mapping revisions are append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_amazon_product_mapping_revision_mutation
    BEFORE UPDATE OR DELETE ON amazon_product_mapping_revisions
    FOR EACH ROW EXECUTE FUNCTION prevent_amazon_product_mapping_revision_mutation();

UPDATE essentials_modules
SET compatibility = jsonb_set(compatibility, '{schema_min}', '22'::jsonb),
    updated_at = now()
WHERE module_id IN ('marketplace.amazon_intelligence', 'pilot.amazon_read_only');
