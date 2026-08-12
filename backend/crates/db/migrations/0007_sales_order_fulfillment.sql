ALTER TABLE sales_orders
    ADD COLUMN fulfilled_at TIMESTAMPTZ;

ALTER TABLE stock_movements
    DROP CONSTRAINT stock_movements_reference_type_check;

ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_reference_type_check
    CHECK (reference_type IN ('invoice', 'sales_order', 'manual'));
