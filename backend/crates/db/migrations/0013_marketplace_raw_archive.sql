-- Preserve the exact downloaded transport document separately from the bytes
-- decoded for a parser. `raw_content` remains the immutable external archive;
-- `decoded_content` is a deterministic derivative (identical for NONE).

ALTER TABLE amazon_report_documents
    ADD COLUMN decoded_content BYTEA,
    ADD COLUMN decoded_sha256 TEXT;

UPDATE amazon_report_documents
SET decoded_content = raw_content,
    decoded_sha256 = sha256;

ALTER TABLE amazon_report_documents
    ALTER COLUMN decoded_content SET NOT NULL,
    ALTER COLUMN decoded_sha256 SET NOT NULL,
    ADD CONSTRAINT amazon_report_documents_decoded_sha256_check
        CHECK (decoded_sha256 ~ '^[0-9a-f]{64}$');

CREATE OR REPLACE FUNCTION prevent_amazon_report_raw_mutation() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.raw_content IS DISTINCT FROM OLD.raw_content
        OR NEW.sha256 IS DISTINCT FROM OLD.sha256
        OR NEW.decoded_content IS DISTINCT FROM OLD.decoded_content
        OR NEW.decoded_sha256 IS DISTINCT FROM OLD.decoded_sha256 THEN
        RAISE EXCEPTION 'amazon report raw document is immutable';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
