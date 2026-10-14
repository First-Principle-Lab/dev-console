#!/usr/bin/env bash
# Started by scripts/test-native-ipc.sh. Never run directly.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${CARGO_HOME:-/root/.cargo}/bin:$PATH"

printf '' | gnome-keyring-daemon --daemonize --components=secrets --unlock >/dev/null 2>&1 || true
sleep 1
if ! dbus-send --session --print-reply --dest=org.freedesktop.DBus /org/freedesktop/DBus \
    org.freedesktop.DBus.ListNames 2>/dev/null | grep -q org.freedesktop.secrets; then
  echo "UNVERIFIED: no Secret Service provider on this bus"
  exit 77
fi
python3 scripts/keyring-default-alias.py || exit $?

cargo test --manifest-path src-tauri/Cargo.toml --locked --test native-crud -- \
  --test-threads=1 --nocapture
