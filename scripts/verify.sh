#!/usr/bin/env bash
# Single documented verification entrypoint. Fails on the first failing check.
#
# SKIP_NPM_CI=1        reuse the existing node_modules instead of `npm ci`
# SKIP_KEYRING_TEST=1  do not try to start a Secret Service provider
# SKIP_RELEASE_BUILD=1 stop after the Rust tests (release compile is slow on small hosts)
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
echo "== browser E2E (runs against the IPC test double: proves UI behaviour only) =="
npx playwright test
echo "== cargo fmt --check =="
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
echo "== clippy (warnings denied) =="
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
echo "== cargo test =="
cargo test --manifest-path src-tauri/Cargo.toml --locked

echo "== keyring integration against a real Secret Service provider =="
if [[ "${SKIP_KEYRING_TEST:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_KEYRING_TEST=1)"
else
  code=0
  bash scripts/test-keyring.sh || code=$?
  if [[ "$code" == "77" ]]; then
    # The contract forbids turning a missing provider into a pass; report it instead.
    echo "UNVERIFIED: no Secret Service provider could be started in this environment"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "== release compile =="
if [[ "${SKIP_RELEASE_BUILD:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_RELEASE_BUILD=1)"
else
  npx tauri build --no-bundle
fi

echo "== native smoke against the compiled binary =="
if [[ "${SKIP_NATIVE_SMOKE:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_NATIVE_SMOKE=1)"
else
  binary="${DEV_CONSOLE_BINARY:-src-tauri/target/release/dev-console}"
  code=0
  bash scripts/smoke-native.sh "$binary" || code=$?
  if [[ "$code" == "77" ]]; then
    echo "UNVERIFIED: this environment cannot host the native smoke test"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "all checks passed"
