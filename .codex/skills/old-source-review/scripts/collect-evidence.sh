#!/usr/bin/env bash

set -euo pipefail

readonly SKILL_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
readonly REPOSITORY_ROOT="$(git -C "$SKILL_DIRECTORY" rev-parse --show-toplevel)"
readonly OLD_SOURCE="$REPOSITORY_ROOT/.local/bin/old-source"

target=""
target_source="old-source"
candidate_date=""
declare -a definitions=()

usage() {
  printf '%s\n' 'usage: collect-evidence.sh [--target <tracked-rust-source>] [--definition <name>]...'
}

fail() {
  printf '%s\n' "$*" >&2
  exit 1
}

while (($# > 0)); do
  case "$1" in
    --target)
      (($# >= 2)) || fail 'missing value for --target'
      target="$2"
      target_source="explicit"
      shift 2
      ;;
    --definition)
      (($# >= 2)) || fail 'missing value for --definition'
      [[ -n "$2" ]] || fail 'definition names must not be empty'
      definitions+=("$2")
      shift 2
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      fail "unknown argument: $1"
      ;;
  esac
done

if [[ -z "$target" ]]; then
  candidate="$($OLD_SOURCE | sed -n '1p')"
  [[ -n "$candidate" ]] || fail 'old-source returned no candidate'
  candidate_date="${candidate%% *}"
  target="${candidate#* }"
fi

[[ "$target" == *.rs ]] || fail "target is not a Rust source file: $target"
git -C "$REPOSITORY_ROOT" ls-files --error-unmatch -- "$target" >/dev/null \
  || fail "target is not a tracked file: $target"

commit_metadata="$(git -C "$REPOSITORY_ROOT" log --follow -1 --format='%H%n%cs%n%s' -- "$target")"
[[ -n "$commit_metadata" ]] || fail "target has no Git history: $target"

commit_sha="$(sed -n '1p' <<<"$commit_metadata")"
commit_date="$(sed -n '2p' <<<"$commit_metadata")"
commit_subject="$(sed -n '3p' <<<"$commit_metadata")"

usages='[]'
for definition in "${definitions[@]}"; do
  matches="$({ git -C "$REPOSITORY_ROOT" grep -n --fixed-strings -- "$definition" -- '*.rs' || true; })"
  matches_json="$(printf '%s\n' "$matches" | jq -Rsc 'split("\n") | map(select(length > 0))')"
  usages="$(jq -cn \
    --arg definition "$definition" \
    --argjson matches "$matches_json" \
    --argjson previous "$usages" \
    '$previous + [{definition: $definition, literal_occurrences: $matches}]')"
done

jq -n \
  --arg target "$target" \
  --arg target_source "$target_source" \
  --arg candidate_date "$candidate_date" \
  --arg commit_sha "$commit_sha" \
  --arg commit_date "$commit_date" \
  --arg commit_subject "$commit_subject" \
  --argjson usages "$usages" \
  '{
    target: $target,
    target_source: $target_source,
    candidate_last_change_date: (if $candidate_date == "" then null else $candidate_date end),
    tracked_rust_source: true,
    last_modifying_commit: {
      sha: $commit_sha,
      date: $commit_date,
      subject: $commit_subject
    },
    literal_definition_occurrences: $usages
  }'
