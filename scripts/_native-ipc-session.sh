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

# The provider-dependent tests are #[ignore]d so that a plain `cargo test` stays green on a host
# without a keyring; they run here for real. Each phase gets its own disposable profile, because
# the state paths are process-global and a manual lock left behind by one phase must not be
# visible to the next.
base="${DEV_CONSOLE_DATA_DIR:?expected by the harness}"
config_base="${DEV_CONSOLE_CONFIG_DIR:?expected by the harness}"
failures=0
for phase in default ignored; do
  export DEV_CONSOLE_DATA_DIR="$base-$phase"
  export DEV_CONSOLE_CONFIG_DIR="$config_base-$phase"
  mkdir -p "$DEV_CONSOLE_DATA_DIR" "$DEV_CONSOLE_CONFIG_DIR"
  chmod 700 "$DEV_CONSOLE_DATA_DIR" "$DEV_CONSOLE_CONFIG_DIR"
  args=(--test-threads=1)
  [[ "$phase" == "ignored" ]] && args+=(--ignored)
  echo "== native backend tests ($phase) =="
  if ! cargo test --manifest-path src-tauri/Cargo.toml --locked --test native-crud -- "${args[@]}"; then
    failures=$((failures + 1))
  fi
done
exit "$failures"
