#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_LOCK_PATH="$ROOT_DIR/Cargo.lock"
TOOLS_DIR="$ROOT_DIR/var/cargo-tools"

resolve_wasm_bindgen_version() {
  awk '
    $0 == "[[package]]" { in_pkg = 0; next }
    $1 == "name" && $3 == "\"wasm-bindgen\"" { in_pkg = 1; next }
    in_pkg && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' "$CARGO_LOCK_PATH"
}

version="$(resolve_wasm_bindgen_version)"
if [[ -z "$version" ]]; then
  echo "could not resolve wasm-bindgen version from $CARGO_LOCK_PATH" >&2
  exit 1
fi

install_root="$TOOLS_DIR/wasm-bindgen-$version"
binary="$install_root/bin/wasm-bindgen"

if [[ ! -x "$binary" ]]; then
  mkdir -p "$TOOLS_DIR"
  cargo install --root "$install_root" wasm-bindgen-cli --version "$version" --locked
fi

export PATH="$install_root/bin:$PATH"
exec "$@"
