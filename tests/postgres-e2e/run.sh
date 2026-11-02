#!/usr/bin/env bash
set -euo pipefail

function usage() {
  echo "usage: run.sh --local | --installed <sql-fun> <sql-fun-server>" >&2
}

fixture_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$fixture_dir/../.." && pwd)
case "${1:-}" in
  --local)
    (( $# == 1 )) || { usage; exit 2; }
    ;;
  --installed)
    (( $# == 3 )) || { usage; exit 2; }
    cli_binary=$(realpath "$2")
    server_binary=$(realpath "$3")
    test -x "$cli_binary" && test -x "$server_binary"
    ;;
  *) usage; exit 2 ;;
esac
for prerequisite in docker psql cargo cargo-expand; do
  command -v "$prerequisite" >/dev/null || {
    echo "$prerequisite is required; see tests/postgres-e2e/README.md" >&2
    exit 1
  }
done
docker info >/dev/null

cd "$repo_root"
export SQL_FUN_HOME="$repo_root/sql-fun-home"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo_root/target}"
CARGO_TARGET_DIR=$(realpath -m "$CARGO_TARGET_DIR")
if [[ "$1" == --local ]]; then
  cargo build --locked -p sql-fun-cli -p sql-fun-server
  cli_binary="$CARGO_TARGET_DIR/debug/sql-fun"
  server_binary="$CARGO_TARGET_DIR/debug/sql-fun-server"
fi

run_dir=$(mktemp -d /tmp/sql-fun-postgres-e2e.XXXXXXXX)
server_pid=""
lockfile="$fixture_dir/default.${run_dir##*/}.lock"
inventory="$fixture_dir/sql-fun.default.${run_dir##*/}.toml"
function cleanup() {
  local status=$? running_pid
  trap - EXIT
  if (( status != 0 )) && [[ -f "$run_dir/server.log" ]]; then
    cat "$run_dir/server.log" >&2
  fi
  for running_pid in $(jobs -pr); do
    if [[ "$running_pid" == "$server_pid" ]]; then
      kill -KILL "$running_pid" 2>/dev/null || true
    fi
  done
  if [[ -n "$server_pid" ]]; then
    wait "$server_pid" 2>/dev/null || true
  fi
  if [[ -s "$run_dir/container-id" ]]; then
    if ! docker rm --force "$(cat "$run_dir/container-id")" >/dev/null; then
      echo "Failed to remove the E2E container recorded in $run_dir/container-id" >&2
      exit 1
    fi
  fi
  rm -f -- "$lockfile" "$inventory"
  rm -rf -- "$run_dir"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# Each container owns its entire database; no developer database is inspected.
docker run --detach --rm --cidfile "$run_dir/container-id" \
  --publish 127.0.0.1::5432 \
  --env POSTGRES_USER=e2e --env POSTGRES_PASSWORD=sql-fun-e2e \
  --env POSTGRES_DB=sql_fun_e2e \
  --tmpfs /var/lib/postgresql postgres:18 >/dev/null
endpoint=$(docker port "$(cat "$run_dir/container-id")" 5432/tcp)
export SQL_FUN_E2E_DATABASE_URL="host=127.0.0.1 port=${endpoint##*:} user=e2e dbname=sql_fun_e2e password=sql-fun-e2e connect_timeout=2"
ready=0
for ((attempt = 0; attempt < 60; attempt++)); do
  if psql -X -w "$SQL_FUN_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 -c 'SELECT 1' >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 1
done
if (( ready == 0 )); then
  docker logs "$(cat "$run_dir/container-id")" >&2
  echo "PostgreSQL did not become ready" >&2
  exit 1
fi
psql -X -w "$SQL_FUN_E2E_DATABASE_URL" -v ON_ERROR_STOP=1 -f "$fixture_dir/schema.sql"

export SQL_FUN_HOME="$run_dir/home"
mkdir -p "$SQL_FUN_HOME/postgres" "$SQL_FUN_HOME/highlighter"
cp -R "$repo_root/sql-fun-home/postgres/18" "$SQL_FUN_HOME/postgres/18"
# Preserve the canonical post-init fixture's relative home layout.
cp -R "$repo_root/sql-fun-home/highlighter/." "$SQL_FUN_HOME/highlighter/"
export USER="${run_dir##*/}"
export SQL_FUN_SERVER="$server_binary"
export SQL_FUN_METADATA_FILE="$fixture_dir/Cargo.toml"
export SQL_FUN_SCHEMA_FILE="$fixture_dir/schema.sql"
export SQL_FUN_SCHEMA="${run_dir##*/}"
unset SQL_FUN_SERVER_COMMAND SQL_FUN_MODE SQL_FUN_DATASOURCE SQL_FUN_DIALECT
unset SQL_FUN_METADATA_FORMAT SQL_FUN_CTE_CATALOG_DIR SQL_FUN_PG_SEARCH_PATH
unset SQL_FUN_POSTGRES_VERSION SQL_FUN_EXTENSIONS SQL_FUN_BUILTIN_SCHEMA SQL_FUN_EXTENSIONS_DIR
unset SQL_FUN_LOG_FILE SQL_FUN_LOG_LEVEL SQL_FUN_HIGHLIGHTER_THEME SQL_FUN_NO_BUILTIN
unset SQL_FUN_OUTPUT_FORMAT SQL_FUN_TERM_COLOR

# Own the server child so cleanup never signals a developer's server.
"$server_binary" --server-idle-timeout 0 </dev/null >"$run_dir/server.log" 2>&1 &
server_pid=$!
for ((attempt = 0; attempt < 200; attempt++)); do
  if [[ -s "$run_dir/sql-fun-server.pid" ]]; then
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    cat "$run_dir/server.log" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ ! -s "$run_dir/sql-fun-server.pid" ]]; then
  cat "$run_dir/server.log" >&2
  echo "E2E server did not become ready" >&2
  exit 1
fi

cd "$fixture_dir"
"$cli_binary" check --bin sql-fun-postgres-e2e
test -s "$lockfile"
test -s "$inventory"
cp "$inventory" "$run_dir/first-inventory.toml"
"$cli_binary" check --bin sql-fun-postgres-e2e --validate-only
cmp "$inventory" "$run_dir/first-inventory.toml"
"$cli_binary" check --bin sql-fun-postgres-e2e
cmp "$inventory" "$run_dir/first-inventory.toml"
export SQL_FUN_E2E_INVENTORY="$inventory"
# Macro inputs include this run's lockfile and are not Cargo fingerprint inputs.
cargo clean --package sql-fun-postgres-e2e
cargo run --bin sql-fun-postgres-e2e
