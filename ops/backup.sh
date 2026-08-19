#!/bin/sh
set -eu

repository_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
project=${COMPOSE_PROJECT_NAME:?COMPOSE_PROJECT_NAME must identify the running disposable or production stack}
compose_env_file=${COMPOSE_ENV_FILE:-/dev/null}
output_dir=${1:?usage: COMPOSE_PROJECT_NAME=name ops/backup.sh OUTPUT_DIRECTORY}

case "$project" in
    *[!A-Za-z0-9_-]*|'') echo "invalid COMPOSE_PROJECT_NAME" >&2; exit 2 ;;
esac

if [ -e "$output_dir" ]; then
    echo "backup target already exists: $output_dir" >&2
    exit 2
fi
mkdir -p "$output_dir/data"
output_dir=$(CDPATH='' cd -- "$output_dir" && pwd)

compose() {
    docker compose --env-file "$compose_env_file" -p "$project" "$@"
}

for service in db backend vendure-db vendure-server vendure-worker; do
    if [ -z "$(compose ps -q "$service")" ]; then
        echo "required service is not running: $service" >&2
        exit 2
    fi
done

resume_services() {
    compose start backend vendure-server vendure-worker storefront frontend >/dev/null 2>&1 || true
}
trap resume_services EXIT HUP INT TERM

# Quiescing both writers gives one coordinated application checkpoint across
# the separate Core and Vendure stores. The databases remain available for the
# two logical dumps.
compose stop frontend storefront vendure-worker vendure-server backend >/dev/null

compose exec -T db pg_dump -U erplite -d erplite --format=custom --no-owner --no-acl \
    >"$output_dir/data/core-postgres.dump"
# Expansion is intentionally performed by the shell inside vendure-db.
# shellcheck disable=SC2016
compose exec -T vendure-db sh -c 'pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" --format=custom --no-owner --no-acl' \
    >"$output_dir/data/vendure-postgres.dump"

core_schema=$(compose exec -T db psql -U erplite -d erplite -qAt -v ON_ERROR_STOP=1 \
    -c 'SELECT COALESCE(max(version), 0) FROM _sqlx_migrations')
# Expansion is intentionally performed by the shell inside vendure-db.
# shellcheck disable=SC2016
vendure_schema=$(compose exec -T vendure-db sh -c \
    'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -qAt -v ON_ERROR_STOP=1 -c '\''SELECT COALESCE(max("timestamp"), 0) FROM migrations'\''')

compose exec -T db psql -U erplite -d erplite -qAt -v ON_ERROR_STOP=1 -c \
    "SELECT COALESCE(jsonb_pretty(jsonb_agg(jsonb_build_object(
        'module_id', module.module_id,
        'state', module.state,
        'configuration', COALESCE(config.configuration, '{}'::jsonb)
    ) ORDER BY module.module_id)), '[]')
    FROM essentials_modules module
    LEFT JOIN module_configurations config USING (module_key)
    WHERE module.catalog_visible" \
    >"$output_dir/data/module-configurations.json"

compose exec -T db psql -U erplite -d erplite -qAt -v ON_ERROR_STOP=1 -c \
    "SELECT jsonb_pretty(jsonb_build_object(
        'declared', ARRAY['sales-traffic-json-v2', 'inventory-planning-tsv-v1'],
        'stored', COALESCE((SELECT jsonb_agg(version ORDER BY version)
                            FROM (SELECT DISTINCT parser_version AS version
                                  FROM amazon_report_documents
                                  WHERE parser_version IS NOT NULL) versions), '[]'::jsonb)
    ))" >"$output_dir/data/parser-versions.json"

: >"$output_dir/data/runtime-image-digests.tsv"
for service in db backend frontend vendure-db vendure-server vendure-worker storefront; do
    container_id=$(compose ps -aq "$service" | head -n 1)
    if [ -n "$container_id" ]; then
        image_id=$(docker inspect --format '{{.Image}}' "$container_id")
        printf '%s\t%s\n' "$service" "$image_id" >>"$output_dir/data/runtime-image-digests.tsv"
    fi
done

archive_volume() {
    volume_name=$1
    archive_name=$2
    docker volume inspect "$volume_name" >/dev/null
    docker run --rm \
        -v "$volume_name:/source:ro" \
        -v "$output_dir/data:/backup" \
        postgres:16-alpine@sha256:cf78e76683b9ca8c5733cbbdce6c9262b45b6767934dd0a95e671f9a0fc20685 \
        tar -C /source -czf "/backup/$archive_name" .
}

archive_volume "${project}_erplite_invoices" core-documents.tar.gz
archive_volume "${project}_vendure_assets" vendure-assets.tar.gz

compose config --format json | node "$repository_dir/ops/redact-compose.mjs" \
    >"$output_dir/data/compose-metadata.json"

revision=$(git -C "$repository_dir" rev-parse HEAD)
node "$repository_dir/ops/backup-manifest.mjs" \
    "$output_dir" "$revision" "$core_schema" "$vendure_schema"

trap - EXIT HUP INT TERM
resume_services
echo "Backup created and verified at $output_dir"
