#!/bin/sh
set -eu

repository_dir=${1:?usage: ops/run-node-tool.sh REPOSITORY_DIR DATA_DIRECTORY ro|rw NODE_ARGUMENTS...}
data_dir=${2:?usage: ops/run-node-tool.sh REPOSITORY_DIR DATA_DIRECTORY ro|rw NODE_ARGUMENTS...}
data_access=${3:?usage: ops/run-node-tool.sh REPOSITORY_DIR DATA_DIRECTORY ro|rw NODE_ARGUMENTS...}
shift 3

case "$repository_dir" in /*) ;; *) echo "repository path must be absolute" >&2; exit 2 ;; esac
case "$data_dir" in /*) ;; *) echo "data path must be absolute" >&2; exit 2 ;; esac
case "$repository_dir$data_dir" in *:*) echo "paths containing colons are not supported" >&2; exit 2 ;; esac
case "$data_access" in ro|rw) ;; *) echo "data access must be ro or rw" >&2; exit 2 ;; esac
test -d "$repository_dir"
test -d "$data_dir"
test "$#" -gt 0

runtime=${MERCHANT_NODE_RUNTIME:-auto}
case "$runtime" in
  auto)
    if command -v node >/dev/null 2>&1; then
      exec node "$@"
    fi
    ;;
  host)
    command -v node >/dev/null 2>&1 || {
      echo "MERCHANT_NODE_RUNTIME=host requires node" >&2
      exit 2
    }
    exec node "$@"
    ;;
  container) ;;
  *) echo "MERCHANT_NODE_RUNTIME must be auto, host, or container" >&2; exit 2 ;;
esac

exec docker run --rm -i \
  --network none \
  --read-only \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --user "$(id -u):$(id -g)" \
  --tmpfs /tmp:rw,noexec,nosuid,nodev,size=16m \
  -v "$repository_dir:$repository_dir:ro" \
  -v "$data_dir:$data_dir:$data_access" \
  -w "$repository_dir" \
  node:22-alpine@sha256:c610fcdfb1d5b4740dd70c284ed3cb16bb857e0f7166196e36a5501df7a3aa32 \
  node "$@"
