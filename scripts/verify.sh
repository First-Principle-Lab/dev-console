#!/usr/bin/env bash
# Single documented verification entrypoint. Fails on the first failing check.
set -euo pipefail
cd "$(dirname "$0")/.."

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export PATH="$CARGO_HOME/bin:$PATH"

echo "== npm ci (set SKIP_NPM_CI=1 to reuse node_modules) =="
if [[ "${SKIP_NPM_CI:-0}" == "1" ]]; then
  [[ -d node_modules ]] || npm ci
else
  npm ci
fi

echo "== svelte-check =="
npm run check
echo "== eslint =="
npm run lint
echo "== frontend unit tests =="
npm run test
echo "== frontend production build =="
npm run build:frontend
echo "== cargo fmt --check =="
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
echo "== clippy (warnings denied) =="
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
echo "== cargo test =="
cargo test --manifest-path src-tauri/Cargo.toml --locked
echo "all checks passed"
