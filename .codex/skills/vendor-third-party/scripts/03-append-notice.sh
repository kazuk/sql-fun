#!/usr/bin/env bash
set -euo pipefail

replace=false
if [[ "${1:-}" == "--replace" ]]; then
  replace=true
  shift
fi

if (( $# != 1 )); then
  echo "usage: $0 [--replace] <repository-relative-notice-concept>" >&2
  exit 2
fi

concept_argument="$1"
repo_root="$(git rev-parse --show-toplevel)"
if [[ "$concept_argument" = /* || "$concept_argument" == *$'\n'* ]]; then
  echo "concept path must be a repository-relative path" >&2
  exit 2
fi

concept_path="$(realpath -m "$repo_root/$concept_argument")"
case "$concept_path" in
  "$repo_root"/*) ;;
  *)
    echo "concept path must remain inside the repository" >&2
    exit 2
    ;;
esac
[[ -f "$concept_path" ]] || { echo "concept does not exist: $concept_argument" >&2; exit 1; }
awk '
  $0 == "automated_verification:" { in_verification = 1; next }
  in_verification && $0 == "  status: completed" { completed = 1; exit }
  in_verification && /^[^[:space:]]/ { in_verification = 0 }
  END { exit !completed }
' "$concept_path" || {
  echo "concept must have completed automated verification before appending a notice" >&2
  exit 1
}

notice_text="$(awk '
  /^<!-- BEGIN THIRD PARTY NOTICE -->$/ { collecting = 1; next }
  /^<!-- END THIRD PARTY NOTICE -->$/ { collecting = 0; found = 1; exit }
  collecting { print }
  END { if (!found) exit 1 }
' "$concept_path")" || {
  echo "concept does not contain complete notice markers" >&2
  exit 1
}
[[ -n "$notice_text" ]] || { echo "notice text is empty" >&2; exit 1; }

notice_file="$repo_root/THIRD_PARTY_NOTICES.md"
relative_concept="$(realpath --relative-to="$(dirname "$notice_file")" "$concept_path")"
record_start="<!-- BEGIN THIRD PARTY NOTICE RECORD: $relative_concept -->"
record_end="<!-- END THIRD PARTY NOTICE RECORD: $relative_concept -->"
if [[ -f "$notice_file" ]] && grep -Fqx "$record_start" "$notice_file"; then
  if [[ "$replace" != true ]]; then
    echo "notice already appended for $relative_concept; use --replace to update it" >&2
    exit 1
  fi
  temporary_notice="$(mktemp)"
  awk -v start="$record_start" -v end="$record_end" '
    $0 == start { skipping = 1; next }
    skipping && $0 == end { skipping = 0; next }
    !skipping { print }
    END { if (skipping) exit 1 }
  ' "$notice_file" > "$temporary_notice" || {
    rm -f "$temporary_notice"
    echo "notice record has no matching end marker: $relative_concept" >&2
    exit 1
  }
  mv "$temporary_notice" "$notice_file"
elif [[ -f "$notice_file" ]] && grep -Fq "]($relative_concept)" "$notice_file"; then
  echo "notice already appended for $relative_concept" >&2
  exit 1
fi

if [[ ! -f "$notice_file" ]]; then
  printf '# Third-party notices\n' > "$notice_file"
fi

printf '\n%s\n\n## %s\n\n```text\n%s\n```\n\nSource record: [%s](%s)\n%s\n' \
  "$record_start" \
  "$(basename "${concept_path%.license-notice.md}")" \
  "$notice_text" \
  "${concept_path#$repo_root/}" \
  "$relative_concept" \
  "$record_end" >> "$notice_file"
