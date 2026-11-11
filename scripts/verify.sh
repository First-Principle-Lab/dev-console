#!/usr/bin/env bash
# Single documented verification entrypoint. Fails on the first failing check.
#
# SKIP_BUN_INSTALL=1     reuse the existing node_modules instead of installing
# SKIP_KEYRING_TEST=1  do not try to start a Secret Service provider
# SKIP_RELEASE_BUILD=1 skip the native release compile and AppImage (slow on small hosts)
set -euo pipefail
cd "$(dirname "$0")/.."

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export PATH="$CARGO_HOME/bin:$PATH"

export BUN_INSTALL="${BUN_INSTALL:-$HOME/.bun}"
export PATH="$BUN_INSTALL/bin:$PATH"

echo "== bun install --frozen-lockfile (set SKIP_BUN_INSTALL=1 to reuse node_modules) =="
if [[ "${SKIP_BUN_INSTALL:-0}" == "1" ]]; then
  [[ -d node_modules ]] || bun install --frozen-lockfile
else
  bun install --frozen-lockfile
fi

echo "== svelte-check =="
bun run check
echo "== eslint =="
bun run lint
echo "== frontend unit tests =="
bun run test
echo "== frontend production build =="
bun run build:frontend
echo "== browser E2E (runs against the IPC test double: proves UI behaviour only) =="
bunx playwright test
echo "== cargo fmt --check =="
# The domain layer moved to the root Cargo workspace (`crates/dev-console-core`), so the gate covers
# both trees: the workspace and the standalone desktop package in `src-tauri`. `--workspace` is what
# runs the storage/vault/backup unit tests that used to live inside `src-tauri`, and it is what now
# compiles and tests the native shell too (`dev-console-desktop`, `dev-console-ui-tests`): those
# gates are headless by construction - the runtime is driven, never rendered - so they belong in the
# same entrypoint as the rest, and a check nothing runs is a check that can rot. The render harness
# is a different matter and still exits 77 (UNVERIFIED) on a host with no display.
cargo fmt --all -- --check
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
echo "== clippy (warnings denied) =="
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
echo "== cargo test =="
cargo test --workspace --locked
cargo test --manifest-path src-tauri/Cargo.toml --locked

echo "== zsh environment directory hook =="
python3 scripts/test-environment-hook.py

echo "== standalone AppImage installer =="
if command -v minisign >/dev/null; then
  python3 scripts/test-appimage-installer.py
else
  echo "UNVERIFIED: minisign is required for the AppImage installer tests"
fi

echo "== keyring integration against a real Secret Service provider =="
if [[ "${SKIP_KEYRING_TEST:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_KEYRING_TEST=1)"
else
  code=0
  bash scripts/test-keyring.sh || code=$?
  if [[ "$code" == "77" ]]; then
    # The contract forbids turning a missing provider into a pass; report it instead.
    echo "UNVERIFIED: no Secret Service provider could be started in this environment"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "== clipboard behaviour against a real X11 selection =="
if [[ "${SKIP_CLIPBOARD_TEST:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_CLIPBOARD_TEST=1)"
else
  code=0
  bash scripts/test-clipboard.sh || code=$?
  if [[ "$code" == "77" ]]; then
    echo "UNVERIFIED: no Xvfb/xclip in this environment"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "== project detection against a live Hyprland focus event =="
if [[ "${SKIP_PROJECT_CONTEXT_TEST:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_PROJECT_CONTEXT_TEST=1)"
else
  code=0
  bash scripts/test-project-context.sh || code=$?
  if [[ "$code" == "77" ]]; then
    # The harness printed why (no Hyprland, wrong window focused, or - always, from here - a shell
    # of verify.sh's own between the terminal and the probe). Run it directly at a prompt to prove it.
    echo "UNVERIFIED: live project detection (see the reason above; run scripts/test-project-context.sh by hand)"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "== packaging: staged install, desktop file, polkit policy, exact uninstall =="
if [[ "${SKIP_PACKAGING_TEST:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_PACKAGING_TEST=1)"
else
  code=0
  bash scripts/test-packaging.sh || code=$?
  if [[ "$code" == "77" ]]; then
    echo "UNVERIFIED: desktop-file-validate is not installed"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "== arch packaging: PKGBUILD layout, installed paths, policy XML =="
if [[ "${SKIP_ARCH_TEST:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_ARCH_TEST=1)"
else
  code=0
  bash scripts/test-arch-package.sh || code=$?
  if [[ "$code" == "77" ]]; then
    echo "UNVERIFIED: the Arch package build did not run here (the static half passed)"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "== native backend integration (real provider, real files) =="
if [[ "${SKIP_NATIVE_IPC_TEST:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_NATIVE_IPC_TEST=1)"
else
  code=0
  bash scripts/test-native-ipc.sh || code=$?
  if [[ "$code" == "77" ]]; then
    echo "UNVERIFIED: no Secret Service provider could be started here"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "== native release compile, AppImage and command-line smoke =="
# What a release ships: the Rune shell from the root workspace, packed by the pinned tooling and
# started from the image. The Tauri binary is no longer compiled here because it is no longer
# published; its tests above still run until the cutover removes the tree.
if [[ "${SKIP_RELEASE_BUILD:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_RELEASE_BUILD=1)"
else
  cargo build --release --locked -p dev-console-desktop
  bash scripts/build-appimage.sh
  code=0
  bash scripts/smoke-native-cli.sh target/appimage/Dev-Console.AppImage || code=$?
  if [[ "$code" == "77" ]]; then
    echo "UNVERIFIED: no AppImage to smoke-test"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "== .deb artifact: build, checksum, extract, validate =="
if [[ "${SKIP_DEB_TEST:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_DEB_TEST=1)"
else
  code=0
  bash scripts/build-deb.sh || code=$?
  if [[ "$code" == "77" ]]; then
    echo "UNVERIFIED: dpkg-deb is not available here"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "all checks passed"
