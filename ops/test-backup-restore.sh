#!/bin/sh
set -eu

repository_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
run_id="${$}-$(date -u +%Y%m%d%H%M%S)"
source_project="merchant-backup-source-${run_id}"
restore_project="merchant-backup-restore-${run_id}"
temporary_root=$(mktemp -d)
backup_dir="$temporary_root/backup"
created_proxy_network=false

export POSTGRES_PASSWORD='synthetic-core-db-only'
export JWT_SECRET='synthetic-jwt-secret-at-least-thirty-two-bytes'
export ADMIN_USERNAME='synthetic-admin'
export ADMIN_PASSWORD='synthetic-admin-password'
export INTEGRATION_SECRET='synthetic-current-hmac-key-at-least-32-bytes'
export INTEGRATION_KEY_ID='current'
export INTEGRATION_PREVIOUS_KEY_ID='previous'
export INTEGRATION_PREVIOUS_SECRET='synthetic-previous-hmac-key-at-least-32-bytes'
export VENDURE_DB_PASSWORD='synthetic-vendure-db-only'
export VENDURE_SUPERADMIN_USERNAME='synthetic-superadmin'
export VENDURE_SUPERADMIN_PASSWORD='synthetic-superadmin-password'
export VENDURE_COOKIE_SECRET='synthetic-cookie-secret-at-least-thirty-two-bytes'
export APP_ENV='test'
export RUST_LOG='warn'
export COMPOSE_ENV_FILE='/dev/null'

compose() {
    project=$1
    shift
    docker compose --env-file /dev/null -p "$project" "$@"
}

cleanup() {
    compose "$restore_project" down --volumes --remove-orphans >/dev/null 2>&1 || true
    compose "$source_project" down --volumes --remove-orphans >/dev/null 2>&1 || true
    if [ "$created_proxy_network" = true ]; then
        docker network rm proxy_net >/dev/null 2>&1 || true
    fi
    rm -rf -- "$temporary_root"
}
trap cleanup EXIT HUP INT TERM

if ! docker network inspect proxy_net >/dev/null 2>&1; then
    docker network create proxy_net >/dev/null
    created_proxy_network=true
fi

cd "$repository_dir"

export COMPOSE_PROJECT_NAME="$source_project"
export FRONTEND_PORT=18090
export VENDURE_PORT=13000
export STOREFRONT_PORT=13001
compose "$source_project" up -d --build --wait

CORE_API_URL='http://127.0.0.1:18090/api' \
STOREFRONT_API_URL='http://127.0.0.1:13001/api/shop' \
CORE_ADMIN_USERNAME="$ADMIN_USERNAME" CORE_ADMIN_PASSWORD="$ADMIN_PASSWORD" \
    node commerce/test/vertical.mjs

# Prove both non-database stores survive as byte-identical archives. The data
# is deliberately synthetic and contains no customer or provider material.
docker run --rm -v "${source_project}_erplite_invoices:/target" alpine:3.20 \
    sh -c "printf '%s' 'synthetic immutable invoice fixture' > /target/backup-fixture.txt"
docker run --rm -v "${source_project}_vendure_assets:/target" alpine:3.20 \
    sh -c "printf '%s' 'synthetic vendure asset fixture' > /target/backup-fixture.txt"

source_order_count=$(compose "$source_project" exec -T db \
    psql -U erplite -d erplite -qAt -v ON_ERROR_STOP=1 \
    -c "SELECT count(*) FROM sales_orders WHERE source = 'vendure'")
source_mapping_count=$(compose "$source_project" exec -T db \
    psql -U erplite -d erplite -qAt -v ON_ERROR_STOP=1 \
    -c "SELECT count(*) FROM external_entity_mappings")
source_inbox_count=$(compose "$source_project" exec -T db \
    psql -U erplite -d erplite -qAt -v ON_ERROR_STOP=1 \
    -c "SELECT count(*) FROM integration_inbox")

COMPOSE_PROJECT_NAME="$source_project" "$repository_dir/ops/backup.sh" "$backup_dir"
node "$repository_dir/ops/verify-backup.mjs" "$backup_dir"

export COMPOSE_PROJECT_NAME="$restore_project"
export FRONTEND_PORT=18091
export VENDURE_PORT=13010
export STOREFRONT_PORT=13011
"$repository_dir/ops/restore.sh" "$backup_dir"

test "$source_order_count" = "$(compose "$restore_project" exec -T db \
    psql -U erplite -d erplite -qAt -v ON_ERROR_STOP=1 \
    -c "SELECT count(*) FROM sales_orders WHERE source = 'vendure'")"
test "$source_mapping_count" = "$(compose "$restore_project" exec -T db \
    psql -U erplite -d erplite -qAt -v ON_ERROR_STOP=1 \
    -c "SELECT count(*) FROM external_entity_mappings")"
test "$source_inbox_count" = "$(compose "$restore_project" exec -T db \
    psql -U erplite -d erplite -qAt -v ON_ERROR_STOP=1 \
    -c "SELECT count(*) FROM integration_inbox")"

test "$(docker run --rm -v "${restore_project}_erplite_invoices:/source:ro" alpine:3.20 \
    cat /source/backup-fixture.txt)" = 'synthetic immutable invoice fixture'
test "$(docker run --rm -v "${restore_project}_vendure_assets:/source:ro" alpine:3.20 \
    cat /source/backup-fixture.txt)" = 'synthetic vendure asset fixture'

# The restored stack must still complete the entire SKU-to-fulfillment path.
CORE_API_URL='http://127.0.0.1:18091/api' \
STOREFRONT_API_URL='http://127.0.0.1:13011/api/shop' \
CORE_ADMIN_USERNAME="$ADMIN_USERNAME" CORE_ADMIN_PASSWORD="$ADMIN_PASSWORD" \
    node commerce/test/vertical.mjs

echo "Backup/restore rehearsal passed: checksums, both databases, document stores and vertical flow verified."
