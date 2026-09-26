#!/usr/bin/env bash
set -euo pipefail

if (( $# < 2 || $# > 3 )); then
  echo "usage: $0 <https-source-url> <repository-relative-destination> [source-revision]" >&2
  exit 2
fi

source_url="$1"
destination="$2"
source_revision="${3:-}"

if [[ ! "$source_url" =~ ^https://[^[:space:]]+$ ]]; then
  echo "source URL must be an HTTPS URL without whitespace" >&2
  exit 2
fi
if [[ -n "$source_revision" && ! "$source_revision" =~ ^[[:alnum:]._-]+$ ]]; then
  echo "source revision contains unsupported characters" >&2
  exit 2
fi

repo_root="$(git rev-parse --show-toplevel)"
if [[ "$destination" = /* || "$destination" == *$'\n'* ]]; then
  echo "destination must be a repository-relative path" >&2
  exit 2
fi

resource_path="$(realpath -m "$repo_root/$destination")"
case "$resource_path" in
  "$repo_root"/*) ;;
  *)
    echo "destination must remain inside the repository" >&2
    exit 2
    ;;
esac
[[ -f "$resource_path" ]] || { echo "vendored resource does not exist: $destination" >&2; exit 1; }

notice_path="${resource_path}.license-notice.md"
[[ ! -e "$notice_path" ]] || { echo "notice scaffold already exists: ${notice_path#$repo_root/}" >&2; exit 1; }

detected_license="$(sed -n '1,80{s/^[[:space:]]*\(\/\/\|#\|\/\*\|\*\)[[:space:]]*SPDX-License-Identifier:[[:space:]]*//p}' "$resource_path" | head -n 1)"
detected_license="${detected_license:-none}"

template_path="$repo_root/.codex/skills/vendor-third-party/assets/third_party_license_notice.md.tera"
command -v tera >/dev/null || { echo "tera from the tera-cli package is required" >&2; exit 1; }

SOURCE_URL="$source_url" \
SOURCE_REVISION="$source_revision" \
RESOURCE_PATH="${resource_path#$repo_root/}" \
CONTENT_SHA256="$(sha256sum "$resource_path" | awk '{print $1}')" \
DETECTED_LICENSE="$detected_license" \
tera --file "$template_path" --env > "$notice_path"
