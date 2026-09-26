#!/usr/bin/env bash
set -euo pipefail

if (( $# != 2 )); then
  echo "usage: $0 <https-source-url> <repository-relative-destination>" >&2
  exit 2
fi

source_url="$1"
destination="$2"

if [[ ! "$source_url" =~ ^https://[^[:space:]]+$ ]]; then
  echo "source URL must be an HTTPS URL without whitespace" >&2
  exit 2
fi

repo_root="$(git rev-parse --show-toplevel)"
if [[ "$destination" = /* || "$destination" == *$'\n'* ]]; then
  echo "destination must be a repository-relative path" >&2
  exit 2
fi

destination_path="$(realpath -m "$repo_root/$destination")"
case "$destination_path" in
  "$repo_root"/*) ;;
  *)
    echo "destination must remain inside the repository" >&2
    exit 2
    ;;
esac

mkdir -p "$(dirname "$destination_path")"
temporary_path="${destination_path}.tmp.$$"
trap 'rm -f "$temporary_path"' EXIT
curl --fail --location --proto '=https' --tlsv1.2 --output "$temporary_path" "$source_url"
mv "$temporary_path" "$destination_path"
