#!/bin/sh
set -eu
umask 077

repository_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
project=${COMPOSE_PROJECT_NAME:?COMPOSE_PROJECT_NAME must identify a new isolated pilot stack}
compose_env_file=${COMPOSE_ENV_FILE:-.env.amazon-pilot}
backup_dir=${1:?usage: COMPOSE_PROJECT_NAME=new-name ops/restore-amazon-pilot.sh BACKUP_DIRECTORY}
backup_dir=$(CDPATH='' cd -- "$backup_dir" && pwd)
compose_file=${PILOT_COMPOSE_FILE:-$repository_dir/compose.amazon-pilot.yml}
case "$compose_file" in /*) ;; *) compose_file="$repository_dir/$compose_file" ;; esac
if [ -n "${RESTORE_FRONTEND_PORT:-}" ]; then
  export PILOT_FRONTEND_PORT="$RESTORE_FRONTEND_PORT"
  export MANTLE_AMAZON_FRONTEND_PORT="$RESTORE_FRONTEND_PORT"
fi

case "$project" in *[!A-Za-z0-9_-]*|'') echo "invalid COMPOSE_PROJECT_NAME" >&2; exit 2 ;; esac
# A restored frontend may share the host's external proxy network when the live
# Compose file is used. Give it a project-specific alias so the live Caddy
# upstream can never resolve to the isolated restore acceptance stack.
export MANTLE_AMAZON_PROXY_ALIAS="${project}-frontend"
compose() {
  docker compose --project-name "$project" --env-file "$compose_env_file" --file "$compose_file" "$@"
}
if [ -n "$(compose ps -aq)" ]; then echo "restore project already has containers: $project" >&2; exit 2; fi
for volume in erplite_db_data erplite_invoices; do
  if docker volume inspect "${project}_${volume}" >/dev/null 2>&1; then
    echo "restore project already has volume: ${project}_${volume}" >&2
    exit 2
  fi
done

node "$repository_dir/ops/verify-amazon-pilot-backup.mjs" "$backup_dir"
compose up -d --wait db
compose exec -T db pg_restore -U erplite -d erplite --no-owner --no-acl --exit-on-error \
  <"$backup_dir/data/core-schema.dump"
compose exec -T db pg_restore -U erplite -d erplite --data-only --no-owner --no-acl --exit-on-error \
  <"$backup_dir/data/pilot-core-data.dump"

if ! docker volume inspect "${project}_erplite_invoices" >/dev/null 2>&1; then
  docker volume create \
    --label "com.docker.compose.project=$project" \
    --label "com.docker.compose.volume=erplite_invoices" \
    "${project}_erplite_invoices" >/dev/null
fi
docker run --rm \
  -v "${project}_erplite_invoices:/target" \
  -v "$backup_dir/data:/backup:ro" \
  postgres:16-alpine@sha256:cf78e76683b9ca8c5733cbbdce6c9262b45b6767934dd0a95e671f9a0fc20685 \
  tar -C /target -xzf /backup/pilot-documents.tar.gz

compose up -d --wait backend frontend
manifest_sha=$(sha256sum "$backup_dir/manifest.json" | cut -d ' ' -f 1)
revision=$(node -e 'const m=require(process.argv[1]); process.stdout.write(m.repository_revision)' "$backup_dir/manifest.json")
compose exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "INSERT INTO pilot_backup_verifications
     (profile, outcome, manifest_sha256, repository_revision, details)
   VALUES ('amazon-read-only', 'passed', '$manifest_sha', '$revision',
           jsonb_build_object('kind', 'pilot-restore', 'empty_target', true, 'contains_secrets', false))" >/dev/null

status=$(compose exec -T db psql -U erplite -d erplite -X -qAt -v ON_ERROR_STOP=1 -c \
  "WITH active AS (SELECT array_agg(module_id ORDER BY module_id) modules FROM essentials_modules WHERE enabled)
   SELECT jsonb_build_object(
     'profile', 'amazon-read-only',
     'compliant', modules = ARRAY['core.catalog','core.inventory','core.operations','core.orders',
       'intelligence.rules','marketplace.amazon_intelligence','pilot.amazon_read_only']::text[],
     'reports', (SELECT count(*) FROM amazon_report_runs),
     'raw_archives', (SELECT count(*) FROM amazon_report_documents),
     'snapshots', (SELECT count(*) FROM amazon_metric_snapshots),
     'analyses', (SELECT count(*) FROM amazon_analysis_results),
     'automatic_schedules', (SELECT count(*) FROM amazon_report_schedules WHERE enabled)
   ) FROM active")
case "$status" in
  *'"compliant": true'*) ;;
  *)
    compose stop backend frontend >/dev/null
    echo "restored pilot is not fail-closed; application services stopped" >&2
    exit 1
    ;;
esac
case "$status" in
  *'"automatic_schedules": 0'*) ;;
  *)
    compose stop backend frontend >/dev/null
    echo "restored pilot unexpectedly enabled an automatic schedule; application services stopped" >&2
    exit 1
    ;;
esac
printf '%s\n' "$status"
