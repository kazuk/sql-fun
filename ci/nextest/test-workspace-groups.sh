#!/usr/bin/env bash
# Offline contracts, maintained by the workspace step in local verification/CI.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source .local/sbin/workspace-test-groups
source .local/sbin/workspace-verification
function require_command() { :; }
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
test_metadata=$(cargo metadata --locked --no-deps --format-version=1)
validate_workspace_test_groups "$test_metadata"
# New/removed packages and duplicate ownership must fail before archiving.
if (WORKSPACE_TEST_GROUP_A+=(unexpected-package); validate_workspace_test_groups "$test_metadata") >"$scratch/invalid" 2>&1; then
  echo 'Unexpected package accepted' >&2; exit 1
fi
if (WORKSPACE_TEST_GROUP_A+=(sql-fun-server); validate_workspace_test_groups "$test_metadata") >"$scratch/invalid" 2>&1; then
  echo 'Duplicate package accepted' >&2; exit 1
fi
if (unset 'WORKSPACE_TEST_GROUP_A[0]'; validate_workspace_test_groups "$test_metadata") >"$scratch/invalid" 2>&1; then
  echo 'Missing package accepted' >&2; exit 1
fi

# Capture the actual archive producer's Cargo arguments.
function cargo() {
  if [[ "$1" == metadata ]]; then
    printf '%s\n' "$test_metadata"
  else
    printf '%s\n' "$@" > "$scratch/build"
  fi
}
FEATURES=(--all-features)
SQL_FUN_CI_ARTIFACT_DIR="$scratch"
for group in a b; do
  build_workspace_test_group "$group"
  mapfile -t args < "$scratch/build"
  [[ "${args[*]}" == nextest\ archive\ --locked* ]]
  owned=()
  for ((i=0; i<${#args[@]}; i++)); do
    [[ "${args[i]}" != -p ]] || owned+=("${args[i+1]}")
    [[ "${args[i]}" != --workspace && "${args[i]}" != --all-targets ]]
  done
  [[ "$(printf '%s\n' "${owned[@]}")" == "$(workspace_test_group_packages "$group")" ]]
  [[ "${args[*]}" == *"--all-features"* && "${args[*]}" == *"nextest-${group}.tar.zst" ]]
  case "$group" in
    a) [[ "${args[*]}" == *sql-fun-server-api/bin-deps,sql-fun-server-api/openrpc-schema* ]] ;;
    b) [[ "${args[*]}" == *sql-fun-core/beta_postgres_versions* ]] ;;
  esac
done

# Both archives and regressions must run, in order, for every failure location.
GIT_ROOT=$PWD
NEXTEST_EXTRA=''
function run_workspace_nextest() {
  local group
  case "$*" in
    *nextest-a.tar.zst*) group=a ;;
    *nextest-b.tar.zst*) group=b ;;
    *) return 99 ;;
  esac
  [[ "$*" == *"--profile ci-${group}"* && "$*" == *"--workspace-remap $GIT_ROOT"* ]] || return 99
  echo "$group" >> "$scratch/order"
  [[ "$failed" != "$group" ]]
}
function run_sqlast_regressions() {
  echo regressions >> "$scratch/order"
  [[ "$failed" != regressions ]]
  # This must not run after an earlier command fails inside the function.
  echo completed >> "$scratch/order"
}
for failed in none a b regressions; do
  : > "$scratch/order"
  status=0
  # Do not invoke this function in a conditional: that disables Bash errexit.
  set +e
  (set -e
    WORKSPACE_PHASE=test SQL_FUN_CI_TEST_GROUPS='a b' \
      SQL_FUN_BIN=/explicit/cli SQL_FUN_TEST_SERVER_BIN=/explicit/server run_workspace
  ) > "$scratch/route-log" 2>&1
  status=$?
  set -e
  if [[ "$failed" == none ]]; then
    [[ "$status" == 0 ]]
  else
    [[ "$status" != 0 ]]
  fi
  expected=$'a\nb\nregressions'
  [[ "$failed" == regressions ]] || expected+=$'\ncompleted'
  [[ "$(cat "$scratch/order")" == "$expected" ]]
done
# The split routes share production coordination, not a test-only replica.
source .local/sbin/workspace-verification
function run_workspace_build_checks() { echo unexpected-build >> "$scratch/order"; return 98; }
function run_sqlast_feature_boundaries() { return 98; }

# Execution routes must not require unrelated artifacts, and both Rust groups
# must execute even after the first fails. Missing inputs fail before execution.
for failed in none a b; do
  : > "$scratch/order"
  (
    unset SQL_FUN_BIN SQL_FUN_TEST_SERVER_BIN
    WORKSPACE_PHASE=rust-test
    SQL_FUN_CI_TEST_GROUPS='a b'
    run_workspace
  ) > "$scratch/route-log" 2>&1 && status=0 || status=$?
  [[ "$(cat "$scratch/order")" == $'a\nb' ]]
  if [[ "$failed" == none ]]; then [[ "$status" == 0 ]]; else [[ "$status" != 0 ]]; fi
done
for phase in rust-test test; do
  : > "$scratch/order"
  (
    unset SQL_FUN_CI_ARTIFACT_DIR SQL_FUN_BIN SQL_FUN_TEST_SERVER_BIN
    WORKSPACE_PHASE=$phase
    run_workspace
  ) > "$scratch/missing" 2>&1 && status=0 || status=$?
  [[ "$status" != 0 && ! -s "$scratch/order" ]]
done

# Restore the real regression function and run against a tiny runner boundary.
# Cargo metadata is allowed; any compilation/check/archive invocation is rejected.
source .local/sbin/workspace-verification
function cargo() {
  if [[ "$*" != 'metadata --locked --no-deps --format-version 1' ]]; then
    echo unexpected-cargo >> "$scratch/order"
    return 97
  fi
  jq -n --arg directory "$scratch/target" '{target_directory: $directory}'
}
mkdir -p "$scratch/root/tests/sqlast-regressions"
cat > "$scratch/root/tests/sqlast-regressions/run.sh" <<'RUNNER'
#!/usr/bin/env bash
set -euo pipefail
[[ "$SQL_FUN_BIN" == /explicit/cli && "$SQL_FUN_TEST_SERVER_BIN" == /explicit/server ]]
[[ "$SQL_FUN_HOME" == "$GIT_ROOT/sql-fun-home" ]]
[[ "$SQL_FUN_REGRESSION_REPORT_DIR" == "$scratch/target/sqlast-regressions" ]]
echo regressions >> "$scratch/order"
[[ "$failed" != regressions ]]
RUNNER
chmod +x "$scratch/root/tests/sqlast-regressions/run.sh"
export scratch failed
for failed in none regressions; do
  : > "$scratch/order"
  (
    export GIT_ROOT="$scratch/root"
    unset SQL_FUN_CI_ARTIFACT_DIR SQL_FUN_CI_TEST_GROUPS
    WORKSPACE_PHASE=sqlast-test
    SQL_FUN_BIN=/explicit/cli
    SQL_FUN_TEST_SERVER_BIN=/explicit/server
    run_workspace
  ) > "$scratch/route-log" 2>&1 && status=0 || status=$?
  [[ "$(cat "$scratch/order")" == regressions ]]
  if [[ "$failed" == none ]]; then [[ "$status" == 0 ]]; else [[ "$status" != 0 ]]; fi
done
for missing in SQL_FUN_BIN SQL_FUN_TEST_SERVER_BIN; do
  : > "$scratch/order"
  (
    WORKSPACE_PHASE=sqlast-test
    SQL_FUN_BIN=/explicit/cli
    SQL_FUN_TEST_SERVER_BIN=/explicit/server
    unset "$missing"
    run_workspace
  ) > "$scratch/missing" 2>&1 && status=0 || status=$?
  [[ "$status" != 0 && ! -s "$scratch/order" ]]
done
# Check the actual workflow consumers and execute its aggregate-gate command.
function workflow_job() {
  awk -v name="$1" '
    $0 == "  " name ":" { copying=1; next }
    copying && /^  [^ ]/ { exit }
    copying { print }
  ' .github/workflows/build-test.yml
}
for prefix in workspace workspace-all-features; do
  rust_job=$(workflow_job "$prefix")
  sqlast_job=$(workflow_job "$prefix-sqlast-regressions")
  [[ "$rust_job" == *"needs: $prefix-tests-build"* ]]
  [[ "$rust_job" != *"name: $prefix-cli"* && "$rust_job" != *"name: $prefix-server"* ]]
  [[ "$rust_job" == *'--workspace-phase=rust-test'* ]]
  [[ "$sqlast_job" == *"needs: [$prefix-cli-build, $prefix-server-build]"* ]]
  [[ "$sqlast_job" != *nextest* && "$sqlast_job" == *'--workspace-phase=sqlast-test'* ]]
done
gate=$(workflow_job build)
[[ "$gate" == *'if: ${{ always() }}'* ]]
gate_command=$(sed -n 's/^        run: //p' <<< "$gate")
for dependency in workspace workspace-sqlast-regressions workspace-all-features workspace-all-features-sqlast-regressions; do
  [[ "$gate" == *"      - $dependency"* ]]
  for result in success failure cancelled skipped; do
    export NEEDS_RESULTS
    NEEDS_RESULTS=$(jq -n --arg dependency "$dependency" --arg result "$result" \
      '{other: {result: "success"}} + {($dependency): {result: $result}}')
    bash -euo pipefail -c "$gate_command" > "$scratch/gate" 2>&1 && status=0 || status=$?
    if [[ "$result" == success ]]; then [[ "$status" == 0 ]]; else [[ "$status" != 0 ]]; fi
  done
done
echo 'Workspace archive group, execution phase and workflow contracts passed'
