#!/usr/bin/env bash

# Own one server child for the suite in a fresh namespace, independently of
# developer server settings. Keep cleanup in Scrut's parent shell.
set -euo pipefail

if (( $# != 1 )); then
  echo "usage: run.sh <scrut-test-directory>" >&2
  exit 2
fi
: "${SQL_FUN_BIN:?SQL_FUN_BIN must name the sql-fun executable}"
: "${SQL_FUN_TEST_SERVER_BIN:?SQL_FUN_TEST_SERVER_BIN must name the server executable}"
test -x "$SQL_FUN_BIN" && test -x "$SQL_FUN_TEST_SERVER_BIN" || exit 1

SQL_FUN_API_FIXTURE=$(mktemp -d /tmp/sql-fun-api-scrut.XXXXXXXX) || exit 1
export SQL_FUN_API_FIXTURE
export USER="${SQL_FUN_API_FIXTURE##*/}"
export SQL_FUN_SERVER="$SQL_FUN_TEST_SERVER_BIN"
unset SQL_FUN_SERVER_COMMAND SQL_FUN_METADATA_FILE RUST_LOG

# A small deterministic catalog exercises initialization without downloading or
# parsing the developer's builtin installation. Only fixture-owned files are used.
api_fixture_source=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/fixtures" && pwd)
mkdir -p "$SQL_FUN_API_FIXTURE/home/postgres/17"
cp "$api_fixture_source/schema.sql" "$SQL_FUN_API_FIXTURE/schema.sql"
cp "$api_fixture_source/Cargo.toml" "$SQL_FUN_API_FIXTURE/Cargo.toml"
for catalog in functions func_detail; do
  printf '[]\n' >"$SQL_FUN_API_FIXTURE/home/postgres/17/$catalog.json"
done
for catalog in left_unary_op right_unary_op casts; do
  printf '{}\n' >"$SQL_FUN_API_FIXTURE/home/postgres/17/$catalog.json"
done
printf '%s\n' '{"intentionally_unused":[]}' >"$SQL_FUN_API_FIXTURE/home/postgres/17/binary_op.json"
printf '%s\n' '[{"schema":"pg_catalog","name":"int4","collatable":false}]' >"$SQL_FUN_API_FIXTURE/home/postgres/17/pg_type.json"
printf '%s\n' '-- synthetic builtin' >"$SQL_FUN_API_FIXTURE/home/postgres/17/schema.sql"
for catalog in collations operator_classes; do
  cp "$api_fixture_source/$catalog.json" "$SQL_FUN_API_FIXTURE/home/postgres/17/$catalog.json"
done
api_dump_hash=$(sha256sum "$SQL_FUN_API_FIXTURE/schema.sql")
api_dump_hash=${api_dump_hash%% *}
jq -n --arg fixture "$SQL_FUN_API_FIXTURE" --arg hash "$api_dump_hash" '{
  method:"initialize_schema_context", params:{
    datasource:"products",
    schema_dump:($fixture + "/schema.sql"), expected_hash:$hash,
    environments:{
      SQL_FUN_HOME:($fixture + "/home"),
      SQL_FUN_METADATA_FILE:($fixture + "/Cargo.toml")
    }
  }
}' >"$SQL_FUN_API_FIXTURE/initialize.json"

api_server_pid=""
function cleanup_api_fixture() {
  # Only signal our still-running shell child; a registration alone is never
  # authority to stop a process, and an exited child's PID must not be reused.
  local running_pid
  for running_pid in $(jobs -pr); do
    if [[ "$running_pid" == "$api_server_pid" ]]; then
      kill -KILL "$running_pid" 2>/dev/null || true
    fi
  done
  if [[ -n "$api_server_pid" ]]; then
    wait "$api_server_pid" 2>/dev/null || true
  fi
  rm -rf -- "$SQL_FUN_API_FIXTURE"
}
trap cleanup_api_fixture EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

"$SQL_FUN_TEST_SERVER_BIN" </dev/null >"$SQL_FUN_API_FIXTURE/server.log" 2>&1 &
api_server_pid=$!
for ((attempt = 0; attempt < 200; attempt++)); do
  if [[ -s "$SQL_FUN_API_FIXTURE/sql-fun-server.pid" ]]; then
    break
  fi
  if ! kill -0 "$api_server_pid" 2>/dev/null; then
    cat "$SQL_FUN_API_FIXTURE/server.log" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ ! -s "$SQL_FUN_API_FIXTURE/sql-fun-server.pid" ]]; then
  echo "Scrut fixture: server registration did not become ready" >&2
  cat "$SQL_FUN_API_FIXTURE/server.log" >&2
  exit 1
fi

# Scrut terminates its test shells without running their EXIT traps.
# Its parent retains ownership and cleans up after success or failure.
unset BASH_ENV
scrut test "$1"
