#!/usr/bin/env bash
# Install or remove Dev Console from a prefix. Only this script's own manifest is ever removed,
# so an uninstall never deletes files it did not place.
#
#   packaging/install.sh install   [PREFIX=/usr/local] [DESTDIR=] [BINARY=path]
#   packaging/install.sh uninstall [PREFIX=/usr/local] [DESTDIR=]
#
# The polkit policy is rewritten to point at the helper inside the chosen prefix, so an
# installation under /usr/local never authorises a helper under /usr (or vice versa).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ACTION="${1:-install}"
PREFIX="${PREFIX:-/usr/local}"
DESTDIR="${DESTDIR:-}"
BINARY="${BINARY:-$ROOT/src-tauri/target/release/dev-console}"
MANIFEST_NAME="share/dev-console/installed-files.txt"

target() { printf '%s%s' "$DESTDIR" "$1"; }

manifest() { target "$PREFIX/$MANIFEST_NAME"; }

record() {
  mkdir -p "$(dirname "$(manifest)")"
  printf '%s\n' "$1" >>"$(manifest)"
}

install_file() {
  local source="$1" dest="$2" mode="$3"
  install -D -m "$mode" "$source" "$(target "$dest")"
  record "${dest#"$DESTDIR"}"
}

case "$ACTION" in
  install)
    [[ -x "$BINARY" ]] || {
      echo "no executable at $BINARY (build it first: bunx tauri build --no-bundle)" >&2
      exit 1
    }
    rm -f "$(manifest)"

    install_file "$BINARY" "$PREFIX/bin/dev-console" 0755
    install_file "$ROOT/packaging/dev-console.desktop" "$PREFIX/share/applications/dev-console.desktop" 0644
    install_file "$ROOT/packaging/dev-console-auth-helper" "$PREFIX/libexec/dev-console-auth-helper" 0755
    install_file "$ROOT/packaging/polkit/ai.codemate.devconsole.policy" \
      "$PREFIX/share/polkit-1/actions/ai.codemate.devconsole.policy" 0644

    # The policy must name the helper that this prefix actually installed.
    policy="$(target "$PREFIX/share/polkit-1/actions/ai.codemate.devconsole.policy")"
    sed -i "s#<annotate key=\"org.freedesktop.policykit.exec.path\">[^<]*</annotate>#<annotate key=\"org.freedesktop.policykit.exec.path\">$PREFIX/libexec/dev-console-auth-helper</annotate>#" "$policy"
    grep -q "<annotate key=\"org.freedesktop.policykit.exec.path\">$PREFIX/libexec/dev-console-auth-helper</annotate>" "$policy" || {
      echo "could not point the polkit policy at $PREFIX/libexec/dev-console-auth-helper" >&2
      exit 1
    }

    install_file "$ROOT/src-tauri/icons/32x32.png" "$PREFIX/share/icons/hicolor/32x32/apps/dev-console.png" 0644
    install_file "$ROOT/src-tauri/icons/128x128.png" "$PREFIX/share/icons/hicolor/128x128/apps/dev-console.png" 0644
    install_file "$ROOT/src-tauri/icons/icon.png" "$PREFIX/share/icons/hicolor/256x256/apps/dev-console.png" 0644
    install_file "$ROOT/src-tauri/icons/icon@2x.png" "$PREFIX/share/icons/hicolor/512x512/apps/dev-console.png" 0644

    echo "installed under $(target "$PREFIX")"
    echo "if systemd or a polkit agent is in use, reload them so the new policy is picked up"
    ;;

  uninstall)
    [[ -f "$(manifest)" ]] || {
      echo "no install manifest at $(manifest); refusing to guess what to remove" >&2
      exit 1
    }
    while read -r relative; do
      [[ -n "$relative" ]] || continue
      rm -f "$(target "$relative")"
      echo "removed $(target "$relative")"
    done <"$(manifest)"
    rm -f "$(manifest)"
    ;;

  *)
    echo "usage: $0 install|uninstall" >&2
    exit 2
    ;;
esac
