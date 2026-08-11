CREATE TABLE vat_rates (
    code TEXT PRIMARY KEY,
    rate_percent NUMERIC(5, 2) NOT NULL,
    sort_order SMALLINT NOT NULL
);

INSERT INTO vat_rates (code, rate_percent, sort_order) VALUES
    ('STANDARD', 19.00, 0),
    ('REDUCED', 7.00, 1),
    ('ZERO', 0.00, 2);

CREATE TABLE customers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_number INTEGER NOT NULL UNIQUE,
    name TEXT NOT NULL,
    contact_person TEXT NOT NULL DEFAULT '',
    address_line1 TEXT NOT NULL DEFAULT '',
    address_line2 TEXT NOT NULL DEFAULT '',
    zip TEXT NOT NULL DEFAULT '',
    city TEXT NOT NULL DEFAULT '',
    country TEXT NOT NULL DEFAULT 'DE',
    email TEXT NOT NULL DEFAULT '',
    phone TEXT NOT NULL DEFAULT '',
    ust_id TEXT NOT NULL DEFAULT '',
    default_payment_terms_days INTEGER,
    notes TEXT NOT NULL DEFAULT '',
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
