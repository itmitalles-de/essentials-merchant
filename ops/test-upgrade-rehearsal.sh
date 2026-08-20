#!/bin/sh
set -eu

repository_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
container="merchant-upgrade-rehearsal-${$}"
database_url=''

cleanup() {
    docker stop "$container" >/dev/null 2>&1 || true
}
trap cleanup EXIT HUP INT TERM

docker run --rm -d --name "$container" \
    -e POSTGRES_USER=merchant_upgrade \
    -e POSTGRES_PASSWORD=synthetic-upgrade-only \
    -e POSTGRES_DB=merchant_upgrade \
    -p 127.0.0.1::5432 postgres:16-alpine@sha256:cf78e76683b9ca8c5733cbbdce6c9262b45b6767934dd0a95e671f9a0fc20685 >/dev/null

attempt=0
until docker logs "$container" 2>&1 \
    | grep -Fq 'PostgreSQL init process complete; ready for start up.'; do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 100 ]; then
        echo 'upgrade rehearsal database initialization did not complete' >&2
        exit 1
    fi
    sleep 0.2
done

attempt=0
until docker exec "$container" pg_isready -U merchant_upgrade -d merchant_upgrade >/dev/null 2>&1; do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 100 ]; then
        echo 'upgrade rehearsal database did not become ready after initialization' >&2
        exit 1
    fi
    sleep 0.2
done

port=$(docker port "$container" 5432/tcp | sed 's/.*://')
database_url="postgres://merchant_upgrade:synthetic-upgrade-only@127.0.0.1:${port}/merchant_upgrade"

cd "$repository_dir/backend"
cargo sqlx migrate run --no-dotenv --database-url "$database_url" \
    --source crates/db/migrations --target-version 10

docker exec -i "$container" psql -U merchant_upgrade -d merchant_upgrade \
    -v ON_ERROR_STOP=1 <<'SQL'
INSERT INTO customers (customer_number, name, email)
VALUES (900001, 'Synthetic Upgrade Customer', 'upgrade@example.test');

INSERT INTO invoices (
    invoice_number, customer_id, status, issue_date, due_date,
    customer_snapshot, company_snapshot, net_total, vat_total, gross_total, sent_at
)
SELECT 'RE-UPGRADE-1', id, 'sent', DATE '2026-01-01', DATE '2026-01-15',
       '{"name":"Synthetic Upgrade Customer"}', '{"company_name":"Synthetic Merchant"}',
       100.00, 19.00, 119.00, TIMESTAMPTZ '2026-01-01 12:00:00+00'
FROM customers WHERE customer_number = 900001;

INSERT INTO invoice_line_items (
    invoice_id, position, description, quantity, unit, unit_price_net,
    vat_rate_code, vat_rate_percent, net_amount, vat_amount, gross_amount
)
SELECT id, 1, 'Synthetic migration fixture', 1, 'Stk', 100.00,
       'STANDARD', 19.00, 100.00, 19.00, 119.00
FROM invoices WHERE invoice_number = 'RE-UPGRADE-1';

WITH connection AS (
    INSERT INTO amazon_connections (seller_id, region, secret_ref, granted_roles, mode)
    VALUES ('SYNTHETIC-SELLER', 'eu', 'fixture:upgrade', ARRAY['Brand Analytics'], 'fixture')
    RETURNING id
), report_run AS (
    INSERT INTO amazon_report_runs (
        connection_id, marketplace_id, report_type, trigger_source,
        idempotency_key, status, amazon_report_document_id, completed_at
    )
    SELECT id, 'A1PA6795UKMFR9', 'GET_SALES_AND_TRAFFIC_REPORT', 'manual',
           'synthetic-upgrade-run', 'succeeded', 'synthetic-document', now()
    FROM connection RETURNING id
)
INSERT INTO amazon_report_documents (
    run_id, amazon_report_document_id, sha256, content_type, raw_content,
    parser_version, import_status
)
SELECT id, 'synthetic-document',
       encode(digest(convert_to('synthetic raw report', 'UTF8'), 'sha256'), 'hex'),
       'application/json', convert_to('synthetic raw report', 'UTF8'), 'sales-traffic-json-v1', 'parsed'
FROM report_run;

UPDATE essentials_modules SET enabled = false WHERE module_key = 'marketplace_intelligence';
SQL

cargo sqlx migrate run --no-dotenv --database-url "$database_url" \
    --source crates/db/migrations

docker exec -i "$container" psql -U merchant_upgrade -d merchant_upgrade \
    -qAt -v ON_ERROR_STOP=1 <<'SQL' | grep -qx 'upgrade-rehearsal-ok'
DO $$
BEGIN
    IF (SELECT count(*) FROM invoices WHERE invoice_number = 'RE-UPGRADE-1'
            AND document_type = 'invoice' AND gross_total = 119.00) <> 1 THEN
        RAISE EXCEPTION 'issued invoice did not survive migration';
    END IF;
    IF (SELECT count(*) FROM invoice_line_items item
            JOIN invoices invoice ON invoice.id = item.invoice_id
            WHERE invoice.invoice_number = 'RE-UPGRADE-1' AND item.gross_amount = 119.00) <> 1 THEN
        RAISE EXCEPTION 'invoice line did not survive migration';
    END IF;
    IF (SELECT count(*) FROM essentials_modules
            WHERE module_key = 'marketplace_intelligence'
              AND module_id = 'marketplace.amazon_intelligence'
              AND state = 'disabled') <> 1 THEN
        RAISE EXCEPTION 'module compatibility alias/state was not preserved';
    END IF;
    IF (SELECT count(*) FROM amazon_report_documents
            WHERE raw_content = decoded_content AND sha256 = decoded_sha256) <> 1 THEN
        RAISE EXCEPTION 'marketplace raw archive was not migrated losslessly';
    END IF;
    IF (SELECT max(version) FROM _sqlx_migrations) <> 22 THEN
        RAISE EXCEPTION 'unexpected final migration version';
    END IF;
    IF to_regclass('public.amazon_ai_strategy_assessments') IS NULL THEN
        RAISE EXCEPTION 'AI strategy assessment store was not created';
    END IF;
    IF to_regclass('public.uq_amazon_ai_strategy_assessments_week') IS NULL THEN
        RAISE EXCEPTION 'weekly AI strategy uniqueness boundary was not created';
    END IF;
    IF to_regclass('public.pilot_provider_secrets') IS NULL THEN
        RAISE EXCEPTION 'write-only provider credential store was not created';
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'amazon_manual_report_imports_report_type_check'
          AND pg_get_constraintdef(oid) LIKE '%AMAZON_ADS_SPONSORED_PRODUCTS_CAMPAIGN_REPORT%'
    ) THEN
        RAISE EXCEPTION 'manual Ads report boundary was not created';
    END IF;
    IF to_regclass('public.mantle_business_knowledge') IS NULL THEN
        RAISE EXCEPTION 'immutable business-knowledge store was not created';
    END IF;
    IF to_regclass('public.amazon_product_mapping_revisions') IS NULL THEN
        RAISE EXCEPTION 'append-only product-mapping store was not created';
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_trigger
        WHERE tgname = 'trg_prevent_amazon_product_mapping_revision_mutation'
          AND tgrelid = 'amazon_product_mapping_revisions'::regclass
          AND NOT tgisinternal
    ) THEN
        RAISE EXCEPTION 'product-mapping append-only trigger was not created';
    END IF;
END $$;
SELECT 'upgrade-rehearsal-ok';
SQL

echo 'Upgrade rehearsal passed: v10 synthetic data migrated losslessly through schema v22.'
