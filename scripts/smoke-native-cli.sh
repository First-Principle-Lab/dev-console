#!/usr/bin/env bash
# Command-line smoke test for the native binary or its AppImage, on a host with no display.
#
#   scripts/smoke-native-cli.sh [path/to/dev-console | path/to/Dev-Console.AppImage]
#
# Proves the installed command line still answers as documented when no instance is running:
# version and usage, an unknown flag that starts nothing, and a manual vault lock that persists in
# a private file and is reported by `vault status`. It runs against a disposable profile with no
# session bus, so it never reads a real keyring, vault or library. What needs a window is the
# business of scripts/test-rune-desktop.py on a live session.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="$(realpath "${1:-target/release/dev-console}")"
[[ -x "$BIN" ]] || { echo "UNVERIFIED: no executable at $BIN" >&2; exit 77; }
RUN=("$BIN")
# An AppImage mounts itself with FUSE, which a CI container may not offer.
[[ "$BIN" == *.AppImage ]] && RUN+=(--appimage-extract-and-run)

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/home" "$WORK/run"
chmod 700 "$WORK/run"

app() {
  env -u DBUS_SESSION_BUS_ADDRESS -u WAYLAND_DISPLAY -u DISPLAY \
    HOME="$WORK/home" XDG_RUNTIME_DIR="$WORK/run" \
    DEV_CONSOLE_DATA_DIR="$WORK/data" DEV_CONSOLE_CONFIG_DIR="$WORK/config" \
    DEV_CONSOLE_RUNTIME_DIR="$WORK/run/dev-console" "${RUN[@]}" "$@"
}
fail() { echo "FAIL: $*" >&2; exit 1; }
# expect <exit code> <args...>: runs the command, leaves its output in $out.
expect() {
  local want="$1" code=0
  shift
  out="$(app "$@" 2>&1)" || code=$?
  [[ "$code" == "$want" ]] || fail "'$*' exited $code, expected $want: $out"
}

expect 0 --version
[[ "$out" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "--version printed '$out'"
version="$out"

expect 0 --help
[[ "$out" == *"dev-console vault lock"* && "$out" == *"dev-console --finder"* ]] \
  || fail "--help does not list the documented commands"
expect 0 --no-such-flag
[[ "$out" == *"Usage:"* ]] || fail "an unknown flag did not print the usage"

# 30: done, but by this process, because nothing is resident.
expect 30 vault lock
flag="$WORK/config/manual-lock"
[[ -f "$flag" ]] || fail "vault lock left no flag at $flag"
[[ "$(stat -c %a "$flag")" == "600" ]] || fail "the lock flag is not private"
[[ "$(stat -c %a "$WORK/config")" == "700" ]] || fail "the profile directory is not private"
expect 10 vault status
[[ "$out" == "locked (manual)" ]] || fail "vault status printed '$out'"
expect 30 vault unlock
[[ -f "$flag" ]] || fail "an unauthenticated unlock removed the manual lock"
expect 30 import project "$WORK"

echo "NATIVE CLI SMOKE OK ($BIN, $version)"
