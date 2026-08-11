CREATE TABLE articles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sku TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    unit TEXT NOT NULL DEFAULT 'Stk',
    sales_price_net NUMERIC(10, 2) NOT NULL DEFAULT 0,
    default_vat_rate_code TEXT NOT NULL DEFAULT 'STANDARD' REFERENCES vat_rates(code),
    purchase_price_net NUMERIC(10, 2),
    -- Denormalized cache, kept correct by the trigger below regardless of write path.
    stock_quantity NUMERIC(10, 2) NOT NULL DEFAULT 0,
    min_stock_quantity NUMERIC(10, 2),
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Append-only ledger. "in"/"out" carry a fixed sign so the direction is self-evident from
-- the type; "adjustment" (e.g. stocktake corrections) may go either way.
CREATE TABLE stock_movements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    article_id UUID NOT NULL REFERENCES articles(id),
    movement_type TEXT NOT NULL CHECK (movement_type IN ('in', 'out', 'adjustment')),
    quantity NUMERIC(10, 2) NOT NULL CHECK (
        (movement_type = 'in' AND quantity > 0) OR
        (movement_type = 'out' AND quantity < 0) OR
        (movement_type = 'adjustment' AND quantity <> 0)
    ),
    reference_type TEXT NOT NULL DEFAULT 'manual' CHECK (reference_type IN ('invoice', 'manual')),
    reference_id UUID,
    note TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_stock_movements_article_id ON stock_movements (article_id);

CREATE FUNCTION apply_stock_movement() RETURNS TRIGGER AS $$
BEGIN
    UPDATE articles SET stock_quantity = stock_quantity + NEW.quantity WHERE id = NEW.article_id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_stock_movements_apply
    AFTER INSERT ON stock_movements
    FOR EACH ROW EXECUTE FUNCTION apply_stock_movement();

ALTER TABLE invoice_line_items
    ADD CONSTRAINT fk_invoice_line_items_article FOREIGN KEY (article_id) REFERENCES articles(id);
