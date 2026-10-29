# Temporary #799 suppressions for the shared source/installed runner.
# The catalog setup fails before any case in each affected group can run.
# Keep this separate from product test input parsing and cast resolution.
declare -A temporary_ignores=()
while IFS=$'\t' read -r fixture case_name versions issue; do
  [[ -z "$fixture" || "$fixture" == \#* ]] && continue
  [[ "$issue" == '#799' && -n "$case_name" && "$fixture" =~ ^[a-z0-9_-]+$ ]] || {
    echo 'Invalid temporary ignore; each entry must identify a case and #799' >&2
    exit 1
  }
  IFS=, read -ra ignored_versions <<< "$versions"
  for version in "${ignored_versions[@]}"; do
    [[ "$version" == 17 || "$version" == 18 ]] || exit 1
    key="$fixture/$version/$case_name"
    [[ -z "${temporary_ignores[$key]+x}" ]] || {
      echo "Duplicate temporary ignore: $key" >&2
      exit 1
    }
    temporary_ignores["$key"]="$issue"
  done
done <"$suite/temporary-ignores.tsv"

function ignore_catalog_group() {
  local fixture=$1 version=$2 file=$3 count=$4
  local case_name key ignored=0
  local -a names
  mapfile -t names < <(jq -r '.[].name' "$file")
  for case_name in "${names[@]}"; do
    key="$fixture/$version/$case_name"
    if [[ -n "${temporary_ignores[$key]+x}" ]]; then
      ignored=$((ignored + 1))
      unset 'temporary_ignores[$key]'
    fi
  done
  (( ignored > 0 )) || return 1
  # #799 blocks catalog initialization, not a subset of query assertions.
  # Do not silently ignore newly added cases or broaden a partial inventory.
  if (( ignored != count )); then
    echo "Incomplete #799 catalog-setup ignore inventory: $fixture PostgreSQL $version ($ignored/$count)" >&2
    exit 1
  fi
  {
    echo '<?xml version="1.0" encoding="UTF-8"?>'
    echo "<testsuites><testsuite name=\"sql-fun test\" tests=\"$count\" failures=\"0\" errors=\"0\" skipped=\"$count\">"
    jq -r --arg file "$file" '.[] |
      "<testcase name=\"" + (($file + "::" + .name) | @html) + "\"><skipped message=\"#799: shipped information_schema domain/base comparison prevents catalog initialization\"/></testcase>"' "$file"
    echo '</testsuite></testsuites>'
  } >"$reports/$fixture-$version.xml"
  {
    for case_name in "${names[@]}"; do
      echo "SKIP $file::$case_name: #799 (PostgreSQL $version catalog initialization)"
    done
    echo "== Result: $fixture PostgreSQL $version: SKIP; cases=0/$count; ignored=$count; issue=#799"
  } | tee "$reports/$fixture-$version.log"
}
