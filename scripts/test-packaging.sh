#!/usr/bin/env bash
# C19 cloud substitute: stage a complete installation into a disposable prefix and prove the
# result is a valid, self-consistent install - and that uninstall removes exactly what install
# placed, nothing more.
#
# What it does NOT prove: that a real desktop environment picks the entry up, that the icon
# theme cache refreshes, or that polkit actually prompts. Those stay desktop checks.
#
# Exit 77 means UNVERIFIED (validator missing).
set -euo pipefail
cd "$(dirname "$0")/.."

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

command -v desktop-file-validate >/dev/null || {
  echo "UNVERIFIED: desktop-file-validate is not installed"
  exit 77
}

# A stand-in executable: packaging must not care what the binary contains.
mkdir -p "$WORK/build"
printf '#!/bin/sh\necho dev-console\n' >"$WORK/build/dev-console"
chmod 700 "$WORK/build/dev-console"

echo "== desktop file validation =="
desktop-file-validate packaging/dev-console.desktop
echo "launcher desktop file is valid"

echo "== staged install into a disposable prefix =="
PREFIX=/usr/local DESTDIR="$WORK/root" BINARY="$WORK/build/dev-console" \
  bash packaging/install.sh install

desk="$WORK/root/usr/local/share/applications/dev-console.desktop"
helper="$WORK/root/usr/local/libexec/dev-console-auth-helper"
policy="$WORK/root/usr/local/share/polkit-1/actions/ai.codemate.devconsole.policy"
binary="$WORK/root/usr/local/bin/dev-console"
manifest="$WORK/root/usr/local/share/dev-console/installed-files.txt"

fail() {
  echo "FAIL: $*"
  exit 1
}
mode_of() { stat -c '%a' "$1"; }

[[ -x "$binary" ]] || fail "binary not installed"
[[ -f "$desk" ]] || fail "desktop file not installed"
[[ -x "$helper" ]] || fail "polkit helper not installed"
[[ -f "$policy" ]] || fail "polkit policy not installed"
desktop-file-validate "$desk" || fail "installed desktop file is invalid"
echo "installed desktop file validates too"

python3 - "$policy" <<'PY' || fail "polkit policy is not well-formed or points at the wrong helper"
import sys, xml.etree.ElementTree as ET
path = sys.argv[1]
root = ET.parse(path).getroot()
assert root.tag == "policyconfig", root.tag
action = root.find("action")
assert action is not None and action.get("id") == "ai.codemate.devconsole.unlock"
paths = [
    (el.text or "").strip()
    for el in action.iter("annotate")
    if el.get("key") == "org.freedesktop.policykit.exec.path"
]
assert paths == ["/usr/local/libexec/dev-console-auth-helper"], paths
defaults = action.find("defaults")
assert defaults is not None, "a policy without defaults would not prompt"
got = {el.tag: (el.text or "").strip() for el in defaults}
assert got.get("allow_any") == "no" and got.get("allow_inactive") == "no", got
assert got.get("allow_active") == "auth_admin_keep", got
print("polkit policy parses; exec.path matches the installed helper; defaults require an admin")
PY

[[ "$(mode_of "$helper")" == "755" ]] || fail "helper mode $(mode_of "$helper")"
[[ "$(mode_of "$policy")" == "644" ]] || fail "policy mode $(mode_of "$policy")"
for size in 32x32 128x128 256x256 512x512; do
  [[ -f "$WORK/root/usr/local/share/icons/hicolor/$size/apps/dev-console.png" ]] ||
    fail "icon missing for $size"
done
echo "icons installed for 32/128/256/512"

echo "== uninstall removes exactly what was installed =="
foreign="$WORK/root/usr/local/share/applications/other-app.desktop"
printf '[Desktop Entry]\nType=Application\nName=Other\n' >"$foreign"
cp "$binary" "$WORK/binary-before"

PREFIX=/usr/local DESTDIR="$WORK/root" bash packaging/install.sh uninstall | sed 's/^/   /'

[[ -f "$foreign" ]] || fail "uninstall deleted a file it never installed"
[[ -f "$binary" ]] && fail "binary survived uninstall"
[[ -f "$desk" ]] && fail "desktop file survived uninstall"
[[ -f "$policy" ]] && fail "policy survived uninstall"
[[ -f "$WORK/root/usr/local/share/icons/hicolor/128x128/apps/dev-console.png" ]] &&
  fail "icon survived uninstall"
echo "uninstall is exact"

echo "== uninstall refuses to guess without a manifest =="
if PREFIX=/usr/local DESTDIR="$WORK/root" bash packaging/install.sh uninstall 2>/dev/null; then
  fail "a second uninstall claimed success with no manifest"
fi
echo "second uninstall refused rather than guessing"
echo "PACKAGING CHECKS PASSED"
