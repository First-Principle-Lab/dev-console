#!/usr/bin/env bash
# Native smoke test for the compiled release binary. Proves the parts a browser test double
# cannot: the real Tauri process starts, the control socket answers, a running instance
# performs the manual lock, the lock survives a restart, locking fails closed without an
# authentication mechanism, the single-instance handoff works, and app data lands with private
# permissions.
#
#   scripts/smoke-native.sh [path/to/dev-console]
#
# Exit 77 means the environment could not host the test (missing Xvfb/dbus/binary).
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${1:-/tmp/dc-release/release/dev-console}"
for tool in dbus-run-session xvfb-run; do
  command -v "$tool" >/dev/null || { echo "missing dependency: $tool" >&2; exit 77; }
done
[[ -x "$BIN" ]] || {
  echo "UNVERIFIED: no release binary at $BIN — run 'bunx tauri build --no-bundle' first" >&2
  exit 77
}

WORK="$(mktemp -d)"
cleanup() {
  pkill -P $$ 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT
mkdir -p "$WORK/run" "$WORK/data" "$WORK/config" "$WORK/cache"
chmod 700 "$WORK/run" "$WORK/data" "$WORK/config"

export HOME="$WORK"
export XDG_RUNTIME_DIR="$WORK/run"
export XDG_DATA_HOME="$WORK/data"
export XDG_CONFIG_HOME="$WORK/config"
export XDG_CACHE_HOME="$WORK/cache"
export CARGO_HOME="${CARGO_HOME:-/root/.cargo}"
export PATH="$CARGO_HOME/bin:$PATH"

dbus-run-session -- xvfb-run -a --server-args="-screen 0 1280x800x24" \
  bash scripts/_smoke-session.sh "$BIN" "$WORK"
code=$?
[[ "$code" == "0" ]] && echo "NATIVE SMOKE OK ($BIN)" || echo "NATIVE SMOKE FAILED (exit $code)"
exit "$code"
