#!/usr/bin/env bash
# Manual harness: follow a real Hyprland focus event all the way to a project.
#
# What it proves, when it can run: that the focused window really is followed through the terminal
# to the shell it started, that the repository found is the one the harness runs in, and that the
# whole lookup stays inside the 30 ms the palette relies on to open without noticing.
#
# What it does NOT prove: any other compositor (there is nothing to read there, by design), nor a
# terminal whose shell is parented elsewhere, like tmux — both cases are "no context", which is the
# documented behaviour rather than something this harness can assert.
#
# Run it from the terminal window you are looking at:  bash scripts/test-project-context.sh
#
# Exit 77 means UNVERIFIED (no Hyprland session in this environment).
set -euo pipefail
cd "$(dirname "$0")/.."

export HOME="${HOME:-/root}"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export PATH="$CARGO_HOME/bin:$PATH"

if [[ -z "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
  echo "UNVERIFIED: no Hyprland session (HYPRLAND_INSTANCE_SIGNATURE is unset)"
  exit 77
fi

runtime="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
socket="$runtime/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock"
if [[ ! -S "$socket" ]]; then
  echo "UNVERIFIED: no Hyprland IPC socket at $socket"
  exit 77
fi

echo "== Hyprland session ${HYPRLAND_INSTANCE_SIGNATURE}: detecting from the focused window =="
code=0
cargo test --manifest-path src-tauri/Cargo.toml --locked --test project-context -- \
  --ignored --exact --nocapture the_focused_terminal_reports_its_project || code=$?

if [[ "$code" == "0" ]]; then
  echo "RESULT: PASS (focused window -> shell -> project, inside the 30 ms budget)"
else
  echo "RESULT: exit $code"
fi
exit "$code"
