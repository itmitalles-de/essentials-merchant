-- Provider credentials entered through the Mantle pilot UI are encrypted by
-- the application before they cross the database boundary. The encryption key
-- remains host-only and is deliberately absent from this schema and backups.

CREATE TABLE pilot_provider_secrets (
    provider TEXT PRIMARY KEY CHECK (provider IN ('openai', 'amazon')),
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) BETWEEN 17 AND 16384),
    nonce BYTEA NOT NULL CHECK (octet_length(nonce) = 12),
    encryption_algorithm TEXT NOT NULL CHECK (encryption_algorithm = 'AES-256-GCM-v1'),
    key_version SMALLINT NOT NULL DEFAULT 1 CHECK (key_version = 1),
    configured_fields TEXT[] NOT NULL,
    context_sha256 TEXT,
    read_only_approved_at TIMESTAMPTZ,
    updated_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (context_sha256 IS NULL OR context_sha256 ~ '^[0-9a-f]{64}$'),
    CHECK (
        (provider = 'openai' AND context_sha256 IS NULL AND read_only_approved_at IS NULL)
        OR
        (provider = 'amazon' AND context_sha256 IS NOT NULL AND read_only_approved_at IS NOT NULL)
    )
);

CREATE INDEX idx_pilot_provider_secrets_updated
    ON pilot_provider_secrets (updated_at DESC);

UPDATE essentials_modules
SET compatibility = jsonb_set(compatibility, '{schema_min}', '19'::jsonb),
    updated_at = now()
WHERE module_id IN ('marketplace.amazon_intelligence', 'pilot.amazon_read_only');
