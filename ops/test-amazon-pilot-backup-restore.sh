#!/bin/sh
set -eu

repository_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
run_id="${$}-$(date -u +%Y%m%d%H%M%S)"
source_project="amazon-pilot-backup-source-${run_id}"
restore_project="amazon-pilot-backup-restore-${run_id}"
temporary_root=$(mktemp -d)
backup_dir="$temporary_root/backup"

export POSTGRES_PASSWORD='synthetic-pilot-postgres'
export JWT_SECRET='synthetic-pilot-jwt-at-least-thirty-two-bytes'
export ADMIN_USERNAME='synthetic-admin'
export ADMIN_PASSWORD='synthetic-admin-password'
export INTEGRATION_SECRET='synthetic-pilot-integration-at-least-thirty-two-bytes'
export PILOT_FRONTEND_PORT=${PILOT_BACKUP_SOURCE_PORT:-18092}
export COMPOSE_ENV_FILE=/dev/null

compose() {
  project=$1
  shift
  docker compose --project-name "$project" --env-file /dev/null \
    --file "$repository_dir/compose.amazon-pilot.yml" "$@"
}
cleanup() {
  compose "$restore_project" down --volumes --remove-orphans >/dev/null 2>&1 || true
  compose "$source_project" down --volumes --remove-orphans >/dev/null 2>&1 || true
  rm -rf -- "$temporary_root"
}
trap cleanup EXIT HUP INT TERM

cd "$repository_dir"
compose "$source_project" up -d --build --wait db backend frontend

compose "$source_project" exec -T db psql -U erplite -d erplite -X -v ON_ERROR_STOP=1 <<'SQL'
INSERT INTO amazon_connections
  (id, seller_id, region, secret_ref, granted_roles, mode, enabled)
VALUES
  ('11111111-1111-4111-8111-111111111111', 'SYNTHETIC-SELLER', 'eu', 'fixture:pilot-backup',
   ARRAY['Brand Analytics'], 'fixture', true);
INSERT INTO amazon_marketplaces (connection_id, marketplace_id)
VALUES ('11111111-1111-4111-8111-111111111111', 'SYNTHETIC-MARKETPLACE');
INSERT INTO amazon_report_runs
  (id, connection_id, marketplace_id, report_type, data_start_time, data_end_time,
   report_options, trigger_source, idempotency_key, status, amazon_report_id,
   amazon_report_document_id, requested_at, completed_at)
VALUES
  ('22222222-2222-4222-8222-222222222222', '11111111-1111-4111-8111-111111111111',
   'SYNTHETIC-MARKETPLACE', 'GET_SALES_AND_TRAFFIC_REPORT',
   '2026-08-01T00:00:00Z', '2026-08-01T23:59:59Z',
   '{"dateGranularity":"DAY","asinGranularity":"CHILD"}', 'manual',
   'synthetic-pilot-backup-run', 'succeeded', 'synthetic-report-id',
   'synthetic-document-id', now(), now());
INSERT INTO amazon_report_run_events (run_id, status, message)
VALUES
  ('22222222-2222-4222-8222-222222222222', 'succeeded', 'Synthetic deterministic pilot fixture');
WITH payload AS (
  SELECT convert_to(repeat('{"synthetic_aggregate":true}' || chr(10), 100000), 'UTF8') AS bytes
)
INSERT INTO amazon_report_documents
  (id, run_id, amazon_report_document_id, sha256, decoded_sha256, content_type,
   raw_content, decoded_content, parser_version, import_status)
SELECT
  '33333333-3333-4333-8333-333333333333', '22222222-2222-4222-8222-222222222222',
  'synthetic-document-id', encode(digest(bytes, 'sha256'), 'hex'),
  encode(digest(bytes, 'sha256'), 'hex'), 'application/json', bytes, bytes,
  'sales-traffic-json-v2', 'parsed'
FROM payload;
INSERT INTO amazon_metric_snapshots
  (id, run_id, connection_id, marketplace_id, report_type, parser_version,
   period_start, period_end, granularity, comparability_key, summary)
VALUES
  ('44444444-4444-4444-8444-444444444444', '22222222-2222-4222-8222-222222222222',
   '11111111-1111-4111-8111-111111111111', 'SYNTHETIC-MARKETPLACE',
   'GET_SALES_AND_TRAFFIC_REPORT', 'sales-traffic-json-v2',
   '2026-08-01T00:00:00Z', '2026-08-01T23:59:59Z', 'day_child',
   'sales-traffic:day_child:1d', '{"synthetic":true,"records":100000}');
INSERT INTO amazon_normalized_metrics
  (snapshot_id, metric_name, dimension_type, dimension_key, value_numeric, unit, evidence)
VALUES
  ('44444444-4444-4444-8444-444444444444', 'sessions', 'catalog', '', 100000, 'sessions',
   '{"source":"synthetic-backup-fixture"}');
INSERT INTO amazon_analysis_jobs
  (id, run_id, connection_id, marketplace_id, report_type, analysis_type,
   period_start, period_end, status, completed_at)
VALUES
  ('55555555-5555-4555-8555-555555555555', '22222222-2222-4222-8222-222222222222',
   '11111111-1111-4111-8111-111111111111', 'SYNTHETIC-MARKETPLACE',
   'GET_SALES_AND_TRAFFIC_REPORT', 'delta', '2026-08-01T00:00:00Z',
   '2026-08-01T23:59:59Z', 'completed', now());
INSERT INTO amazon_analysis_results
  (id, job_id, strategy, prompt_version, payload_sha256, result)
VALUES
  ('66666666-6666-4666-8666-666666666666', '55555555-5555-4555-8555-555555555555',
   'deterministic_rules', 'marketplace-rules-v2', repeat('a', 64),
   '{"facts":["synthetic"],"delta":[],"trend":"stable","anomalies":[],
     "hypotheses":[],"options":[],"uncertainty":"synthetic-only",
     "missing_data":[],"evidence_refs":["snapshot:44444444-4444-4444-8444-444444444444"]}');
INSERT INTO amazon_ai_strategy_assessments
  (id, analysis_id, payload_sha256, model_name, prompt_version, result,
   provider_request_id_redacted, input_tokens, output_tokens, week_start, created_by)
SELECT
  '77777777-7777-4777-8777-777777777777',
  '66666666-6666-4666-8666-666666666666', repeat('c', 64), 'gpt-5.6',
  'mantle-amazon-weekly-strategy-v2',
  '{"executive_summary":"synthetic previous week","assessment":"synthetic previous week",
    "opportunities":[],"risks":[],"hypotheses":[],"recommended_actions":[],
    "open_questions":[],"limitations":["no real data"],
    "handover":{"continuity_summary":"synthetic previous handover","priorities_until_next_run":[],
      "evidence_for_next_run":[],"next_run_checks":[]}}',
  'fedcba987654', 90, 40, '2026-08-10', id
FROM users WHERE username = 'synthetic-admin';
INSERT INTO amazon_ai_strategy_assessments
  (id, analysis_id, payload_sha256, model_name, prompt_version, result,
   provider_request_id_redacted, input_tokens, output_tokens, week_start,
   previous_assessment_id, created_by)
SELECT
  '88888888-8888-4888-8888-888888888888',
  '66666666-6666-4666-8666-666666666666', repeat('b', 64), 'gpt-5.6',
  'mantle-amazon-weekly-strategy-v2',
  '{"executive_summary":"synthetic current week","assessment":"synthetic current week",
    "opportunities":[],"risks":[],"hypotheses":[],"recommended_actions":[],
    "open_questions":[],"limitations":["no real data"],
    "handover":{"continuity_summary":"synthetic current handover","priorities_until_next_run":[],
      "evidence_for_next_run":[],"next_run_checks":[]}}',
  '0123456789ab', 100, 50, '2026-08-17',
  '77777777-7777-4777-8777-777777777777', id
FROM users WHERE username = 'synthetic-admin';
INSERT INTO amazon_transport_observations
  (run_id, operation, request_id_redacted, rate_limit_limit, retry_after_seconds)
VALUES
  ('22222222-2222-4222-8222-222222222222', 'create_report', 'sha256:aaaaaaaaaaaa', 'synthetic', 0);
INSERT INTO administrative_audit_log
  (actor_user_id, action, target_type, target_id, idempotency_key, details)
SELECT id, 'pilot.synthetic_backup_fixture', 'pilot_profile', 'amazon-read-only',
       'pilot-synthetic-backup-fixture-v1', '{"pii":false,"synthetic":true}'
FROM users WHERE username = 'synthetic-admin';
INSERT INTO pilot_provider_secrets
  (provider, ciphertext, nonce, encryption_algorithm, key_version,
   configured_fields, updated_by)
SELECT 'openai', decode(repeat('ab', 32), 'hex'), decode(repeat('cd', 12), 'hex'),
       'AES-256-GCM-v1', 1, ARRAY['api_key'], id
FROM users WHERE username = 'synthetic-admin';
SQL

docker run --rm --user 0:0 \
  -v "${source_project}_erplite_invoices:/target" \
  postgres:16-alpine@sha256:cf78e76683b9ca8c5733cbbdce6c9262b45b6767934dd0a95e671f9a0fc20685 \
  sh -c "mkdir -p /target/amazon-pilot && printf '%s' 'synthetic pilot operations document' > /target/amazon-pilot/fixture.txt"

source_fingerprint=$(compose "$source_project" exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "SELECT concat_ws('|',
     (SELECT count(*) FROM amazon_report_runs),
     (SELECT sha256 FROM amazon_report_documents WHERE id = '33333333-3333-4333-8333-333333333333'),
     (SELECT count(*) FROM amazon_metric_snapshots),
     (SELECT count(*) FROM amazon_analysis_results),
     (SELECT count(*) FROM amazon_ai_strategy_assessments),
     (SELECT count(*) FROM administrative_audit_log),
     (SELECT count(*) FROM essentials_modules WHERE enabled))")
test "$(compose "$source_project" exec -T db psql -U erplite -d erplite -X -qAt -c \
  "SELECT octet_length(raw_content) >= 2000000 FROM amazon_report_documents WHERE id = '33333333-3333-4333-8333-333333333333'")" = t
test "$(compose "$source_project" exec -T db psql -U erplite -d erplite -X -qAt -c \
  "SELECT count(*) FROM amazon_report_documents document
   JOIN amazon_report_runs run ON run.id = document.run_id
   WHERE run.report_type <> 'GET_SALES_AND_TRAFFIC_REPORT'")" = 0
test "$(compose "$source_project" exec -T db psql -U erplite -d erplite -X -qAt -c \
  "SELECT count(*) FROM pilot_provider_secrets")" = 1

compose "$source_project" exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "INSERT INTO amazon_connections (seller_id, region, secret_ref, granted_roles, mode)
   VALUES ('SYNTHETIC-UNSAFE-REFERENCE', 'eu', 'Atzr|literal-looking-secret',
           ARRAY['Brand Analytics'], 'live')" >/dev/null
if COMPOSE_PROJECT_NAME="$source_project" "$repository_dir/ops/backup-amazon-pilot.sh" \
    "$temporary_root/refused-unsafe-backup" >/dev/null 2>&1; then
  echo "pilot backup unexpectedly accepted an unsafe live secret reference" >&2
  exit 1
fi
compose "$source_project" exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "DELETE FROM amazon_connections WHERE seller_id = 'SYNTHETIC-UNSAFE-REFERENCE'" >/dev/null

COMPOSE_PROJECT_NAME="$source_project" "$repository_dir/ops/backup-amazon-pilot.sh" "$backup_dir"
node "$repository_dir/ops/verify-amazon-pilot-backup.mjs" "$backup_dir"

export PILOT_FRONTEND_PORT=${PILOT_BACKUP_RESTORE_PORT:-18093}
COMPOSE_PROJECT_NAME="$restore_project" "$repository_dir/ops/restore-amazon-pilot.sh" "$backup_dir"

restored_fingerprint=$(compose "$restore_project" exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "SELECT concat_ws('|',
     (SELECT count(*) FROM amazon_report_runs),
     (SELECT sha256 FROM amazon_report_documents WHERE id = '33333333-3333-4333-8333-333333333333'),
     (SELECT count(*) FROM amazon_metric_snapshots),
     (SELECT count(*) FROM amazon_analysis_results),
     (SELECT count(*) FROM amazon_ai_strategy_assessments),
     (SELECT count(*) FROM administrative_audit_log),
     (SELECT count(*) FROM essentials_modules WHERE enabled))")
test "$source_fingerprint" = "$restored_fingerprint"
test "$(compose "$restore_project" exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "SELECT count(*) FROM pilot_backup_verifications WHERE outcome = 'passed'")" -ge 1
test "$(compose "$restore_project" exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "SELECT count(*) FROM amazon_report_schedules WHERE enabled")" = 0
test "$(compose "$restore_project" exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "SELECT count(*) FROM pilot_provider_secrets")" = 0
test "$(compose "$restore_project" exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "SELECT count(*) FROM amazon_ai_strategy_assessments
   WHERE week_start = DATE '2026-08-17'
     AND result ? 'handover'
     AND previous_assessment_id = '77777777-7777-4777-8777-777777777777'")" = 1
test "$(docker run --rm -v "${restore_project}_erplite_invoices:/source:ro" \
  postgres:16-alpine@sha256:cf78e76683b9ca8c5733cbbdce6c9262b45b6767934dd0a95e671f9a0fc20685 \
  cat /source/amazon-pilot/fixture.txt)" = 'synthetic pilot operations document'

echo "Amazon pilot backup/restore passed: large raw archive, hashes, snapshot, parser, deterministic/AI analyses, modules, audit, documents, credential exclusion, and fail-closed profile verified."
