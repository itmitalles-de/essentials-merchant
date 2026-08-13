-- Canonical Essentials+ module manifests. `module_key` remains the stable
-- erplite compatibility key used by existing installations and foreign keys;
-- `module_id` is the public Essentials+ identifier.

ALTER TABLE essentials_modules
    ADD COLUMN module_id TEXT,
    ADD COLUMN version TEXT NOT NULL DEFAULT '1.0.0',
    ADD COLUMN state TEXT NOT NULL DEFAULT 'disabled',
    ADD COLUMN required BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN dependencies TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN conflicts TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN compatibility JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN configuration_requirements JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN secret_requirements JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN api_boundaries TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN navigation_boundaries TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN jobs TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN webhooks TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN healthcheck JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN data_ownership TEXT NOT NULL DEFAULT 'core',
    ADD COLUMN backup_restore JSONB NOT NULL DEFAULT '{"preserve_on_disable":true}'::jsonb,
    ADD COLUMN catalog_visible BOOLEAN NOT NULL DEFAULT true;

UPDATE essentials_modules
SET module_id = CASE module_key
        WHEN 'core_operations' THEN 'core.operations'
        WHEN 'marketplace_intelligence' THEN 'marketplace.amazon_intelligence'
        WHEN 'connector_dhl' THEN 'shipping.dhl'
        WHEN 'connector_dpd' THEN 'shipping.dpd'
        ELSE module_key
    END,
    state = CASE
        WHEN enabled THEN 'enabled'
        WHEN module_kind = 'connector' THEN 'needs_configuration'
        ELSE 'disabled'
    END,
    required = module_key = 'core_operations',
    catalog_visible = module_key <> 'core_operations';

ALTER TABLE essentials_modules
    ALTER COLUMN module_id SET NOT NULL,
    ADD CONSTRAINT essentials_modules_module_id_unique UNIQUE (module_id),
    ADD CONSTRAINT essentials_modules_state_check CHECK (state IN (
        'not_installed', 'needs_configuration', 'disabled', 'enabled', 'degraded'
    )),
    ADD CONSTRAINT essentials_modules_required_enabled_check CHECK (
        NOT required OR (state = 'enabled' AND enabled)
    ),
    ADD CONSTRAINT essentials_modules_enabled_state_check CHECK (
        enabled = (state IN ('enabled', 'degraded'))
    );

UPDATE essentials_modules
SET module_group = CASE module_key
        WHEN 'marketplace_intelligence' THEN 'Marktplätze'
        WHEN 'connector_dhl' THEN 'Versand'
        WHEN 'connector_dpd' THEN 'Versand'
        ELSE 'Katalog und Bestand'
    END,
    version = '1.0.0',
    compatibility = '{"product":"Essentials+ Merchant","core_api":"1","schema_min":11}'::jsonb,
    backup_restore = '{"preserve_on_disable":true,"included_in_backup":true}'::jsonb;

UPDATE essentials_modules
SET configuration_requirements = '[{"name":"seller_context","required":true},{"name":"marketplaces","required":true}]'::jsonb,
    secret_requirements = '[{"name":"lwa_refresh_token","mechanism":"secret_ref"},{"name":"lwa_client_secret","mechanism":"secret_ref"}]'::jsonb,
    api_boundaries = ARRAY['/api/marketplace'],
    navigation_boundaries = ARRAY['/marketplace'],
    jobs = ARRAY['amazon-report-scheduler', 'amazon-report-worker', 'marketplace-analysis-worker'],
    healthcheck = '{"kind":"connection_and_registry","pii":"none"}'::jsonb,
    data_ownership = 'core:amazon_*; immutable raw reports retained when disabled'
WHERE module_key = 'marketplace_intelligence';

UPDATE essentials_modules
SET configuration_requirements = '[{"name":"account_number","required":true},{"name":"environment","required":true}]'::jsonb,
    secret_requirements = '[{"name":"api_credentials","mechanism":"secret_ref"}]'::jsonb,
    api_boundaries = ARRAY['/api/shipping'],
    jobs = ARRAY['shipping-reconciliation'],
    webhooks = ARRAY['shipping-status'],
    healthcheck = '{"kind":"configuration_and_provider","pii":"none"}'::jsonb,
    data_ownership = 'connector mappings and audit in core'
WHERE module_key IN ('connector_dhl', 'connector_dpd');

INSERT INTO essentials_modules (
    module_key, module_id, module_group, display_name, module_kind, enabled, state,
    required, dependencies, conflicts, compatibility, configuration_requirements,
    secret_requirements, api_boundaries, navigation_boundaries, jobs, webhooks,
    healthcheck, data_ownership, backup_restore
)
VALUES
    ('core_catalog', 'core.catalog', 'Katalog und Bestand', 'Katalog', 'core', true, 'enabled', true,
     '{}', '{}', '{"product":"Essentials+ Merchant","core_api":"1","schema_min":11}', '[]', '[]',
     ARRAY['/api/articles'], ARRAY['/articles'], ARRAY['vendure-product-projection'], '{}',
     '{"kind":"database"}', 'core: articles and immutable projection history',
     '{"preserve_on_disable":true,"included_in_backup":true}'),
    ('core_inventory', 'core.inventory', 'Katalog und Bestand', 'Bestand', 'core', true, 'enabled', true,
     ARRAY['core.catalog'], '{}', '{"product":"Essentials+ Merchant","core_api":"1","schema_min":11}', '[]', '[]',
     ARRAY['/api/articles/*/stock-movements'], ARRAY['/articles'], ARRAY['vendure-stock-projection'], '{}',
     '{"kind":"database"}', 'core: stock movements',
     '{"preserve_on_disable":true,"included_in_backup":true}'),
    ('core_orders', 'core.orders', 'Commerce und Storefront', 'Aufträge', 'core', true, 'enabled', true,
     ARRAY['core.catalog', 'core.inventory'], '{}', '{"product":"Essentials+ Merchant","core_api":"1","schema_min":11}', '[]', '[]',
     ARRAY['/api/sales-orders'], ARRAY['/sales-orders'], ARRAY['vendure-order-import'], '{}',
     '{"kind":"database"}', 'core: sales orders and stock booking',
     '{"preserve_on_disable":true,"included_in_backup":true}'),
    ('commerce_vendure', 'commerce.vendure', 'Commerce und Storefront', 'Vendure Commerce', 'optional', true, 'enabled', false,
     ARRAY['core.catalog', 'core.inventory', 'core.orders'], '{}', '{"product":"Essentials+ Merchant","vendure_major":3,"schema_min":11}', '[]',
     '[{"name":"integration_hmac","mechanism":"environment_secret"}]',
     ARRAY['/api/integrations/vendure'], ARRAY['/integration-diagnostics'], ARRAY['core-outbox-delivery', 'vendure-outbox-delivery'],
     ARRAY['vendure-domain-events'], '{"kind":"signed_readiness"}', 'split: core owns ERP; Vendure owns commerce runtime',
     '{"preserve_on_disable":true,"included_in_backup":true,"stores":["core_db","vendure_db","vendure_assets"]}'),
    ('commerce_storefront', 'commerce.storefront', 'Commerce und Storefront', 'Storefront', 'optional', true, 'enabled', false,
     ARRAY['commerce.vendure'], '{}', '{"product":"Essentials+ Merchant","vendure_major":3}', '[]', '[]',
     ARRAY['/api/shop'], ARRAY['storefront'], '{}', '{}', '{"kind":"vendure_readiness"}', 'vendure: channel presentation',
     '{"preserve_on_disable":true,"included_in_backup":true}'),
    ('payment_test', 'payment.test', 'Zahlung', 'Testzahlung', 'connector', true, 'enabled', false,
     ARRAY['commerce.vendure'], '{}', '{"product":"Essentials+ Merchant","synthetic_only":true}', '[{"name":"mode","value":"synthetic"}]', '[]',
     ARRAY['/api/payments/test'], '{}', ARRAY['payment-reconciliation'], ARRAY['payment-callback'],
     '{"kind":"deterministic_fake"}', 'vendure: test payment state; core: redacted reconciliation audit',
     '{"preserve_on_disable":true,"included_in_backup":true}'),
    ('shipping_manual', 'shipping.manual', 'Versand', 'Manueller Versand', 'connector', true, 'enabled', false,
     ARRAY['core.orders'], '{}', '{"product":"Essentials+ Merchant","schema_min":11}', '[]', '[]',
     ARRAY['/api/sales-orders/*/fulfill'], ARRAY['/sales-orders'], '{}', '{}',
     '{"kind":"database"}', 'core: fulfillment and tracking state',
     '{"preserve_on_disable":true,"included_in_backup":true}'),
    ('accounting_invoices', 'accounting.invoices', 'Buchhaltung und Export', 'Rechnungen', 'optional', true, 'enabled', false,
     ARRAY['core.orders'], '{}', '{"product":"Essentials+ Merchant","schema_min":11}', '[]', '[]',
     ARRAY['/api/invoices'], ARRAY['/invoices'], '{}', '{}', '{"kind":"database_and_document_store"}',
     'core: immutable issued invoices and PDFs', '{"preserve_on_disable":true,"included_in_backup":true}'),
    ('accounting_corrections', 'accounting.corrections', 'Buchhaltung und Export', 'Korrekturrechnungen', 'optional', false, 'not_installed', false,
     ARRAY['accounting.invoices'], '{}', '{"product":"Essentials+ Merchant","schema_min":12}', '[]', '[]',
     ARRAY['/api/invoices/*/corrections'], ARRAY['/invoices'], '{}', '{}', '{"kind":"database_and_document_store"}',
     'core: immutable correction invoices and references', '{"preserve_on_disable":true,"included_in_backup":true}'),
    ('export_datev', 'export.datev', 'Buchhaltung und Export', 'DATEV-Export', 'optional', false, 'not_installed', false,
     ARRAY['accounting.invoices', 'accounting.corrections'], '{}', '{"product":"Essentials+ Merchant","mapping_status":"external_gate"}', '[]', '[]',
     ARRAY['/api/exports/datev'], ARRAY['/exports'], ARRAY['datev-export'], '{}', '{"kind":"mapping_validation"}',
     'core: immutable export batches', '{"preserve_on_disable":true,"included_in_backup":true}'),
    ('intelligence_rules', 'intelligence.rules', 'Intelligence', 'Regelbasierte Auswertung', 'optional', true, 'enabled', false,
     '{}', '{}', '{"product":"Essentials+ Merchant","external_ai":false}', '[]', '[]', '{}', '{}', '{}', '{}',
     '{"kind":"deterministic"}', 'core: aggregated metrics only',
     '{"preserve_on_disable":true,"included_in_backup":true}'),
    ('custom_catalog', 'custom.catalog', 'Kundenspezifisch', 'Kundenspezifische Module', 'optional', false, 'not_installed', false,
     '{}', '{}', '{"product":"Essentials+ Merchant"}', '[]', '[]', '{}', '{}', '{}', '{}',
     '{"kind":"none"}', 'module-defined', '{"preserve_on_disable":true,"included_in_backup":true}')
ON CONFLICT (module_key) DO NOTHING;

INSERT INTO connector_module_health (module_key, configuration_valid, health_status, message)
VALUES
    ('payment_test', true, 'healthy', 'Deterministic synthetic provider is available.'),
    ('shipping_manual', true, 'healthy', 'Manual fulfillment requires no external credentials.')
ON CONFLICT (module_key) DO UPDATE
SET configuration_valid = EXCLUDED.configuration_valid,
    health_status = EXCLUDED.health_status,
    message = EXCLUDED.message;

CREATE TABLE module_configurations (
    module_key TEXT PRIMARY KEY REFERENCES essentials_modules(module_key) ON DELETE RESTRICT,
    configuration JSONB NOT NULL DEFAULT '{}'::jsonb,
    secret_refs TEXT[] NOT NULL DEFAULT '{}',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (jsonb_typeof(configuration) = 'object')
);

CREATE INDEX idx_essentials_modules_public_catalog
    ON essentials_modules (module_group, module_id)
    WHERE catalog_visible;
