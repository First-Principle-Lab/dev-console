#!/usr/bin/env bash
# C04/C05/C08/C09 cloud substitute: run the backend integration test against a real Secret
# Service provider, a real Xvfb display, real SQLite files and a real age vault.
# Exit 77 means UNVERIFIED (no provider could be started).
set -euo pipefail
cd "$(dirname "$0")/.."

export HOME="${HOME:-/root}"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export PATH="$CARGO_HOME/bin:$PATH"

for tool in dbus-run-session xvfb-run gnome-keyring-daemon; do
  command -v "$tool" >/dev/null || { echo "UNVERIFIED: missing $tool"; exit 77; }
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/run" "$WORK/data" "$WORK/config" "$WORK/appdata" "$WORK/appconfig"
chmod 700 "$WORK/run" "$WORK/appdata" "$WORK/appconfig"

# HOME stays the real one: rustc needs its toolchain and registry environment intact, and the
# app profile is isolated through DEV_CONSOLE_* plus the XDG variables below.
XDG_RUNTIME_DIR="$WORK/run" XDG_DATA_HOME="$WORK/data" XDG_CONFIG_HOME="$WORK/config" \
DEV_CONSOLE_DATA_DIR="$WORK/appdata" DEV_CONSOLE_CONFIG_DIR="$WORK/appconfig" \
  dbus-run-session -- xvfb-run -a bash scripts/_native-ipc-session.sh
code=$?
echo "RESULT: exit $code"
exit "$code"
