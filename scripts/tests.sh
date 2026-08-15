#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

resolve_cargo() {
  if command -v cargo >/dev/null 2>&1; then
    command -v cargo
    return
  fi
  if [[ -n "${CARGO_HOME:-}" && -x "${CARGO_HOME}/bin/cargo.exe" ]]; then
    printf '%s\n' "${CARGO_HOME}/bin/cargo.exe"
    return
  fi
  if [[ -n "${USERPROFILE:-}" && -x "${USERPROFILE}/.cargo/bin/cargo.exe" ]]; then
    printf '%s\n' "${USERPROFILE}/.cargo/bin/cargo.exe"
    return
  fi
  for candidate in /c/Users/*/.cargo/bin/cargo.exe /mnt/c/Users/*/.cargo/bin/cargo.exe; do
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
  for candidate in "/c/Program Files/nodejs/node.exe" "/mnt/c/Program Files/nodejs/node.exe"; do
    if [[ -x "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return
    fi
  done
  printf '%s\n' node
}

CARGO_BIN="${ECLIPSEDTL_CARGO:-$(resolve_cargo)}"
NODE_BIN="$(resolve_node)"
"${CARGO_BIN}" test --all-targets --locked
ECLIPSEDTL_CARGO="${CARGO_BIN}" "${NODE_BIN}" --test tests/node/*.test.js
