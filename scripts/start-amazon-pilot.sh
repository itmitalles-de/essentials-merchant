#!/usr/bin/env bash
set -euo pipefail

readonly PROJECT_NAME="essentials-merchant-amazon-pilot"
readonly COMPOSE_FILE="compose.amazon-pilot.yml"
MODE="check"
ENV_FILE=".env.amazon-pilot"

usage() {
  echo "Usage: scripts/start-amazon-pilot.sh [--check|--start] [--env-file PATH]" >&2
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

if [[ ! -f "$COMPOSE_FILE" ]]; then
  echo "Run this script from the repository root." >&2
  exit 2
fi
if [[ ! -f "$ENV_FILE" ]]; then
  echo "Pilot environment file is missing. Copy .env.amazon-pilot.example locally and fill required values." >&2
  exit 2
fi

compose=(docker compose --project-name "$PROJECT_NAME" --env-file "$ENV_FILE" --file "$COMPOSE_FILE")

# Quiet validation deliberately avoids rendering environment values.
"${compose[@]}" config --quiet
mapfile -t services < <("${compose[@]}" config --services)
expected=(db backend frontend)
if [[ " ${services[*]} " != " ${expected[*]} " ]]; then
  echo "Pilot service allowlist mismatch; refusing to continue." >&2
  exit 1
fi

echo '{"profile":"amazon-read-only","configuration":"valid","services":["db","backend","frontend"]}'
if [[ "$MODE" == "check" ]]; then
  exit 0
fi

"${compose[@]}" up --detach --build db backend frontend

mapfile -t running < <("${compose[@]}" ps --services --status running)
for forbidden in vendure-db vendure-server vendure-worker storefront payment shipping datev; do
  if printf '%s\n' "${running[@]}" | grep -Fqx "$forbidden"; then
    "${compose[@]}" stop backend frontend >/dev/null
    echo "Unexpected mutating or Commerce service is running; pilot stopped fail-closed." >&2
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
  echo "Persisted pilot state is not compliant; application services stopped fail-closed." >&2
  exit 1
fi
printf '%s\n' "$status"
