#!/bin/sh
set -eu

repository_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
project=${COMPOSE_PROJECT_NAME:?COMPOSE_PROJECT_NAME must identify the exact running pilot stack}
compose_env_file=${COMPOSE_ENV_FILE:-.env.amazon-pilot}
output_dir=${1:?usage: COMPOSE_PROJECT_NAME=name ops/backup-amazon-pilot.sh OUTPUT_DIRECTORY}
compose_file="$repository_dir/compose.amazon-pilot.yml"

case "$project" in *[!A-Za-z0-9_-]*|'') echo "invalid COMPOSE_PROJECT_NAME" >&2; exit 2 ;; esac
if [ -e "$output_dir" ]; then echo "backup target already exists: $output_dir" >&2; exit 2; fi
mkdir -p "$output_dir/data"
output_dir=$(CDPATH='' cd -- "$output_dir" && pwd)

compose() {
  docker compose --project-name "$project" --env-file "$compose_env_file" --file "$compose_file" "$@"
}

configured=$(compose config --services | sort | tr '\n' ' ')
running=$(compose ps --services --status running | sort | tr '\n' ' ')
if [ "$configured" != "backend db frontend " ] || [ "$running" != "backend db frontend " ]; then
  echo "pilot backup requires exactly db, backend, and frontend" >&2
  exit 2
fi

unsafe_secret_refs=$(compose exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "SELECT count(*) FROM amazon_connections
   WHERE mode = 'live' AND secret_ref !~ '^[a-z][a-z0-9_]{0,63}$'")
potential_pii_archives=$(compose exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "SELECT count(*) FROM amazon_report_documents document
   JOIN amazon_report_runs run ON run.id = document.run_id
   WHERE run.report_type <> 'GET_SALES_AND_TRAFFIC_REPORT'")
if [ "$unsafe_secret_refs" != 0 ] || [ "$potential_pii_archives" != 0 ]; then
  echo "pilot backup refused: unsafe secret reference or non-pilot raw archive detected" >&2
  exit 2
fi

resume() { compose start backend frontend >/dev/null 2>&1 || true; }
trap resume EXIT HUP INT TERM
compose stop frontend backend >/dev/null

compose exec -T db pg_dump -U erplite -d erplite --schema-only --format=custom --no-owner --no-acl \
  >"$output_dir/data/core-schema.dump"
compose exec -T db pg_dump -U erplite -d erplite --data-only --format=custom --no-owner --no-acl \
  --table=_sqlx_migrations \
  --table=users \
  --table=essentials_modules \
  --table=user_module_permissions \
  --table=connector_module_health \
  --table=administrative_audit_log \
  --table='amazon_*' \
  --table=pilot_backup_verifications \
  >"$output_dir/data/pilot-core-data.dump"

schema_version=$(compose exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 \
  -c 'SELECT COALESCE(max(version), 0) FROM _sqlx_migrations')
compose exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "SELECT jsonb_pretty(jsonb_build_object(
     'declared', ARRAY['sales-traffic-json-v2', 'inventory-planning-tsv-v1'],
     'stored', COALESCE((SELECT jsonb_agg(version ORDER BY version)
                         FROM (SELECT DISTINCT parser_version AS version
                               FROM amazon_report_documents WHERE parser_version IS NOT NULL) versions), '[]'::jsonb)
   ))" >"$output_dir/data/parser-versions.json"

docker run --rm \
  -v "${project}_erplite_invoices:/source:ro" \
  -v "$output_dir/data:/backup" \
  postgres:16-alpine@sha256:cf78e76683b9ca8c5733cbbdce6c9262b45b6767934dd0a95e671f9a0fc20685 \
  sh -c 'if [ -d /source/amazon-pilot ]; then tar -C /source -czf /backup/pilot-documents.tar.gz amazon-pilot; else tar -C /tmp -czf /backup/pilot-documents.tar.gz --files-from /dev/null; fi'

compose config --format json | node "$repository_dir/ops/redact-compose.mjs" \
  >"$output_dir/data/compose-metadata.json"
: >"$output_dir/data/runtime-image-digests.tsv"
for service in db backend frontend; do
  container_id=$(compose ps -aq "$service" | head -n 1)
  image_id=$(docker inspect --format '{{.Image}}' "$container_id")
  printf '%s\t%s\n' "$service" "$image_id" >>"$output_dir/data/runtime-image-digests.tsv"
done

revision=$(git -C "$repository_dir" rev-parse HEAD)
node "$repository_dir/ops/amazon-pilot-backup-manifest.mjs" "$output_dir" "$revision" "$schema_version"
node "$repository_dir/ops/verify-amazon-pilot-backup.mjs" "$output_dir"
manifest_sha=$(sha256sum "$output_dir/manifest.json" | cut -d ' ' -f 1)
compose exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "INSERT INTO pilot_backup_verifications
     (profile, outcome, manifest_sha256, repository_revision, details)
   VALUES ('amazon-read-only', 'passed', '$manifest_sha', '$revision',
           jsonb_build_object('kind', 'pilot-backup', 'contains_secrets', false))" >/dev/null

trap - EXIT HUP INT TERM
resume
echo "Amazon pilot backup created and verified at $output_dir"
