CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Single-row table: id is always 1, enforced so callers can never accidentally create a second company.
CREATE TABLE company_settings (
    id SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    company_name TEXT NOT NULL DEFAULT '',
    owner_name TEXT NOT NULL DEFAULT '',
    address_line1 TEXT NOT NULL DEFAULT '',
    address_line2 TEXT NOT NULL DEFAULT '',
    zip TEXT NOT NULL DEFAULT '',
    city TEXT NOT NULL DEFAULT '',
    country TEXT NOT NULL DEFAULT 'DE',
    email TEXT NOT NULL DEFAULT '',
    phone TEXT NOT NULL DEFAULT '',
    tax_id TEXT NOT NULL DEFAULT '',
    vat_id TEXT NOT NULL DEFAULT '',
    iban TEXT NOT NULL DEFAULT '',
    bic TEXT NOT NULL DEFAULT '',
    bank_name TEXT NOT NULL DEFAULT '',
    invoice_number_prefix TEXT NOT NULL DEFAULT 'RE',
    next_invoice_number INTEGER NOT NULL DEFAULT 1,
    next_customer_number INTEGER NOT NULL DEFAULT 1,
    invoice_footer_note TEXT NOT NULL DEFAULT '',
    default_payment_terms_days INTEGER NOT NULL DEFAULT 14,
    skr TEXT NOT NULL DEFAULT 'SKR03',
    datev_berater_nr TEXT NOT NULL DEFAULT '',
    datev_mandant_nr TEXT NOT NULL DEFAULT '',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO company_settings (id) VALUES (1);
