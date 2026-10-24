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
reports=${SQL_FUN_REGRESSION_REPORT_DIR:?SQL_FUN_REGRESSION_REPORT_DIR must select the report directory}
mkdir -p "$reports"
reports=$(cd -- "$reports" && pwd)
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

# Keep fixture cases together on disk; adapt only at the existing CLI boundary.
project_cases() {
  jq '{sql: map(.sql), expected: map(.expected)}' "$1" >"$2"
}

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
  if ! jq -e --argjson count "$count" '
    type == "array" and length == $count and
    all(.[]; type == "object" and
      (.name | type == "string" and length > 0) and
      (.sql | type == "string" and length > 0) and
      (.expected | type == "object")) and
    (map(.name) | unique | length) == $count
  ' "$file" >/dev/null; then
    echo "Invalid cases in $file: expected $count objects with unique nonempty names, SQL strings and expectation objects" >&2
    exit 1
  fi
  projected="$runtime/$fixture.json"
  project_cases "$file" "$projected"
  IFS=, read -ra selected_versions <<< "$versions"
  for version in "${selected_versions[@]}"; do
    [[ "$version" == 17 || "$version" == 18 ]] || exit 1
    test -s "$SQL_FUN_HOME/postgres/$version/metadata.json"
    jq -e --argjson version "$version" '.server_version_num / 10000 | floor == $version' \
      "$SQL_FUN_HOME/postgres/$version/metadata.json" >/dev/null
    checked_versions["$version"]=1
    metadata="$runtime/metadata-$version.toml"
    cat >"$metadata" <<METADATA
[package.metadata.database.regression]
engine = "PostgreSQL"
dialect = "postgres"
version = "$version"
search_path = ["public"]
connector = "tokio-postgres"
METADATA
    args=(--term-color never --metadata-file "$metadata" --datasource regression)
    if [[ "$schema" != - ]]; then args+=(--schema-file "$suite/$schema"); fi
    echo "== $fixture PostgreSQL $version ($level), expected $count cases; catalog $SQL_FUN_HOME/postgres/$version"
    jq -r 'to_entries[] | "  [\(.key)] \(.value.name)"' "$file" \
      >"$reports/$fixture-$version.log"
    rm -f "$reports/$fixture-$version.xml"
    status=0
    group_failed=0
    # Measure only the product invocation; parsing reports and setup are outside.
    { time "$SQL_FUN_BIN" "${args[@]}" test "$projected#$.sql" --expect "$projected#$.expected" \
        --schema-level "$level" --junit-xml "$reports/$fixture-$version.xml" \
        >>"$reports/$fixture-$version.log" 2>&1;
    } 2>"$runtime/test-seconds" || status=$?
    elapsed=$(<"$runtime/test-seconds")
    test_seconds=$(jq -n --argjson total "$test_seconds" --argjson elapsed "$elapsed" '($total + $elapsed) * 1000 | round / 1000')
    cat "$reports/$fixture-$version.log"
    if (( status != 0 )); then group_failed=1; fi
    if ! grep -F ' failures="0" errors="0"' "$reports/$fixture-$version.xml" >/dev/null 2>&1; then
      echo "JUnit report missing or contains failures/errors: $fixture PostgreSQL $version" >&2
      group_failed=1
    fi
    # Verify both the declared JUnit total and the actual testcase elements.
    actual=0
    if [[ -s "$reports/$fixture-$version.xml" ]]; then
      actual=$(sed -n 's/.*<testsuite name="sql-fun test" tests="\([0-9]*\)".*/\1/p' "$reports/$fixture-$version.xml")
    fi
    elements=$(grep -c '<testcase ' "$reports/$fixture-$version.xml" 2>/dev/null || true)
    if [[ "$elements" != "$count" ]]; then echo "JUnit testcase count mismatch: expected $count, actual ${elements:-0}" >&2; group_failed=1; fi
    if [[ "$actual" != "$count" ]]; then echo "Case count mismatch: expected $count, actual $actual" >&2; group_failed=1; fi
    if (( group_failed != 0 )); then failed=1; result=FAIL; else result=PASS; fi
    echo "== Result: $fixture PostgreSQL $version: $result; cases=$actual/$count; test-command=${elapsed}s; exit=$status" \
      | tee -a "$reports/$fixture-$version.log"
    total=$((total + ${actual:-0}))
    groups=$((groups + 1))
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
# Deliberately wrong expectations must fail while an independent later case passes.
# Keep expected-failure XML apart from positive JUnit reports consumed by CI.
mkdir -p "$reports/controls"
jq '. + [.[0]] |
    .[0].expected.columns[0].name = "intentionally_wrong" |
    .[1].expected.diagnostics = ["intentionally_missing"]' \
  "$suite/pilot.json" >"$runtime/negative-cases.json"
project_cases "$runtime/negative-cases.json" "$runtime/negative.json"
for version in "${!checked_versions[@]}"; do
  control="$reports/controls/negative-$version"
  rm -f "$control.xml"
  echo "== Expected-failure controls: PostgreSQL $version (not regression failures)"
  echo "The next 2 FAIL results are intentional; exit 1 and a later PASS are required."
  status=0
  "$SQL_FUN_BIN" --term-color never --metadata-file "$runtime/metadata-$version.toml" \
    --datasource regression test "$runtime/negative.json#$.sql" \
    --expect "$runtime/negative.json#$.expected" --schema-level builtin \
    --junit-xml "$control.xml" >"$control.log" 2>&1 || status=$?
  cat "$control.log"
  [[ "$status" == 1 ]] || { echo "Negative control returned $status, expected 1" >&2; exit 1; }
  for evidence in 'tests="3" failures="2" errors="0"' '<testcase' ; do
    grep -F "$evidence" "$control.xml" >/dev/null
  done
  for evidence in "FAIL $runtime/negative.json#$.sql[0]" \
    "FAIL $runtime/negative.json#$.sql[1]" "PASS $runtime/negative.json#$.sql[2]" \
    '  column 0:' '    expected:' '    actual:' '    missing:' '    unexpected:' \
    '3 cases: 1 passed, 2 failed or errored'; do
    grep -Fx "$evidence" "$control.log" >/dev/null
  done
  # Invalid selection is a command error, never a successful empty test run.
  status=0
  "$SQL_FUN_BIN" --term-color never --metadata-file "$runtime/metadata-$version.toml" \
    --datasource regression test "$runtime/pilot.json#$.nonexistent" \
    --expect "$runtime/pilot.json#$.expected" --schema-level builtin \
    >"$reports/controls/invalid-$version.log" 2>&1 || status=$?
  [[ "$status" != 0 ]] || { echo 'Invalid selection unexpectedly succeeded' >&2; exit 1; }
  status=0
  "$SQL_FUN_BIN" --term-color never --metadata-file "$runtime/metadata-$version.toml" \
    --datasource nonexistent test "$runtime/pilot.json#$.sql" \
    --expect "$runtime/pilot.json#$.expected" --schema-level builtin \
    >"$reports/controls/datasource-$version.log" 2>&1 || status=$?
  [[ "$status" == 1 ]] || { echo "Missing datasource returned $status, expected 1" >&2; exit 1; }
  if grep -q '^PASS ' "$reports/controls/datasource-$version.log"; then
    echo 'Unconfigured datasource unexpectedly analyzed a passing query' >&2; exit 1
  fi
  echo "PostgreSQL $version: negative controls verified (3 executed, 2 intentional failures, 1 pass; invalid selector and missing datasource rejected)"
done
rate=$(jq -n --argjson cases "$total" --argjson seconds "$test_seconds" \
  'if $seconds > 0 then $cases / $seconds * 1000 | round / 1000 else 0 end')
echo "SQLAST regressions: $total executed cases in $groups groups; failure=$failed; test-command-total=${test_seconds}s; cases/second=$rate"
exit "$failed"
