#!/bin/sh
set -eu

mode=check
env_file=.env.amazon-pilot
gate_file=.amazon-staging-gate.json
output_file=.amazon-staging-result.json

usage() {
  echo "Usage: scripts/request-amazon-staging-report.sh [--check|--execute] [--env-file PATH] [--gate-file PATH] [--output PATH]" >&2
}
while [ "$#" -gt 0 ]; do
  case "$1" in
    --check) mode=check ;;
    --execute) mode=execute ;;
    --env-file) shift; [ "$#" -gt 0 ] || { usage; exit 2; }; env_file=$1 ;;
    --gate-file) shift; [ "$#" -gt 0 ] || { usage; exit 2; }; gate_file=$1 ;;
    --output) shift; [ "$#" -gt 0 ] || { usage; exit 2; }; output_file=$1 ;;
    *) usage; exit 2 ;;
  esac
  shift
done

exec node scripts/request-amazon-staging-report.mjs \
  --mode "$mode" --env-file "$env_file" --gate-file "$gate_file" --output "$output_file"
