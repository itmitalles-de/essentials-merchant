-- Extend the existing immutable manual-report boundary with one aggregate,
-- read-only Amazon Ads evidence type. This is not an Ads API operation and
-- grants no advertising mutation capability.

ALTER TABLE amazon_manual_report_imports
    DROP CONSTRAINT amazon_manual_report_imports_report_type_check,
    ADD CONSTRAINT amazon_manual_report_imports_report_type_check CHECK (
        report_type IN (
            'GET_SALES_AND_TRAFFIC_REPORT',
            'AMAZON_ADS_SPONSORED_PRODUCTS_CAMPAIGN_REPORT'
        )
    );

UPDATE essentials_modules
SET compatibility = jsonb_set(compatibility, '{schema_min}', '20'::jsonb),
    updated_at = now()
WHERE module_id IN ('marketplace.amazon_intelligence', 'pilot.amazon_read_only');
