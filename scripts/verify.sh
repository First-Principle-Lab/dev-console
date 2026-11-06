#!/usr/bin/env bash
# Single documented verification entrypoint. Fails on the first failing check.
#
# SKIP_BUN_INSTALL=1     reuse the existing node_modules instead of installing
# SKIP_KEYRING_TEST=1  do not try to start a Secret Service provider
# SKIP_RELEASE_BUILD=1 stop after the Rust tests (release compile is slow on small hosts)
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
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
echo "== clippy (warnings denied) =="
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
echo "== cargo test =="
cargo test --manifest-path src-tauri/Cargo.toml --locked

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
    echo "UNVERIFIED: makepkg is not available here (the static half still ran)"
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

echo "== project context: live detection under a real Hyprland session =="
if [[ "${SKIP_PROJECT_CONTEXT_TEST:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_PROJECT_CONTEXT_TEST=1)"
else
  code=0
  bash scripts/test-project-context.sh || code=$?
  if [[ "$code" == "77" ]]; then
    echo "UNVERIFIED: this session is not Hyprland with a usable terminal"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "== release compile =="
if [[ "${SKIP_RELEASE_BUILD:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_RELEASE_BUILD=1)"
else
  bunx tauri build --no-bundle
fi

echo "== native smoke against the compiled binary =="
if [[ "${SKIP_NATIVE_SMOKE:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_NATIVE_SMOKE=1)"
else
  binary="${DEV_CONSOLE_BINARY:-src-tauri/target/release/dev-console}"
  code=0
  bash scripts/smoke-native.sh "$binary" || code=$?
  if [[ "$code" == "77" ]]; then
    echo "UNVERIFIED: this environment cannot host the native smoke test"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "== .deb artifact: build, checksum, extract, validate =="
if [[ "${SKIP_DEB_TEST:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_DEB_TEST=1)"
else
  code=0
  SKIP_NATIVE_SMOKE=1 bash scripts/build-deb.sh || code=$?
  if [[ "$code" == "77" ]]; then
    echo "UNVERIFIED: dpkg-deb is not available here"
  elif [[ "$code" != "0" ]]; then
    exit "$code"
  fi
fi

echo "all checks passed"
