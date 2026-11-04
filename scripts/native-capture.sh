#!/usr/bin/env bash
# Native capture pass (checkpoint C9): launches the COMPILED RELEASE binary under a private
# dbus session + Xvfb, seeds synthetic commands (never secrets) into its own database, and
# captures the redesigned library and palette exactly as WebKitGTK paints them over a dark and
# a busy X11 background. Proves the opaque-shell material level on X11; true Wayland/Hyprland
# blur and compositor translucency cannot be proven here and stay a manual checklist
# (docs/raycast-ui-research-2026-11-02.md, section "Native material pass").
#
#   scripts/native-capture.sh [path/to/dev-console] [output-dir]
#
# Exit 77 means the environment could not host the pass (missing Xvfb/dbus/scrot/bun/probe
# build, or no release binary).
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${1:-src-tauri/target/release/dev-console}"
OUT="${2:-artifacts/ui-review/native-redesign}"
for tool in dbus-run-session xvfb-run scrot bun cc pkg-config; do
  command -v "$tool" >/dev/null || { echo "missing dependency: $tool" >&2; exit 77; }
done
pkg-config --exists x11 xtst || {
  echo "UNVERIFIED: X11/XTEST development headers are not installed" >&2
  exit 77
}
[[ -x "$BIN" ]] || {
  echo "UNVERIFIED: no release binary at $BIN - build it with the memory-tuned cargo command first" >&2
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
cc -O2 -Wall -Wextra scripts/native/xsession-probe.c -o "$WORK/xsession-probe" \
  $(pkg-config --cflags --libs x11 xtst)

export HOME="$WORK"
export XDG_RUNTIME_DIR="$WORK/run"
export XDG_DATA_HOME="$WORK/data"
export XDG_CONFIG_HOME="$WORK/config"
export XDG_CACHE_HOME="$WORK/cache"

mkdir -p "$OUT"
code=0
dbus-run-session -- xvfb-run -a --server-args="-screen 0 1280x800x24" \
  bash scripts/_native-capture-session.sh "$PWD/$BIN" "$WORK" \
  "$WORK/xsession-probe" "$OUT" || code=$?
[[ "$code" == "0" ]] && echo "NATIVE CAPTURE OK ($OUT)" || echo "NATIVE CAPTURE FAILED (exit $code)"
exit "$code"
