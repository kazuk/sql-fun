#!/usr/bin/env bash

set -euo pipefail

readonly SKILL_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
readonly REPOSITORY_ROOT="$(git -C "$SKILL_DIRECTORY" rev-parse --show-toplevel)"
readonly OLD_SOURCE="$REPOSITORY_ROOT/.local/bin/old-source"

target=""
suggest_candidate=false
oldest=false
evidence_commit=""
declare -a definitions=()

usage() {
  printf '%s\n' 'usage: collect-evidence.sh --oldest [--definition <name>]...'
  printf '%s\n' '   or: collect-evidence.sh --suggest-candidate'
  printf '%s\n' '   or: collect-evidence.sh --target <tracked-rust-source> [--definition <name>]... [--evidence-commit <sha>]'
}

fail() {
  printf '%s\n' "$*" >&2
  exit 1
}

while (($# > 0)); do
  case "$1" in
    --suggest-candidate)
      suggest_candidate=true
      shift
      ;;
    --oldest)
      oldest=true
      shift
      ;;
    --evidence-commit)
      (($# >= 2)) || fail 'missing value for --evidence-commit'
      [[ "$2" =~ ^[0-9a-fA-F]{40}$ ]] || fail 'evidence commit must be a full commit SHA'
      evidence_commit="$2"
      shift 2
      ;;
    --target)
      (($# >= 2)) || fail 'missing value for --target'
      target="$2"
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

if "$oldest" || "$suggest_candidate"; then
  [[ -z "$target" && -z "$evidence_commit" ]] || fail 'candidate modes cannot be combined with target or evidence commit'
  if "$oldest"; then
    ! "$suggest_candidate" || fail '--oldest cannot be combined with --suggest-candidate'
  fi
  if "$suggest_candidate"; then
    ((${#definitions[@]} == 0)) || fail '--suggest-candidate cannot be combined with definitions'
  fi
  candidate="$(cd -- "$REPOSITORY_ROOT" && "$OLD_SOURCE" | sed -n '1p')"
  [[ -n "$candidate" ]] || fail 'old-source returned no candidate'
  if "$oldest"; then
    target="${candidate#* }"
  fi
fi

if "$suggest_candidate"; then
  [[ -z "$target" && ${#definitions[@]} -eq 0 ]] \
    || fail '--suggest-candidate cannot be combined with target or definition arguments'
  jq -n \
    --arg candidate_date "${candidate%% *}" \
    --arg candidate "${candidate#* }" \
    '{suggested_target: $candidate, candidate_last_change_date: $candidate_date}'
  exit 0
fi

[[ -n "$target" ]] || fail 'select --oldest or supply --target'
[[ "$target" == *.rs ]] || fail "target is not a Rust source file: $target"
if [[ -z "$evidence_commit" ]]; then
  evidence_commit="$(git -C "$REPOSITORY_ROOT" rev-parse HEAD)"
fi
git -C "$REPOSITORY_ROOT" cat-file -e "$evidence_commit^{commit}" \
  || fail "invalid evidence commit: $evidence_commit"
git -C "$REPOSITORY_ROOT" cat-file -e "$evidence_commit:$target" \
  || fail "target is not tracked at the evidence commit: $target"

commit_metadata="$(git -C "$REPOSITORY_ROOT" log --follow -1 --format='%H%n%cs%n%s' "$evidence_commit" -- "$target")"
[[ -n "$commit_metadata" ]] || fail "target has no Git history: $target"

commit_sha="$(sed -n '1p' <<<"$commit_metadata")"
commit_date="$(sed -n '2p' <<<"$commit_metadata")"
commit_subject="$(sed -n '3p' <<<"$commit_metadata")"

collect_usages() {
  local definition matches status
  for definition in "${definitions[@]}"; do
    if matches="$(git -C "$REPOSITORY_ROOT" grep -n --fixed-strings -e "$definition" "$evidence_commit" -- '*.rs')"; then
      :
    else
      status=$?
      [[ "$status" -eq 1 ]] || fail "git grep failed for definition '$definition' at $evidence_commit (exit $status)"
      matches=""
    fi

    printf '%s' "$matches" | jq -Rsc --arg definition "$definition" '{
      definition: $definition,
      literal_occurrences: (split("\n") | map(select(length > 0)))
    }' || return
  done
}

# Finish collection before writing the evidence document to stdout.
usages="$(collect_usages)"
printf '%s' "$usages" | jq -s \
  --arg target "$target" \
  --arg evidence_commit "$evidence_commit" \
  --arg commit_sha "$commit_sha" \
  --arg commit_date "$commit_date" \
  --arg commit_subject "$commit_subject" \
  '{
    target: $target,
    evidence_commit: $evidence_commit,
    tracked_rust_source: true,
    last_modifying_commit: {
      sha: $commit_sha,
      date: $commit_date,
      subject: $commit_subject
    },
    literal_definition_occurrences: .
  }'
