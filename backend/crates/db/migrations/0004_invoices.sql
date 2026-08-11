CREATE TABLE invoices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_number TEXT UNIQUE,
    customer_id UUID NOT NULL REFERENCES customers(id),
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'sent', 'overdue', 'paid', 'cancelled')),
    issue_date DATE,
    due_date DATE,
    -- Snapshots of customer/company master data, populated when the invoice is sent so
    -- later edits to the customer or company settings never retroactively change an
    -- already-issued invoice. NULL while still a draft (draft display uses a live join).
    customer_snapshot JSONB,
    company_snapshot JSONB,
    net_total NUMERIC(10, 2) NOT NULL DEFAULT 0,
    vat_total NUMERIC(10, 2) NOT NULL DEFAULT 0,
    gross_total NUMERIC(10, 2) NOT NULL DEFAULT 0,
    notes TEXT NOT NULL DEFAULT '',
    pdf_path TEXT,
    sent_at TIMESTAMPTZ,
    paid_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_invoices_customer_id ON invoices (customer_id);

CREATE TABLE invoice_line_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    description TEXT NOT NULL,
    article_id UUID,
    quantity NUMERIC(10, 2) NOT NULL DEFAULT 1,
    unit TEXT NOT NULL DEFAULT 'Stk',
    unit_price_net NUMERIC(10, 2) NOT NULL,
    vat_rate_code TEXT NOT NULL REFERENCES vat_rates(code),
    -- Snapshotted at line-item write time: VAT law changes (e.g. the 2020 COVID
    -- rate cut), and historical invoices must keep the rate that applied then.
    vat_rate_percent NUMERIC(5, 2) NOT NULL,
    net_amount NUMERIC(10, 2) NOT NULL,
    vat_amount NUMERIC(10, 2) NOT NULL,
    gross_amount NUMERIC(10, 2) NOT NULL
);

CREATE INDEX idx_invoice_line_items_invoice_id ON invoice_line_items (invoice_id);
