-- Mantle's manually triggered AI strategy is globally limited to one accepted
-- provider result per Europe/Berlin calendar week. Existing v1 assessments
-- remain valid historical records and therefore keep a NULL week_start.

ALTER TABLE amazon_ai_strategy_assessments
    ADD COLUMN week_start DATE,
    ADD COLUMN previous_assessment_id UUID
        REFERENCES amazon_ai_strategy_assessments(id) ON DELETE RESTRICT,
    ADD CONSTRAINT amazon_ai_strategy_week_starts_monday
        CHECK (week_start IS NULL OR EXTRACT(ISODOW FROM week_start) = 1),
    ADD CONSTRAINT amazon_ai_strategy_previous_is_not_self
        CHECK (previous_assessment_id IS NULL OR previous_assessment_id <> id);

CREATE UNIQUE INDEX uq_amazon_ai_strategy_assessments_week
    ON amazon_ai_strategy_assessments (week_start)
    WHERE week_start IS NOT NULL;

CREATE INDEX idx_amazon_ai_strategy_assessments_created
    ON amazon_ai_strategy_assessments (created_at DESC);

UPDATE essentials_modules
SET compatibility = jsonb_set(compatibility, '{schema_min}', '18'::jsonb),
    updated_at = now()
WHERE module_id IN ('marketplace.amazon_intelligence', 'pilot.amazon_read_only');
