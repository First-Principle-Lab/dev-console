#!/usr/bin/env bash
# C02: produce an installable .deb with a SHA-256 checksum, extract it into a clean
# filesystem root, and prove the *packaged* binary is what the command-line smoke runs against.
#
# What it does NOT prove: that a distro package manager resolves dependencies or that a
# real desktop launcher/icon cache behaves - those stay desktop checks.
#
# Exit 77 means UNVERIFIED (dpkg-deb unavailable on this host).
set -euo pipefail
cd "$(dirname "$0")/.."

command -v dpkg-deb >/dev/null || {
  echo "UNVERIFIED: dpkg-deb is not installed"
  exit 77
}

BINARY="${DEV_CONSOLE_BINARY:-target/release/dev-console}"
[[ -x "$BINARY" ]] || {
  echo "FAIL: no release binary at $BINARY (run 'cargo build --release --locked -p dev-console-desktop' first)"
  exit 1
}
VERSION="$("$BINARY" --version)"

ARCH="$(dpkg --print-architecture)"
NAME="dev-console_${VERSION}_${ARCH}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
ROOTFS="$WORK/rootfs"
mkdir -p "$ROOTFS/DEBIAN"

DESTDIR="$ROOTFS" PREFIX=/usr BINARY="$PWD/$BINARY" bash packaging/install.sh install

cat >"$ROOTFS/DEBIAN/control" <<EOF
Package: dev-console
Version: $VERSION
Architecture: $ARCH
Maintainer: Dev Console maintainers
Section: utils
Priority: optional
Description: Encrypted command and secret library with a keyboard-first finder
 Native desktop app: SQLite command library, age-encrypted vault backed by the
 OS keyring, clipboard handling with expiry, CLI control channel, and a finder
 window designed for a global shortcut.
EOF

mkdir -p dist
deb="dist/${NAME}.deb"
dpkg-deb --root-owner-group --build "$ROOTFS" "$deb"
(cd dist && sha256sum "$(basename "$deb")" >"$(basename "$deb").sha256" && cat "$(basename "$deb").sha256")

echo "== extract into a clean filesystem root and validate the placement =="
PREFIXDIR="$WORK/prefix"
mkdir -p "$PREFIXDIR"
dpkg-deb -x "$deb" "$PREFIXDIR"
test -x "$PREFIXDIR/usr/bin/dev-console"
[[ "$(stat -c '%a' "$PREFIXDIR/usr/bin/dev-console")" == "755" ]]
desktop-file-validate "$PREFIXDIR/usr/share/applications/dev-console.desktop"
test -f "$PREFIXDIR/usr/share/polkit-1/actions/ai.codemate.devconsole.policy"
dpkg-deb -I "$deb" | grep -q "Package: dev-console"
echo "ok: package metadata, binary mode, desktop entry and polkit policy all check out"

echo "== native smoke against the binary extracted from the package =="
if [[ "${SKIP_NATIVE_SMOKE:-0}" == "1" ]]; then
  echo "skipped by request (SKIP_NATIVE_SMOKE=1)"
else
  bash scripts/smoke-native-cli.sh "$PREFIXDIR/usr/bin/dev-console"
fi

echo "DEB ARTIFACT OK: $deb"
