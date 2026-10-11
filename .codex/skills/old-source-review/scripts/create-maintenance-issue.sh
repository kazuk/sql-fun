#!/usr/bin/env bash

set -euo pipefail

readonly SKILL_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
readonly REPOSITORY_ROOT="$(git -C "$SKILL_DIRECTORY" rev-parse --show-toplevel)"
readonly ISSUE_TEMPLATE="$REPOSITORY_ROOT/.github/ISSUE_TEMPLATE/maintenance.md"
readonly KEEP_NOTICE="$SKILL_DIRECTORY/assets/keep-review-notice.md"

title=""
body_file=""
repository=""
repository_context=false
print_required_headings=false

usage() {
  printf '%s\n' 'usage: create-maintenance-issue.sh --required-headings'
  printf '%s\n' '   or: create-maintenance-issue.sh (--repository-context | --repo <owner/repo>) --title <title> --body-file <path>'
}

fail() {
  printf '%s\n' "$*" >&2
  exit 1
}

while (($# > 0)); do
  case "$1" in
    --required-headings)
      print_required_headings=true
      shift
      ;;
    --title)
      (($# >= 2)) || fail 'missing value for --title'
      title="$2"
      shift 2
      ;;
    --repository-context)
      repository_context=true
      shift
      ;;
    --repo)
      (($# >= 2)) || fail 'missing value for --repo'
      repository="$2"
      shift 2
      ;;
    --body-file)
      (($# >= 2)) || fail 'missing value for --body-file'
      body_file="$2"
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

mapfile -t required_headings < <(sed -n 's/^## /## /p' "$ISSUE_TEMPLATE")
((${#required_headings[@]} > 0)) || fail 'maintenance issue template has no required headings'

if "$print_required_headings"; then
  [[ -z "$title" && -z "$body_file" && -z "$repository" ]] && ! "$repository_context" \
    || fail '--required-headings cannot be combined with issue creation arguments'
  printf '%s\n' "${required_headings[@]}"
  printf '%s\n' '## Pending'
  exit 0
fi

[[ -n "$title" ]] || fail 'missing --title'
[[ -n "$body_file" ]] || fail 'missing --body-file'
if "$repository_context"; then
  [[ -z "$repository" ]] || fail '--repository-context cannot be combined with --repo'
  remote="$(git -C "$REPOSITORY_ROOT" remote get-url origin)" \
    || fail 'repository context requires an origin remote'
  case "$remote" in
    https://github.com/*) slug="${remote#https://github.com/}" ;;
    git@github.com:*) slug="${remote#git@github.com:}" ;;
    ssh://git@github.com/*) slug="${remote#ssh://git@github.com/}" ;;
    *) fail "unsupported repository origin: $remote" ;;
  esac
  slug="${slug%.git}"
  [[ "$slug" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] \
    || fail 'origin must identify one GitHub owner/repository'
  repository="$(gh repo view "https://github.com/$slug" --json nameWithOwner --jq .nameWithOwner)" \
    || fail 'could not resolve origin repository'
  [[ "$repository" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] \
    || fail 'repository resolution returned an invalid owner/repository'
fi
[[ -n "$repository" ]] || fail 'select --repository-context or supply --repo'
[[ -f "$body_file" ]] || fail "body file does not exist: $body_file"

for heading in "${required_headings[@]}" '## Pending'; do
  grep --fixed-strings --quiet --line-regexp -- "$heading" "$body_file" \
    || fail "body file is missing required heading: $heading"
done

if grep --fixed-strings --quiet -- '# Required follow-up for Keep' "$body_file"; then
  fail 'body file already includes the fixed Keep follow-up notice'
fi

temporary_body="$(mktemp)"
cleanup() {
  rm -f -- "$temporary_body"
}
trap cleanup EXIT

{
  cat -- "$body_file"
  printf '\n\n'
  cat -- "$KEEP_NOTICE"
  printf '\n'
} > "$temporary_body"

gh issue create --repo "$repository" \
  --title "$title" \
  --body-file "$temporary_body" \
  --label not-ready
