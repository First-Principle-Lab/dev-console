#!/usr/bin/env bash
# C15 cloud substitute: exercise the real X11 copy path (a genuine selection owner, not a
# stub) under Xvfb. Proves paste-back, expiry retraction, and that a lock never clears
# content the user copied afterwards.
#
# What it does NOT prove: that a clipboard history manager honours anything — none runs here,
# which is exactly why the app reports exclusion as unavailable.
#
# Exit 77 means UNVERIFIED (no Xvfb, no xclip).
set -euo pipefail
cd "$(dirname "$0")/.."

export HOME="${HOME:-/root}"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export PATH="$CARGO_HOME/bin:$PATH"

for tool in xvfb-run xclip; do
  command -v "$tool" >/dev/null || {
    echo "UNVERIFIED: missing dependency: $tool"
    exit 77
  }
done

echo "== clipboard provider: $(command -v xclip) on $(command -v xvfb-run) =="
xvfb-run -a cargo test --manifest-path src-tauri/Cargo.toml --locked --test clipboard-x11 -- \
  --ignored --test-threads=1 --nocapture
code=$?

if [[ "$code" == "0" ]]; then
  echo "RESULT: PASS (real X11 selection through xclip)"
else
  echo "RESULT: exit $code"
fi
exit "$code"
