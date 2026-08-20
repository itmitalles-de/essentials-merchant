#!/usr/bin/env bash
set -euo pipefail

readonly PROJECT_NAME="essentials-merchant-amazon"
readonly COMPOSE_FILE="compose.mantle-amazon.yml"
MODE="check"
ENV_FILE=".env.mantle-amazon"

usage() {
  echo "Usage: scripts/start-mantle-amazon.sh [--check|--start] [--env-file PATH]" >&2
}

while (($#)); do
  case "$1" in
    --check) MODE="check" ;;
    --start) MODE="start" ;;
    --env-file)
      shift
      (($#)) || { usage; exit 2; }
      ENV_FILE="$1"
      ;;
    *) usage; exit 2 ;;
  esac
  shift
done

if [[ ! -f "$COMPOSE_FILE" || ! -f "$ENV_FILE" ]]; then
  echo "Run from the repository root with a populated, private environment file." >&2
  exit 2
fi

configured_revision="$(sed -n 's/^MERCHANT_GIT_SHA=//p' "$ENV_FILE" | tail -n 1)"
repository_revision="$(git rev-parse HEAD)"
if [[ ! "$configured_revision" =~ ^[0-9a-f]{40}$ ]] || [[ "$configured_revision" != "$repository_revision" ]]; then
  echo "MERCHANT_GIT_SHA must equal the full checked-out live commit." >&2
  exit 2
fi

compose=(docker compose --project-name "$PROJECT_NAME" --env-file "$ENV_FILE" --file "$COMPOSE_FILE")
"${compose[@]}" config --quiet
mapfile -t services < <("${compose[@]}" config --services | sort)
expected=(backend db frontend)
if [[ " ${services[*]} " != " ${expected[*]} " ]]; then
  echo "Live service allowlist mismatch; refusing to continue." >&2
  exit 1
fi
for image in $("${compose[@]}" config --images); do
  case "$image" in
    essentials-merchant-backend:"$repository_revision"|essentials-merchant-frontend:"$repository_revision"|postgres:16-alpine@sha256:*) ;;
    *) echo "Image is not pinned to the live Git SHA or an upstream digest." >&2; exit 1 ;;
  esac
done

echo '{"profile":"mantle-amazon-analysis","configuration":"valid","services":["db","backend","frontend"]}'
if [[ "$MODE" == "check" ]]; then
  exit 0
fi

"${compose[@]}" up --detach --build --wait db backend frontend

mapfile -t running < <("${compose[@]}" ps --services --status running | sort)
if [[ " ${running[*]} " != " ${expected[*]} " ]]; then
  "${compose[@]}" stop backend frontend >/dev/null
  echo "Not all allowlisted live services are healthy; application services stopped." >&2
  exit 1
fi
for forbidden in vendure-db vendure-server vendure-worker storefront payment shipping datev; do
  if printf '%s\n' "${running[@]}" | grep -Fqx "$forbidden"; then
    "${compose[@]}" stop backend frontend >/dev/null
    echo "Unexpected mutating or Commerce service is running; application services stopped." >&2
    exit 1
  fi
done

status="$("${compose[@]}" exec -T db psql -U erplite -d erplite -X -A -t -c \
  "WITH active AS (
     SELECT coalesce(array_agg(module_id ORDER BY module_id), ARRAY[]::text[]) AS modules
     FROM essentials_modules WHERE enabled
   ), schedules AS (
     SELECT count(*)::bigint AS enabled FROM amazon_report_schedules WHERE enabled
   )
   SELECT jsonb_build_object(
     'profile', 'amazon-read-only',
     'compliant', active.modules = ARRAY[
       'core.catalog','core.inventory','core.operations','core.orders',
       'intelligence.rules','marketplace.amazon_intelligence','pilot.amazon_read_only'
     ]::text[] AND schedules.enabled = 0,
     'active_modules', active.modules,
     'automatic_schedules_enabled', schedules.enabled
   ) FROM active CROSS JOIN schedules;")"
if [[ "$status" != *'"compliant": true'* ]]; then
  "${compose[@]}" stop backend frontend >/dev/null
  echo "Persisted live state is not read-only compliant; application services stopped." >&2
  exit 1
fi
printf '%s\n' "$status"
