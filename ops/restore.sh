#!/bin/sh
set -eu

repository_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
project=${COMPOSE_PROJECT_NAME:?COMPOSE_PROJECT_NAME must identify a new isolated stack}
compose_env_file=${COMPOSE_ENV_FILE:-/dev/null}
backup_dir=${1:?usage: COMPOSE_PROJECT_NAME=new-name ops/restore.sh BACKUP_DIRECTORY}
backup_dir=$(CDPATH='' cd -- "$backup_dir" && pwd)

case "$project" in
    *[!A-Za-z0-9_-]*|'') echo "invalid COMPOSE_PROJECT_NAME" >&2; exit 2 ;;
esac

compose() {
    docker compose --env-file "$compose_env_file" -p "$project" "$@"
}

if [ -n "$(compose ps -aq)" ]; then
    echo "restore project already has containers: $project" >&2
    exit 2
fi
for volume in erplite_db_data erplite_invoices vendure_db_data vendure_assets; do
    if docker volume inspect "${project}_${volume}" >/dev/null 2>&1; then
        echo "restore project already has volume: ${project}_${volume}" >&2
        exit 2
    fi
done

node "$repository_dir/ops/verify-backup.mjs" "$backup_dir"

compose up -d --wait db vendure-db
compose exec -T db pg_restore -U erplite -d erplite --clean --if-exists --no-owner --no-acl --exit-on-error \
    <"$backup_dir/data/core-postgres.dump"
# Expansion is intentionally performed by the shell inside vendure-db.
# shellcheck disable=SC2016
compose exec -T vendure-db sh -c \
    'pg_restore -U "$POSTGRES_USER" -d "$POSTGRES_DB" --clean --if-exists --no-owner --no-acl --exit-on-error' \
    <"$backup_dir/data/vendure-postgres.dump"

ensure_compose_volume() {
    logical_name=$1
    if docker volume inspect "${project}_${logical_name}" >/dev/null 2>&1; then
        return
    fi
    docker volume create \
        --label "com.docker.compose.project=$project" \
        --label "com.docker.compose.volume=$logical_name" \
        "${project}_${logical_name}" >/dev/null
}

restore_volume() {
    logical_name=$1
    archive_name=$2
    # `compose up db vendure-db` may eagerly create every declared volume.
    # The initial empty-project guard makes either path safe.
    ensure_compose_volume "$logical_name"
    docker run --rm \
        -v "${project}_${logical_name}:/target" \
        -v "$backup_dir/data:/backup:ro" \
        postgres:16-alpine@sha256:cf78e76683b9ca8c5733cbbdce6c9262b45b6767934dd0a95e671f9a0fc20685 \
        tar -C /target -xzf "/backup/$archive_name"
}

restore_volume erplite_invoices core-documents.tar.gz
restore_volume vendure_assets vendure-assets.tar.gz

compose up -d --wait
echo "Backup restored into isolated project $project"
