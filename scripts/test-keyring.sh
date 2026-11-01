#!/usr/bin/env bash
# C11 cloud substitute: drive the vault keyring adapter against a real Secret Service
# provider (gnome-keyring-daemon) on a private session bus in a disposable profile.
#
# What this proves: the adapter speaks the real org.freedesktop.secrets protocol, the
# randomly generated vault key survives process restarts, and the encrypted vault can be
# reopened through a keyring-held credential with no app master password.
#
# What it does NOT prove: that the owner's login keyring unlocks at graphical session start
# on Arch/Hyprland. That remains a desktop-only check.
set -euo pipefail
cd "$(dirname "$0")/.."

export HOME="${HOME:-/root}"
# The daemon runs with an isolated HOME; cargo must keep using the real toolchain.
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"

for tool in dbus-run-session gnome-keyring-daemon secret-tool; do
  command -v "$tool" >/dev/null || { echo "missing dependency: $tool" >&2; exit 1; }
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/run" "$WORK/data" "$WORK/config"
chmod 700 "$WORK/run"

set +e
XDG_RUNTIME_DIR="$WORK/run" \
XDG_DATA_HOME="$WORK/data" \
XDG_CONFIG_HOME="$WORK/config" \
HOME="$WORK" \
  dbus-run-session -- bash scripts/_keyring-session.sh
code=$?
set -e

if [ "$code" -eq 77 ]; then
  echo "RESULT: UNVERIFIED (exit 77) — see the reason above"
else
  echo "RESULT: exit $code"
fi
exit "$code"
