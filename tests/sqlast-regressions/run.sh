#!/usr/bin/env bash
# Internal shared source/installed regression entry point. No live database is used.
set -euo pipefail
: "${SQL_FUN_HOME:?SQL_FUN_HOME must select the real builtin installation}"
: "${SQL_FUN_BIN:?SQL_FUN_BIN must name the CLI executable}"
: "${SQL_FUN_TEST_SERVER_BIN:?SQL_FUN_TEST_SERVER_BIN must name the server executable}"
for executable in "$SQL_FUN_BIN" "$SQL_FUN_TEST_SERVER_BIN"; do
  test -x "$executable" || { echo "Not executable: $executable" >&2; exit 1; }
done
suite=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "$suite/operations.sh"
reports=${SQL_FUN_REGRESSION_REPORT_DIR:?SQL_FUN_REGRESSION_REPORT_DIR must select the report directory}
mkdir -p "$reports"
reports=$(cd -- "$reports" && pwd)
: > "$reports/timings.tsv"
runtime=$(mktemp -d /tmp/sql-fun-regressions.XXXXXXXX)
export USER="${runtime##*/}"
export SQL_FUN_SERVER="$SQL_FUN_TEST_SERVER_BIN"
unset SQL_FUN_SERVER_COMMAND SQL_FUN_METADATA_FILE SQL_FUN_DATASOURCE RUST_LOG BASH_ENV \
  SQL_FUN_SCHEMA SQL_FUN_SCHEMA_FILE SQL_FUN_DIALECT SQL_FUN_CTE_CATALOG_DIR \
  SQL_FUN_METADATA_FORMAT SQL_FUN_NO_BUILTIN SQL_FUN_HIGHLIGHTER_THEME SQL_FUN_MODE \
  SQL_FUN_PG_SEARCH_PATH SQL_FUN_POSTGRES_VERSION SQL_FUN_EXTENSIONS \
  SQL_FUN_BUILTIN_SCHEMA SQL_FUN_EXTENSIONS_DIR SQL_FUN_LOG_FILE SQL_FUN_OUTPUT_FORMAT
server_pid=""
cleanup() {
  local running_pid
  for running_pid in $(jobs -pr); do
    if [[ "$running_pid" == "$server_pid" ]]; then
      kill -KILL "$running_pid" 2>/dev/null || true
    fi
  done
  if [[ -n "$server_pid" ]]; then wait "$server_pid" 2>/dev/null || true; fi
  cp "$runtime/server.log" "$reports/server.log"
  rm -rf -- "$runtime"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
"$SQL_FUN_TEST_SERVER_BIN" </dev/null >"$runtime/server.log" 2>&1 &
server_pid=$!
for ((attempt = 0; attempt < 200; attempt++)); do
  if [[ -s "$runtime/sql-fun-server.pid" ]]; then break; fi
  if ! kill -0 "$server_pid" 2>/dev/null; then cat "$runtime/server.log" >&2; exit 1; fi
  sleep 0.05
done
test -s "$runtime/sql-fun-server.pid" || { echo 'Server readiness timed out' >&2; exit 1; }

# Every manifest row is explicit. Counts guard against accidental omission/empty runs.
total=0
failed=0
groups=0
test_seconds=0
# Bash time emits decimal seconds independent of the caller's numeric locale.
export LC_ALL=C
TIMEFORMAT='%3R'
declare -A checked_versions=()
declare -A selected_fixtures=()
while IFS=$'\t' read -r fixture level schema count versions; do
  [[ -z "$fixture" || "$fixture" == \#* ]] && continue
  [[ "$level" == builtin || "$level" == default || "$level" == extensions || "$level" == project ]] || {
    echo "Invalid schema level for $fixture: $level" >&2; exit 1;
  }
  [[ -n "$versions" ]] || { echo "No versions for $fixture" >&2; exit 1; }
  [[ "$fixture" =~ ^[a-z0-9_-]+$ && "$count" =~ ^[1-9][0-9]*$ ]] || exit 1
  [[ -z "${selected_fixtures[$fixture]+x}" ]] || { echo "Duplicate fixture: $fixture" >&2; exit 1; }
  selected_fixtures["$fixture"]=1
  file="$suite/$fixture.json"
  # Case shape, names and expectations are validated by the product CLI.
  # The manifest owns the independently declared expected suite size.
  if ! jq -e --argjson count "$count" 'type == "array" and length == $count' "$file" >/dev/null; then
    echo "Invalid case count in $file: expected $count" >&2
    exit 1
  fi
  IFS=, read -ra selected_versions <<< "$versions"
  for version in "${selected_versions[@]}"; do
    [[ "$version" == 17 || "$version" == 18 ]] || exit 1
    test -s "$SQL_FUN_HOME/postgres/$version/metadata.json"
    jq -e --argjson version "$version" '.server_version_num / 10000 | floor == $version' \
      "$SQL_FUN_HOME/postgres/$version/metadata.json" >/dev/null
    checked_versions["$version"]=1
    run_group
  done
done <"$suite/manifest.tsv"

for fixture_file in "$suite"/*.json; do
  fixture_name=${fixture_file##*/}
  fixture_name=${fixture_name%.json}
  if [[ -z "${selected_fixtures[$fixture_name]+x}" ]]; then
    echo "Fixture not registered in manifest: $fixture_name" >&2
    failed=1
  fi
done
(( groups > 0 && total > 0 )) || { echo 'Empty regression suite' >&2; exit 1; }
run_controls
rate=$(jq -n --argjson cases "$total" --argjson seconds "$test_seconds" \
  'if $seconds > 0 then $cases / $seconds * 1000 | round / 1000 else 0 end')
echo "SQLAST regressions: $total executed cases, in $groups groups; failure=$failed; test-command-total=${test_seconds}s; cases/second=$rate"
exit "$failed"
