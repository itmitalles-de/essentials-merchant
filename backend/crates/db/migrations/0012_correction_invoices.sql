-- Full correction invoices reverse an issued invoice without mutating the
-- original snapshot or creating a second stock movement.

ALTER TABLE company_settings
    ADD COLUMN correction_number_prefix TEXT NOT NULL DEFAULT 'KR',
    ADD COLUMN next_correction_number INTEGER NOT NULL DEFAULT 1;

ALTER TABLE invoices
    ADD COLUMN document_type TEXT NOT NULL DEFAULT 'invoice'
        CHECK (document_type IN ('invoice', 'correction')),
    ADD COLUMN corrects_invoice_id UUID REFERENCES invoices(id) ON DELETE RESTRICT,
    ADD COLUMN correction_reason TEXT,
    ADD COLUMN correction_idempotency_key TEXT UNIQUE,
    ADD CONSTRAINT invoices_correction_fields_check CHECK (
        (document_type = 'invoice' AND corrects_invoice_id IS NULL
            AND correction_reason IS NULL AND correction_idempotency_key IS NULL)
        OR
        (document_type = 'correction' AND corrects_invoice_id IS NOT NULL
            AND length(trim(correction_reason)) > 0
            AND correction_idempotency_key IS NOT NULL)
    ),
    ADD CONSTRAINT invoices_one_full_correction_per_invoice UNIQUE (corrects_invoice_id);

CREATE INDEX idx_invoices_document_type ON invoices (document_type, created_at DESC);

CREATE TABLE invoice_audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    action TEXT NOT NULL,
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE RESTRICT,
    related_invoice_id UUID REFERENCES invoices(id) ON DELETE RESTRICT,
    idempotency_key TEXT NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (action, idempotency_key)
);

CREATE INDEX idx_invoice_audit_invoice ON invoice_audit_log (invoice_id, created_at);

CREATE FUNCTION prevent_invoice_audit_mutation() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'invoice audit history is immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_invoice_audit_mutation
    BEFORE UPDATE OR DELETE ON invoice_audit_log
    FOR EACH ROW EXECUTE FUNCTION prevent_invoice_audit_mutation();

CREATE FUNCTION prevent_issued_invoice_snapshot_mutation() RETURNS TRIGGER AS $$
BEGIN
    IF OLD.status <> 'draft' AND (
        NEW.invoice_number IS DISTINCT FROM OLD.invoice_number OR
        NEW.customer_id IS DISTINCT FROM OLD.customer_id OR
        NEW.issue_date IS DISTINCT FROM OLD.issue_date OR
        NEW.due_date IS DISTINCT FROM OLD.due_date OR
        NEW.customer_snapshot IS DISTINCT FROM OLD.customer_snapshot OR
        NEW.company_snapshot IS DISTINCT FROM OLD.company_snapshot OR
        NEW.net_total IS DISTINCT FROM OLD.net_total OR
        NEW.vat_total IS DISTINCT FROM OLD.vat_total OR
        NEW.gross_total IS DISTINCT FROM OLD.gross_total OR
        NEW.notes IS DISTINCT FROM OLD.notes OR
        NEW.document_type IS DISTINCT FROM OLD.document_type OR
        NEW.corrects_invoice_id IS DISTINCT FROM OLD.corrects_invoice_id OR
        NEW.correction_reason IS DISTINCT FROM OLD.correction_reason OR
        NEW.correction_idempotency_key IS DISTINCT FROM OLD.correction_idempotency_key OR
        NEW.sent_at IS DISTINCT FROM OLD.sent_at
    ) THEN
        RAISE EXCEPTION 'issued invoice snapshot is immutable';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_issued_invoice_snapshot_mutation
    BEFORE UPDATE ON invoices
    FOR EACH ROW EXECUTE FUNCTION prevent_issued_invoice_snapshot_mutation();

CREATE FUNCTION prevent_issued_invoice_line_mutation() RETURNS TRIGGER AS $$
DECLARE
    parent_status TEXT;
BEGIN
    SELECT status INTO parent_status
    FROM invoices
    WHERE id = CASE WHEN TG_OP = 'DELETE' THEN OLD.invoice_id ELSE NEW.invoice_id END;
    IF parent_status <> 'draft' THEN
        RAISE EXCEPTION 'issued invoice line items are immutable';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_prevent_issued_invoice_line_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON invoice_line_items
    FOR EACH ROW EXECUTE FUNCTION prevent_issued_invoice_line_mutation();

UPDATE essentials_modules
SET state = 'enabled', enabled = true, version = '1.0.0',
    compatibility = jsonb_set(compatibility, '{schema_min}', '12'::jsonb),
    updated_at = now()
WHERE module_id = 'accounting.corrections';
