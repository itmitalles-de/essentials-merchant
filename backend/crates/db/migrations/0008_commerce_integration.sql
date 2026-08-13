ALTER TABLE sales_orders
    DROP CONSTRAINT sales_orders_source_check;

ALTER TABLE sales_orders
    ADD CONSTRAINT sales_orders_source_check
    CHECK (source IN ('manual', 'woocommerce', 'amazon', 'ebay', 'vendure'));

ALTER TABLE sales_orders
    ADD COLUMN external_status TEXT,
    ADD COLUMN stock_booked_at TIMESTAMPTZ;

ALTER TABLE sales_order_items
    ADD COLUMN external_line_id TEXT,
    ADD COLUMN unit_price_net NUMERIC(10, 2) NOT NULL DEFAULT 0,
    ADD COLUMN vat_rate_percent NUMERIC(5, 2) NOT NULL DEFAULT 0,
    ADD COLUMN gross_amount NUMERIC(10, 2) NOT NULL DEFAULT 0;

CREATE TABLE external_entity_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    internal_id UUID NOT NULL,
    external_id TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, entity_type, internal_id),
    UNIQUE (provider, entity_type, external_id)
);

CREATE TABLE integration_inbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'completed'
        CHECK (status IN ('completed', 'failed')),
    last_error TEXT,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ,
    UNIQUE (source, event_id)
);

CREATE TABLE integration_outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sequence BIGSERIAL NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    payload JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'delivered', 'dead')),
    attempts INTEGER NOT NULL DEFAULT 0,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    delivered_at TIMESTAMPTZ
);

CREATE INDEX idx_integration_outbox_pending
    ON integration_outbox (available_at, created_at)
    WHERE status = 'pending';

CREATE FUNCTION enqueue_vendure_article_projection() RETURNS TRIGGER AS $$
DECLARE
    event_uuid UUID := gen_random_uuid();
    vat_percent NUMERIC(5, 2);
BEGIN
    SELECT rate_percent INTO vat_percent
    FROM vat_rates
    WHERE code = NEW.default_vat_rate_code;

    INSERT INTO integration_outbox (
        id, event_type, aggregate_type, aggregate_id, idempotency_key, payload
    ) VALUES (
        event_uuid,
        'vendure.product.project',
        'article',
        NEW.id,
        'article:' || NEW.id::text || ':' || event_uuid::text,
        jsonb_build_object(
            'core_id', NEW.id,
            'sku', NEW.sku,
            'name', NEW.name,
            'unit', NEW.unit,
            'sales_price_net', NEW.sales_price_net,
            'vat_rate_percent', vat_percent,
            'available_stock', NEW.stock_quantity,
            'active', NEW.active
        )
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_articles_vendure_projection_insert
    AFTER INSERT ON articles
    FOR EACH ROW EXECUTE FUNCTION enqueue_vendure_article_projection();

CREATE TRIGGER trg_articles_vendure_projection_update
    AFTER UPDATE ON articles
    FOR EACH ROW
    WHEN (
        OLD.sku IS DISTINCT FROM NEW.sku OR
        OLD.name IS DISTINCT FROM NEW.name OR
        OLD.sales_price_net IS DISTINCT FROM NEW.sales_price_net OR
        OLD.default_vat_rate_code IS DISTINCT FROM NEW.default_vat_rate_code OR
        OLD.stock_quantity IS DISTINCT FROM NEW.stock_quantity OR
        OLD.active IS DISTINCT FROM NEW.active
    )
    EXECUTE FUNCTION enqueue_vendure_article_projection();

INSERT INTO integration_outbox (
    id, event_type, aggregate_type, aggregate_id, idempotency_key, payload
)
SELECT
    event_uuid,
    'vendure.product.project',
    'article',
    article_id,
    'article:' || article_id::text || ':' || event_uuid::text,
    payload
FROM (
    SELECT
        gen_random_uuid() AS event_uuid,
        a.id AS article_id,
        jsonb_build_object(
            'core_id', a.id,
            'sku', a.sku,
            'name', a.name,
            'unit', a.unit,
            'sales_price_net', a.sales_price_net,
            'vat_rate_percent', v.rate_percent,
            'available_stock', a.stock_quantity,
            'active', a.active
        ) AS payload
    FROM articles a
    JOIN vat_rates v ON v.code = a.default_vat_rate_code
) projections;
