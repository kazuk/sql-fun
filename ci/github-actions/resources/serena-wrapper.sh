#!/usr/bin/env bash
set -euo pipefail

SERENA_HOME_DIR="${SERENA_HOME_DIR:-$HOME/serena}"
SERENA_TARBALL="${SERENA_TARBALL:-/opt/serena.tar.gz}"
SERENA_MARKER="${SERENA_HOME_DIR}/.installed"

install_serena() {
  rm -rf "${SERENA_HOME_DIR}"
  mkdir -p "${SERENA_HOME_DIR}"
  tar -xzf "${SERENA_TARBALL}" -C "${SERENA_HOME_DIR}" --strip-components=1
  : > "${SERENA_MARKER}"
}

if [ ! -f "${SERENA_MARKER}" ]; then
  install_serena

  uv run --project "${SERENA_HOME_DIR}" serena project generate-yml --language rust
fi

exec uv run --project "${SERENA_HOME_DIR}" serena "$@"
