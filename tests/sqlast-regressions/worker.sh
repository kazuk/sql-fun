#!/usr/bin/env bash
# Private line-oriented worker. The coordinator owns stdin, namespace and server.
set -euo pipefail
suite=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "$suite/operations.sh"
runtime=$1
reports=$2
mkdir -p "$runtime" "$reports"
export LC_ALL=C
TIMEFORMAT='%3R'
total=0 failed=0 groups=0 test_seconds=0
while IFS=$'\t' read -r fixture level schema count version; do
  file="$suite/$fixture.json"
  # Reserve stdout for the completion protocol; keep all human output in logs.
  run_group >>"$reports/worker.log" 2>&1
  jq -cn --arg fixture "$fixture" --arg version "$version" \
    --argjson status "$status" --argjson elapsed "$elapsed" \
    '{fixture:$fixture,version:$version,status:$status,seconds:$elapsed}'
done
exit "$failed"
