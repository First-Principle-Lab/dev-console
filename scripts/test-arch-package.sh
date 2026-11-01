#!/usr/bin/env bash
# Arch packaging check: validate packaging/arch/PKGBUILD statically, and build it for real when
# makepkg exists. Exit 77 (UNVERIFIED) when this host cannot build Arch packages at all.
set -euo pipefail
cd "$(dirname "$0")/.."

PKG=packaging/arch/PKGBUILD
[[ -f "$PKG" ]] || { echo "FAIL: $PKG is missing"; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0
note() { printf '   %s\n' "$*"; }
ok()   { printf 'ok   %s\n' "$*"; }
bad()  { printf 'FAIL %s\n' "$*"; fail=1; }

echo "== PKGBUILD declares the required fields =="
for field in pkgname pkgver pkgrel pkgdesc arch url license depends makedepends source "build()" "package()"; do
  grep -q "^[a-z]*\-*.*=\|^${field}" "$PKG" && ok "declares ${field}" || bad "missing ${field}"
done
grep -q 'bun install --frozen-lockfile' "$PKG" && ok 'builds JS deps with bun, frozen lockfile' || bad 'build does not use bun install --frozen-lockfile'
grep -q 'cargo build --release .*--locked' "$PKG" && ok 'cargo build is --release --locked' || bad 'cargo build is not --release --locked'

echo "== every packaged file is absolute under \$pkgdir =="
while IFS= read -r dest; do
  case "$dest" in
    '$pkgdir/usr/'*) ok "installs $dest" ;;
    *) bad "install destination is not under \$pkgdir/usr: $dest" ;;
  esac
done < <(grep -o '\$pkgdir/[A-Za-z0-9_./-]*' "$PKG" | sort -u)

for required in \
  '$pkgdir/usr/bin/dev-console' \
  '$pkgdir/usr/share/applications/dev-console.desktop' \
  '$pkgdir/usr/share/polkit-1/actions/ai.codemate.devconsole.policy' \
  '$pkgdir/usr/share/icons/hicolor/128x128/apps/dev-console.png'; do
  grep -qF -- "$required" "$PKG" && ok "packages ${required#\$pkgdir}" || bad "does not package ${required#\$pkgdir}"
done

echo "== the policy and launcher end up pointing at installed paths =="
grep -q 'dev-console-auth-helper' "$PKG" && ok 'policy exec path is rewritten' || bad 'policy exec path never rewritten'
grep -q 'Exec=/usr/bin/dev-console' "$PKG" && ok 'launcher Exec is an absolute installed path' || bad 'launcher Exec is not rewritten'

echo "== the policy XML this package installs is well formed =="
python3 - <<'PY' || bad "policy XML is invalid"
import xml.etree.ElementTree as ET
ET.parse('packaging/polkit/ai.codemate.devconsole.policy')
print('ok   packaging/polkit/ai.codemate.devconsole.policy parses')
PY

echo "== desktop entry this package installs is valid =="
if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate packaging/dev-console.desktop && ok 'desktop-file-validate clean' || bad 'desktop entry invalid'
else
  note 'desktop-file-validate not installed, skipped'
fi

echo "== real package build =="
if ! command -v makepkg >/dev/null; then
  echo "UNVERIFIED: makepkg is not available on this host (Arch toolchain required)"
  echo "RESULT: exit 77"
  exit 77
fi
cp -a packaging src-tauri package.json bun.lock "$WORK/" 2>/dev/null || true
( cd "$WORK" && makepkg --repackage -d --noconfirm ) || bad 'makepkg failed'

if [[ "$fail" == 0 ]]; then
  echo "ARCH PACKAGING CHECKS PASSED"
  exit 0
fi
echo "ARCH PACKAGING CHECKS FAILED"
exit 1
