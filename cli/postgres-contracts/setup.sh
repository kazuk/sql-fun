#!/usr/bin/env bash

: "${SQL_FUN_BIN:?SQL_FUN_BIN must name the sql-fun executable}"
: "${SQL_FUN_TEST_DOCKER_HOST:?SQL_FUN_TEST_DOCKER_HOST must name the Docker host address}"
: "${SQL_FUN_TEST_DOCKER_BIND:?SQL_FUN_TEST_DOCKER_BIND must name the Docker bind address}"
test -x "$SQL_FUN_BIN"

function sql-fun() {
  "$SQL_FUN_BIN" "$@"
}

# This catalog contract retains the canonical completed-home theme offline.
export SQL_FUN_TEST_THEME="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/sql-fun-home/highlighter/default.toml"
