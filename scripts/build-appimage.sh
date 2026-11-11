#!/usr/bin/env bash
# Package the native release binary as the AppImage a release publishes.
#
#   scripts/build-appimage.sh [path/to/dev-console] [output.AppImage]
#
# The AppDir is assembled here, file by file, and packed with pinned tooling: appimagetool and the
# type2 runtime are downloaded once into the cache and refused unless their SHA-256 matches. The
# binary links only libc, so nothing else is bundled; the window system, GPU and fontconfig
# libraries are the host's, loaded at run time, as they are for the packaged binary.
#
# APPIMAGE_TOOL_CACHE overrides where the tooling is kept (default: ~/.cache/dev-console-appimage).
set -euo pipefail
# Arguments are the caller's paths; the defaults are the repository's. The packer changes
# directory, so both end up absolute.
BIN="$(realpath -m "${1:-$(dirname "$0")/../target/release/dev-console}")"
OUT="$(realpath -m "${2:-$(dirname "$0")/../target/appimage/Dev-Console.AppImage}")"
cd "$(dirname "$0")/.."
[[ -x "$BIN" ]] || {
  echo "no release binary at $BIN - run 'cargo build --release --locked -p dev-console-desktop' first" >&2
  exit 1
}
[[ "$(uname -m)" == "x86_64" ]] || { echo "published AppImages are x86_64 only" >&2; exit 1; }

TOOL_VERSION=1.9.1
TOOL_SHA256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
RUNTIME_VERSION=20251108
RUNTIME_SHA256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d

CACHE="${APPIMAGE_TOOL_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/dev-console-appimage}"
mkdir -p "$CACHE"

# fetch <url> <sha256> <file>: reuse a cached copy only if it still has the pinned digest.
fetch() {
  local url="$1" sum="$2" file="$3"
  if [[ ! -f "$file" ]] || ! echo "$sum  $file" | sha256sum --check --status; then
    curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
      --retry 2 --output "$file.partial" "$url"
    echo "$sum  $file.partial" | sha256sum --check --status || {
      rm -f "$file.partial"
      echo "checksum mismatch for $url" >&2
      exit 1
    }
    mv "$file.partial" "$file"
  fi
}

TOOL="$CACHE/appimagetool-$TOOL_VERSION-x86_64.AppImage"
RUNTIME="$CACHE/runtime-$RUNTIME_VERSION-x86_64"
fetch "https://github.com/AppImage/appimagetool/releases/download/$TOOL_VERSION/appimagetool-x86_64.AppImage" \
  "$TOOL_SHA256" "$TOOL"
fetch "https://github.com/AppImage/type2-runtime/releases/download/$RUNTIME_VERSION/runtime-x86_64" \
  "$RUNTIME_SHA256" "$RUNTIME"
chmod +x "$TOOL"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
APPDIR="$WORK/Dev-Console.AppDir"

install -Dm755 "$BIN" "$APPDIR/usr/bin/dev-console"
install -Dm644 packaging/dev-console.desktop "$APPDIR/usr/share/applications/dev-console.desktop"
install -Dm644 src-tauri/icons/32x32.png "$APPDIR/usr/share/icons/hicolor/32x32/apps/dev-console.png"
install -Dm644 src-tauri/icons/128x128.png "$APPDIR/usr/share/icons/hicolor/128x128/apps/dev-console.png"
install -Dm644 src-tauri/icons/icon.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/dev-console.png"
install -Dm644 src-tauri/icons/icon@2x.png "$APPDIR/usr/share/icons/hicolor/512x512/apps/dev-console.png"
# What the AppImage format itself reads: the entry point, one desktop file and its icon at the root.
ln -s usr/bin/dev-console "$APPDIR/AppRun"
ln -s usr/share/applications/dev-console.desktop "$APPDIR/dev-console.desktop"
ln -s usr/share/icons/hicolor/256x256/apps/dev-console.png "$APPDIR/dev-console.png"
ln -s dev-console.png "$APPDIR/.DirIcon"

mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"
# The tool is itself an AppImage; extract-and-run keeps it working where FUSE is not available.
ARCH=x86_64 "$TOOL" --appimage-extract-and-run --no-appstream --runtime-file "$RUNTIME" \
  "$APPDIR" "$OUT" >"$WORK/appimagetool.log" 2>&1 || {
  cat "$WORK/appimagetool.log" >&2
  exit 1
}
chmod 755 "$OUT"

# The image has to start as the binary does, without FUSE too, before anything signs it.
expected="$("$BIN" --version)"
actual="$("$OUT" --appimage-extract-and-run --version)"
[[ "$actual" == "$expected" ]] || {
  echo "the AppImage reports '$actual', the binary '$expected'" >&2
  exit 1
}
echo "$OUT ($actual, $(stat -c %s "$OUT") bytes)"
