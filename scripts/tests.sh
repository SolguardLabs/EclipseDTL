#!/usr/bin/env bash
set -euo pipefail

resolve_cargo() {
  if command -v cargo >/dev/null 2>&1; then
    command -v cargo
    return
  fi
  if [[ -n "${CARGO_HOME:-}" && -x "${CARGO_HOME}/bin/cargo.exe" ]]; then
    printf '%s\n' "${CARGO_HOME}/bin/cargo.exe"
    return
  fi
  if [[ -n "${HOME:-}" && -x "${HOME}/.cargo/bin/cargo.exe" ]]; then
    printf '%s\n' "${HOME}/.cargo/bin/cargo.exe"
    return
  fi
  for candidate in /mnt/c/Users/*/.cargo/bin/cargo.exe; do
    if [[ -x "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return
    fi
  done
  printf '%s\n' cargo
}

resolve_node() {
  if command -v node >/dev/null 2>&1; then
    command -v node
    return
  fi
  if [[ -x "/mnt/c/Program Files/nodejs/node.exe" ]]; then
    printf '%s\n' "/mnt/c/Program Files/nodejs/node.exe"
    return
  fi
  printf '%s\n' node
}

CARGO_BIN="$(resolve_cargo)"
NODE_BIN="$(resolve_node)"

"${CARGO_BIN}" test --locked
ECLIPSEDTL_CARGO="${CARGO_BIN}" "${NODE_BIN}" --test "tests/node/*.test.js"
