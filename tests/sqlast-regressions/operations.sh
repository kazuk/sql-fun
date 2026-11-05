#!/usr/bin/env bash
# Internal operations shared by serial and parallel regression execution.
# Callers select suite/runtime/reports, binaries, and a server namespace.
write_metadata() {
  metadata="$runtime/metadata-$version.toml"
  cat >"$metadata" <<METADATA
[package.metadata.database.regression]
engine = "PostgreSQL"
dialect = "postgres"
version = "$version"
search_path = ["public"]
connector = "tokio-postgres"
METADATA
  # Optional fixture-owned datasource fields are shared by source/installed checks.
  if [[ -f "$suite/$fixture.metadata.toml" ]]; then
    cat "$suite/$fixture.metadata.toml" >>"$metadata"
  fi
}

run_group() {
  write_metadata
  args=(--term-color never --metadata-file "$metadata" --datasource regression)
  if [[ "$schema" != - ]]; then args+=(--schema-file "$suite/$schema"); fi
  echo "== $fixture PostgreSQL $version ($level), expected $count cases; catalog $SQL_FUN_HOME/postgres/$version"
  rm -f "$reports/$fixture-$version.xml"
  status=0
  group_failed=0
  # Measure only the product invocation; parsing reports and setup are outside.
  { time "$SQL_FUN_BIN" "${args[@]}" test "$file" \
      --schema-level "$level" --junit-xml "$reports/$fixture-$version.xml" \
      >"$reports/$fixture-$version.log" 2>&1;
  } 2>"$runtime/test-seconds" || status=$?
  elapsed=$(<"$runtime/test-seconds")
  printf '%s\t%s\n' "$fixture-$version" "$elapsed" >> "$reports/timings.tsv"
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
}

run_controls() {
  # Deliberately wrong expectations must fail while an independent later case passes.
  # Keep expected-failure XML apart from positive JUnit reports consumed by CI.
  mkdir -p "$reports/controls"
  jq '. + [.[0]] |
      .[2].name = "independent-later-pass" |
      .[0].expected.columns[0].name = "intentionally_wrong" |
      .[1].expected.diagnostics = ["intentionally_missing"]' \
    "$suite/pilot.json" >"$runtime/negative-cases.json"
  for version in "${!checked_versions[@]}"; do
    write_metadata
    control="$reports/controls/negative-$version"
    rm -f "$control.xml"
    echo "== Expected-failure controls: PostgreSQL $version (not regression failures)"
    echo "The next 2 FAIL results are intentional; exit 1 and a later PASS are required."
    status=0
    "$SQL_FUN_BIN" --term-color never --metadata-file "$runtime/metadata-$version.toml" \
      --datasource regression test "$runtime/negative-cases.json" --schema-level builtin \
      --junit-xml "$control.xml" >"$control.log" 2>&1 || status=$?
    cat "$control.log"
    [[ "$status" == 1 ]] || { echo "Negative control returned $status, expected 1" >&2; exit 1; }
    for evidence in 'tests="3" failures="2" errors="0"' '<testcase' ; do
      grep -F "$evidence" "$control.xml" >/dev/null
    done
    for evidence in "FAIL $runtime/negative-cases.json::$(jq -r '.[0].name' "$runtime/negative-cases.json")" \
      "FAIL $runtime/negative-cases.json::$(jq -r '.[1].name' "$runtime/negative-cases.json")" "PASS $runtime/negative-cases.json::independent-later-pass" \
      '  Column 0 · name' '    Expected: "intentionally_wrong"' '  Diagnostics:' '    Missing:' '    Unexpected:' \
      '3 cases: 1 passed, 2 failed or errored'; do
      grep -Fx "$evidence" "$control.log" >/dev/null
    done
    # Invalid selection is a command error, never a successful empty test run.
    status=0
    "$SQL_FUN_BIN" --term-color never --metadata-file "$runtime/metadata-$version.toml" \
      --datasource regression test "$suite/pilot.json#$.nonexistent" --schema-level builtin \
      >"$reports/controls/invalid-$version.log" 2>&1 || status=$?
    [[ "$status" != 0 ]] || { echo 'Invalid selection unexpectedly succeeded' >&2; exit 1; }
    status=0
    "$SQL_FUN_BIN" --term-color never --metadata-file "$runtime/metadata-$version.toml" \
      --datasource nonexistent test "$suite/pilot.json" --schema-level builtin \
      >"$reports/controls/datasource-$version.log" 2>&1 || status=$?
    [[ "$status" == 1 ]] || { echo "Missing datasource returned $status, expected 1" >&2; exit 1; }
    if grep -q '^PASS ' "$reports/controls/datasource-$version.log"; then
      echo 'Unconfigured datasource unexpectedly analyzed a passing query' >&2; exit 1
    fi
    # A recoverable schema error leaves a usable context, but only explicit
    # permission to ignore errors may run queries against it.
    cat >"$runtime/schema-diagnostics.sql" <<'SQL'
CREATE TABLE measured (id int4);
ALTER TABLE measured ALTER COLUMN id SET STATISTICS 10001;
ALTER TABLE measured ALTER COLUMN id SET STATISTICS -2;
SQL
    jq '.[0:1] | .[0].name = "loaded-schema" | .[0].sql = "SELECT id FROM measured" |
      .[0].expected.columns[0].name = "id" | .[0].expected.columns[0].nullability = "nullable"' \
      "$suite/pilot.json" >"$runtime/schema-cases.json"
    for threshold in default warning error; do
      threshold_args=()
      if [[ "$threshold" != default ]]; then threshold_args=(--ignore-schema-diagnostics "$threshold"); fi
      control="$reports/controls/schema-$version-$threshold"
      status=0
      "$SQL_FUN_BIN" --term-color never --metadata-file "$runtime/metadata-$version.toml" \
        --datasource regression --schema-file "$runtime/schema-diagnostics.sql" \
        test "$runtime/schema-cases.json" --schema-level project \
        "${threshold_args[@]}" --junit-xml "$control.xml" >"$control.log" 2>&1 || status=$?
      grep -F 'alter_column_statistics_reduced' "$control.log" >/dev/null
      grep -F 'alter_column_statistics_too_low' "$control.log" >/dev/null
      if [[ "$threshold" == error ]]; then
        [[ "$status" == 0 ]] || { cat "$control.log"; exit 1; }
        grep -F 'tests="1" failures="0" errors="0"' "$control.xml" >/dev/null
        grep -F 'PASS ' "$control.log" >/dev/null
      else
        [[ "$status" == 1 ]] || { cat "$control.log"; exit 1; }
        grep -F 'tests="1" failures="0" errors="1"' "$control.xml" >/dev/null
        grep -F 'ERROR ' "$control.log" >/dev/null
      fi
    done
    echo "PostgreSQL $version: negative controls verified (3 executed, 2 intentional failures, 1 pass; invalid selector and missing datasource rejected)"
  done
}
