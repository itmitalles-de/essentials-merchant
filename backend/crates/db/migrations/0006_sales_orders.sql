CREATE TABLE sales_orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_number BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE,
    customer_id UUID NOT NULL REFERENCES customers(id),
    source TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'woocommerce', 'amazon', 'ebay')),
    external_order_id TEXT,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'fulfilled', 'cancelled')),
    shipping_carrier TEXT CHECK (shipping_carrier IN ('dhl', 'hermes', 'dpd')),
    tracking_number TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (source, external_order_id)
);

CREATE TABLE sales_order_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sales_order_id UUID NOT NULL REFERENCES sales_orders(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    article_id UUID REFERENCES articles(id),
    description TEXT NOT NULL,
    quantity NUMERIC(10, 2) NOT NULL CHECK (quantity > 0),
    unit TEXT NOT NULL DEFAULT 'Stk',
    UNIQUE (sales_order_id, position)
);

CREATE INDEX idx_sales_orders_customer_id ON sales_orders (customer_id);
CREATE INDEX idx_sales_orders_status ON sales_orders (status);
CREATE INDEX idx_sales_order_items_order_id ON sales_order_items (sales_order_id);
