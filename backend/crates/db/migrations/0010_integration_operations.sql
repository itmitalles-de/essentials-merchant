-- Operational reliability data for the existing Core <-> Vendure adapter.
-- Payloads remain in the owning outboxes; diagnostics deliberately store only
-- redacted metadata and never copy customer or order payloads.

CREATE TABLE integration_request_nonces (
    key_id TEXT NOT NULL,
    nonce TEXT NOT NULL,
    request_timestamp TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (key_id, nonce)
);

CREATE INDEX idx_integration_request_nonces_created
    ON integration_request_nonces (created_at);

CREATE TABLE administrative_audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (action, idempotency_key)
);

CREATE INDEX idx_administrative_audit_created
    ON administrative_audit_log (created_at DESC);

ALTER TABLE integration_outbox
    ADD COLUMN requeue_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN requeued_at TIMESTAMPTZ;

CREATE TABLE integration_remote_status (
    provider TEXT PRIMARY KEY,
    health_status TEXT NOT NULL,
    last_success_at TIMESTAMPTZ,
    last_error TEXT,
    observed_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE integration_remote_events (
    provider TEXT NOT NULL,
    external_event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    status TEXT NOT NULL,
    attempts INTEGER NOT NULL,
    available_at TIMESTAMPTZ,
    locked_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    delivered_at TIMESTAMPTZ,
    observed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (provider, external_event_id)
);

CREATE INDEX idx_integration_remote_events_status
    ON integration_remote_events (provider, status, created_at);

CREATE TABLE integration_admin_commands (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('requeue')),
    target_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    actor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'completed', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    locked_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_integration_admin_commands_claim
    ON integration_admin_commands (provider, created_at)
    WHERE status IN ('pending', 'processing');
