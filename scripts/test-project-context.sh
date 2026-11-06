#!/usr/bin/env bash
# Native harness for the project-context feature. Under a real Hyprland session it opens a
# terminal running a shell inside a fresh git repository (Hyprland focuses new windows), then
# runs the ignored Rust tests that (a) resolve exactly that repository through the real
# compositor socket and the real /proc, and (b) assert the detection pass stays inside the
# 30 ms budget.
#
# What it does NOT prove: that the settings gate works from the GUI, or timing on a loaded
# machine.
#
# Exit 77 means UNVERIFIED (not a Hyprland session, no usable terminal).
set -euo pipefail
cd "$(dirname "$0")/.."

export HOME="${HOME:-/root}"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export PATH="$CARGO_HOME/bin:$PATH"

if [[ -z "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
  echo "UNVERIFIED: not running under Hyprland (HYPRLAND_INSTANCE_SIGNATURE unset)"
  exit 77
fi
socket="${XDG_RUNTIME_DIR:-/tmp}/hypr/${HYPRLAND_INSTANCE_SIGNATURE}/.socket.sock"
if [[ ! -S "$socket" ]]; then
  echo "UNVERIFIED: no Hyprland IPC socket at $socket"
  exit 77
fi
command -v hyprctl >/dev/null || { echo "UNVERIFIED: hyprctl is not installed"; exit 77; }

# The shell choice mirrors the SHELL_NAMES list in context.rs; the terminal must be able to
# start in a chosen directory.
shell=""
for candidate in zsh bash fish nu; do
  command -v "$candidate" >/dev/null && shell="$candidate" && break
done
terminal_launch=""
if [[ -n "$shell" ]] && command -v kitty >/dev/null; then
  terminal_launch="kitty --directory {REPO} $shell"
elif [[ -n "$shell" ]] && command -v alacritty >/dev/null; then
  terminal_launch="alacritty --working-directory {REPO} -e $shell"
elif [[ -n "$shell" ]] && command -v foot >/dev/null; then
  terminal_launch="foot --working-directory={REPO} $shell"
else
  echo "UNVERIFIED: no usable terminal (kitty/alacritty/foot) with a known shell (zsh/bash/fish/nu)"
  exit 77
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
repo="$work/ctx-harness-proj"
mkdir -p "$repo"
git -C "$repo" init -q
git -C "$repo" commit -q --allow-empty -m "seed" >/dev/null 2>&1 || true

launch="${terminal_launch//\{REPO\}/$repo}"
echo "== opening a focused terminal: $launch"
hyprctl dispatch exec "$launch" >/dev/null
# Give the window time to map and take focus, and the shell time to settle.
sleep 2

if ! focused_json="$(hyprctl activewindow -j)"; then
  echo "UNVERIFIED: hyprctl could not report the active window"
  exit 77
fi
echo "== focused window: $(printf '%s' "$focused_json" | head -c 200)"

echo "== detection must resolve $repo through the live compositor and /proc =="
PROJECT_CONTEXT_EXPECT_ROOT="$repo" \
  cargo test --manifest-path src-tauri/Cargo.toml --locked --lib context::tests::resolves_the_focused_repository -- \
  --ignored --nocapture

echo "== detection timing budget =="
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib context::tests::detection_under_the_timing_budget -- \
  --ignored --nocapture

# Close the harness terminal (best effort; the window belongs to the compositor now).
harness_pid="$(printf '%s' "$focused_json" | sed -n 's/.*"pid": *\([0-9]*\).*/\1/p')"
if [[ -n "${harness_pid:-}" ]]; then
  hyprctl dispatch closewindow pid:"$harness_pid" >/dev/null 2>&1 || true
fi

echo "project-context harness passed"
