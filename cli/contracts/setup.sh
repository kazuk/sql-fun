#!/usr/bin/env bash

: "${SQL_FUN_BIN:?SQL_FUN_BIN must name the sql-fun executable}"
test -x "$SQL_FUN_BIN"

function sql-fun() {
  "$SQL_FUN_BIN" "$@"
}
