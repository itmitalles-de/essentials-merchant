-- Immutable accounting source entries. DATEV rendering is a derivative of
-- these rows; issued invoices and corrections remain the source snapshots.

CREATE TABLE accounting_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE RESTRICT,
    invoice_line_item_id UUID NOT NULL REFERENCES invoice_line_items(id) ON DELETE RESTRICT,
    document_type TEXT NOT NULL CHECK (document_type IN ('invoice', 'correction')),
    document_number TEXT NOT NULL,
    corrected_document_number TEXT,
    customer_number INTEGER NOT NULL,
    booking_date DATE NOT NULL,
    service_date DATE NOT NULL,
    line_position INTEGER NOT NULL,
    booking_text TEXT NOT NULL,
    currency_code TEXT NOT NULL DEFAULT 'EUR' CHECK (currency_code ~ '^[A-Z]{3}$'),
    net_amount NUMERIC(10, 2) NOT NULL,
    tax_amount NUMERIC(10, 2) NOT NULL,
    gross_amount NUMERIC(10, 2) NOT NULL CHECK (gross_amount <> 0),
    tax_rate_percent NUMERIC(5, 2) NOT NULL,
    source_sha256 TEXT NOT NULL CHECK (source_sha256 ~ '^[0-9a-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (invoice_line_item_id)
);

CREATE INDEX idx_accounting_entries_period
    ON accounting_entries (booking_date, document_number, line_position, id);

CREATE FUNCTION capture_invoice_accounting_entries(target_invoice_id UUID)
RETURNS VOID AS $$
BEGIN
    INSERT INTO accounting_entries (
        invoice_id, invoice_line_item_id, document_type, document_number,
        corrected_document_number, customer_number, booking_date, service_date,
        line_position, booking_text, net_amount, tax_amount, gross_amount,
        tax_rate_percent, source_sha256
    )
    SELECT invoice.id, item.id, invoice.document_type, invoice.invoice_number,
           original.invoice_number,
           COALESCE(
               NULLIF(invoice.customer_snapshot->>'customer_number', '')::integer,
               customer.customer_number
           ),
           invoice.issue_date,
           invoice.issue_date, item.position, item.description, item.net_amount,
           item.vat_amount, item.gross_amount, item.vat_rate_percent,
           encode(digest(concat_ws('|',
               invoice.id::text, item.id::text, invoice.document_type,
               invoice.invoice_number, COALESCE(original.invoice_number, ''),
               COALESCE(
                   NULLIF(invoice.customer_snapshot->>'customer_number', ''),
                   customer.customer_number::text
               ),
               invoice.issue_date::text,
               item.position::text, item.description, item.net_amount::text,
               item.vat_amount::text, item.gross_amount::text,
               item.vat_rate_percent::text
           ), 'sha256'), 'hex')
    FROM invoices invoice
    JOIN invoice_line_items item ON item.invoice_id = invoice.id
    JOIN customers customer ON customer.id = invoice.customer_id
    LEFT JOIN invoices original ON original.id = invoice.corrects_invoice_id
    WHERE invoice.id = target_invoice_id
      AND invoice.status <> 'draft'
      AND invoice.invoice_number IS NOT NULL
      AND invoice.issue_date IS NOT NULL
      AND item.gross_amount <> 0
    ON CONFLICT (invoice_line_item_id) DO NOTHING;
END;
$$ LANGUAGE plpgsql;

CREATE FUNCTION capture_invoice_accounting_entries_trigger() RETURNS TRIGGER AS $$
BEGIN
    PERFORM capture_invoice_accounting_entries(NEW.id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_capture_invoice_accounting_entries_insert
    AFTER INSERT ON invoices
    FOR EACH ROW
    WHEN (NEW.status <> 'draft')
    EXECUTE FUNCTION capture_invoice_accounting_entries_trigger();

CREATE TRIGGER trg_capture_invoice_accounting_entries_update
    AFTER UPDATE OF status ON invoices
    FOR EACH ROW
    WHEN (NEW.status <> 'draft')
    EXECUTE FUNCTION capture_invoice_accounting_entries_trigger();

SELECT capture_invoice_accounting_entries(id)
FROM invoices
WHERE status <> 'draft';

CREATE FUNCTION prevent_accounting_history_mutation() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'accounting history is immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_accounting_entries_mutation
    BEFORE UPDATE OR DELETE ON accounting_entries
    FOR EACH ROW EXECUTE FUNCTION prevent_accounting_history_mutation();

CREATE TABLE accounting_export_batches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    export_type TEXT NOT NULL CHECK (export_type IN ('datev_extf_v13')),
    period_start DATE NOT NULL,
    period_end DATE NOT NULL CHECK (period_end >= period_start),
    idempotency_key TEXT NOT NULL UNIQUE,
    parameters_sha256 TEXT NOT NULL CHECK (parameters_sha256 ~ '^[0-9a-f]{64}$'),
    payload_sha256 TEXT NOT NULL CHECK (payload_sha256 ~ '^[0-9a-f]{64}$'),
    payload BYTEA NOT NULL,
    entry_ids UUID[] NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TRIGGER trg_prevent_accounting_export_mutation
    BEFORE UPDATE OR DELETE ON accounting_export_batches
    FOR EACH ROW EXECUTE FUNCTION prevent_accounting_history_mutation();

UPDATE essentials_modules
SET state = 'disabled', enabled = false, version = '1.0.0',
    compatibility = '{"product":"Essentials+ Merchant","schema_min":14,"datev_header":700,"booking_batch_version":13,"validation":"external_gate"}'::jsonb,
    configuration_requirements = '[{"name":"advisor_and_client_numbers","required":true},{"name":"customer_accounts","required":true},{"name":"revenue_accounts_and_tax_keys","required":true}]'::jsonb,
    api_boundaries = ARRAY['/api/exports/datev'],
    jobs = '{}',
    healthcheck = '{"kind":"mapping_validation","external_validator_required":true}'::jsonb,
    data_ownership = 'core: immutable accounting entries and byte-identical export batches',
    updated_at = now()
WHERE module_id = 'export.datev';
