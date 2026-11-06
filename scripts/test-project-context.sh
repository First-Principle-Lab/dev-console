#!/usr/bin/env bash
# Manual harness: follow a real Hyprland focus event all the way to a project.
#
# What it proves, when it can run: that the focused window really is followed through the terminal
# to the shell it started, that the repository found is the one the harness was started in, that
# the whole lookup stays inside the 30 ms the palette relies on, and - when this branch's build of
# Dev Console is running - that a real palette open (`window.finder`, the same path as Super+F)
# names that repository.
#
# What it does NOT prove: any other compositor (there is nothing to read there, by design), nor a
# terminal whose shell is parented elsewhere, like tmux - both cases are "no context", which is the
# documented behaviour rather than something this harness can assert.
#
# Run it from an idle prompt in the terminal window you are looking at, inside the checkout:
#   bash scripts/test-project-context.sh
#
# Exit 77 means UNVERIFIED: no Hyprland session, the focused window is not the terminal running
# this, the harness was started through another shell (verify.sh), or no running Dev Console
# answers `finder.context`. Each says which.
set -euo pipefail
start_dir="$PWD"
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

if ! expected="$(git -C "$start_dir" rev-parse --show-toplevel 2>/dev/null)"; then
  echo "UNVERIFIED: started from $start_dir, which is not inside a git checkout"
  exit 77
fi

# Built before anything looks at focus, so a long compile cannot be what moved it.
echo "== building the harness =="
cargo test --manifest-path src-tauri/Cargo.toml --locked --test project-context --no-run

# `exec`: this bash is itself a shell under the terminal, and the youngest one. Replacing it with
# the probe leaves the user's own prompt as the shell detection has to find.
exec python3 -I scripts/project-context-probe.py "$socket" "$expected"
