#!/usr/bin/env bash
# Offline contracts, maintained by the workspace step in local verification/CI.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source .local/sbin/workspace-test-groups
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
NEXTEST_EXTRA='--config-file ci/nextest/config.toml'
function run_nextest() {
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
  (set -e; run_grouped_workspace_verification)
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
echo 'Workspace archive group contracts passed'
