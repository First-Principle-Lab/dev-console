#!/usr/bin/env bash
# Started by scripts/test-keyring.sh inside a private session bus with a real
# Secret Service provider. Never run this directly.
set -euo pipefail
cd "$(dirname "$0")/.."

export PATH="${CARGO_HOME:-${HOME:-/root}/.cargo}/bin:$PATH"

# Started with an empty master password so the in-memory "session" collection exists without
# a GUI prompter; this is the collection a real session would also hand out.
printf '' | gnome-keyring-daemon --daemonize --components=secrets --unlock >/dev/null 2>&1 || true
sleep 1

if ! dbus-send --session --print-reply --dest=org.freedesktop.DBus /org/freedesktop/DBus \
    org.freedesktop.DBus.ListNames 2>/dev/null | grep -q org.freedesktop.secrets; then
  echo "UNVERIFIED: gnome-keyring-daemon did not register org.freedesktop.secrets in this"
  echo "            container. Rerun scripts/test-keyring.sh somewhere the daemon can start,"
  echo "            or on the target desktop."
  exit 77
fi
echo "provider registered: org.freedesktop.secrets"

python3 scripts/keyring-default-alias.py || exit $?

echo "== libsecret sanity check through the same provider =="
printf 'synthetic-probe-value' | secret-tool store --label='Dev Console probe' \
  service dev-console-probe account probe-key
secret-tool lookup service dev-console-probe account probe-key | grep -q synthetic-probe-value
echo "secret-tool store and lookup OK"

echo "== Dev Console keyring adapter (ignored tests: they need a real provider) =="
# Only the library's own provider probe runs here. The X11 clipboard and native-IPC suites are
# ignored for their own reasons and are run by their own harnesses, which can supply the display
# and the disposable profile they need.
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib keyring::tests::real_keyring_round_trip -- \
  --nocapture --ignored

exit 0
