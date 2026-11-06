#!/usr/bin/env bash
# Captures the redesigned windows as ACTUALLY RENDERED by the release binary under Xvfb/X11.
# Started by scripts/native-capture.sh inside a private dbus session + Xvfb. Never run directly.
#
# This host has no Wayland session, no Hyprland and no compositor: what this proves is the
# opaque-shell material level on X11 (window fully painted, square window corners, no
# background bleed, frames at rest are stable) - never compositor blur. See the C9 section of
# docs/design/raycast-ui-research-2026-11-02.md for the achieved material level and the manual
# Wayland/Hyprland checklist.
#
# Usage: _native-capture-session.sh <binary> <workdir> <probe> <shot-dir>
set -uo pipefail

BIN="$1"
WORK="$2"
PROBE="$3"
SHOTS="$4"
SOCK="${XDG_RUNTIME_DIR}/dev-console/control.sock"
FAILURES=0
BG_PID=""
APP_PID=""

fail() { echo "FAIL: $*"; FAILURES=$((FAILURES + 1)); }
pass() { echo "ok: $*"; }
note() { echo "   $*"; }

cleanup() {
  [[ -n "$APP_PID" ]] && kill -KILL "$APP_PID" 2>/dev/null
  [[ -n "$BG_PID" ]] && kill -KILL "$BG_PID" 2>/dev/null
  return 0
}
trap cleanup EXIT

# Colours the busy background paints that the application palette can never produce: the six
# saturated cells everywhere, plus pure black; pure white is added only for the dark theme
# because the light theme's shell background legitimately is #ffffff.
BUSY_A='ff00ff,00ffff,ff0000,00ff00,ffa500,8000ff,000000'
BUSY_B='ff00ff,00ffff,ff0000,00ff00,ffa500,8000ff,000000,ffffff'

start_bg() { # start_bg dark|busy
  if [[ -n "$BG_PID" ]] && kill -0 "$BG_PID" 2>/dev/null; then
    kill -KILL "$BG_PID" 2>/dev/null
    wait "$BG_PID" 2>/dev/null
  fi
  "$PROBE" bg "$1" >/dev/null 2>&1 &
  BG_PID=$!
  sleep 1
}

start_app() { # start_app <tag> [GTK_THEME=value] [args...]
  local tag="$1"; shift
  if [[ "${1:-}" == theme=* ]]; then
    export GTK_THEME="${1#theme=}"
    shift
  else
    unset GTK_THEME
  fi
  "$BIN" "$@" >"$WORK/$tag.log" 2>&1 &
  APP_PID=$!
  local i
  for i in $(seq 1 80); do
    [[ -S "$SOCK" ]] && return 0
    kill -0 "$APP_PID" 2>/dev/null || return 1
    sleep 0.25
  done
  return 1
}

stop_app() {
  [[ -n "$APP_PID" ]] || return 0
  kill -TERM "$APP_PID" 2>/dev/null || true
  local i
  for i in $(seq 1 40); do
    kill -0 "$APP_PID" 2>/dev/null || break
    sleep 0.25
  done
  kill -KILL "$APP_PID" 2>/dev/null || true
  wait "$APP_PID" 2>/dev/null
  APP_PID=""
  rm -f "$SOCK"
}

# find_window <name-substring> -> "<id> <x> <y> <w> <h> <map_state>" for the first match
find_window() {
  "$PROBE" list | awk -v pat="$1" 'index($0, pat) { print $1, $2, $3, $4, $5, $6; exit }'
}

paints_ok() { # paints_ok <x> <y> <bg-pixel> - first sample must be app paint, not bare background
  local px
  px="$("$PROBE" sample "$1" "$2")"
  [[ "$px" != "$3" ]] && return 0 || return 1
}

mkdir -p "$SHOTS"

echo "== 0. dark X background; first start creates and migrates the database =="
start_bg dark
if ! start_app boot; then
  echo "FAIL: the binary did not come up; log follows"
  cat "$WORK/boot.log"
  exit 1
fi
stop_app
DB="$WORK/data/dev-console/commands.sqlite3"
[[ -f "$DB" ]] || { echo "FAIL: no database at $DB to seed" >&2; exit 1; }
bun "$(dirname "$0")/native/seed-commands.mjs" "$DB" \
  || { echo "FAIL: seeding failed" >&2; exit 1; }

echo
echo "phase A: system default theme (WebKitGTK light palette)"
echo "== 1. library window on the dark background =="
start_app library --library || { cat "$WORK/library.log"; exit 1; }
sleep 4
read -r _L0 LX LY LW LH _ <<<"$(find_window "Dev Console")"
if [[ -z "${LX:-}" ]]; then
  fail "no library window on the X server"; note "$("$PROBE" list)"
else
  note "library window: x=$LX y=$LY w=$LW h=$LH"
  D="$("$PROBE" diff "$LX" "$LY" "$LW" "$LH" 400 | tail -1)"
  if [[ "$D" == "changed=0" ]]; then
    pass "library client area: 0 pixels changed across two grabs 400ms apart (settled frame, no stale or torn paint)"
  else
    fail "library client area still changing at rest: $D"
  fi
  scrot "$SHOTS/native-redesign-library__dark-xvfb.png"
  for corner in "2 2 top-left" "$((LW - 3)) 2 top-right" "2 $((LH - 3)) bottom-left" \
                "$((LW - 3)) $((LH - 3)) bottom-right"; do
    set -- $corner
    C="$("$PROBE" sample $((LX + $1)) $((LY + $2)))"
    note "corner $3: $C"
    if [[ "$C" == "#000000" ]]; then
      fail "corner $3 shows the bare background - window is not an opaque square"
    else
      pass "corner $3 is app paint ($C) - window corners are square and opaque"
    fi
  done
fi

echo "== 2. finder over the dark background (vault storage unavailable here, seeded commands) =="
"$BIN" --finder >/dev/null 2>&1
sleep 0.4
read -r F0 FX FY FW FH _ <<<"$(find_window "dev-console-finder")"
if [[ -z "${F0:-}" ]]; then
  fail "no finder window after --finder"
else
  note "finder window: x=$FX y=$FY w=$FW h=$FH"
  if paints_ok $((FX + FW / 2)) $((FY + FH / 2)) "#000000"; then
    C="$("$PROBE" sample $((FX + FW / 2)) $((FY + FH / 2)))"
    pass "first probe after map: centre is app paint ($C), not the background - no blank/unpainted frame"
  else
    fail "palette centre is the bare background immediately after mapping"
  fi
  sleep 1.5
  scrot "$SHOTS/native-redesign-finder__dark-xvfb.png"
  echo "== 2b. typed query through XTEST (focus + type on one connection) =="
  if "$PROBE" type "$F0" "docker"; then
    sleep 1
    scrot "$WORK/finder-query-a.png"
    if cmp -s "$SHOTS/native-redesign-finder__dark-xvfb.png" "$WORK/finder-query-a.png"; then
      fail "XTEST typing changed nothing in the rendered palette"
    else
      pass "typing 'docker' changed the rendered palette (live keyboard path into the webview)"
    fi
    cp "$WORK/finder-query-a.png" "$SHOTS/native-redesign-finder-query__dark-xvfb.png"
  else
    fail "XTEST typing unavailable"
  fi
fi

echo "== 3. busy background: opacity and bleed, both windows =="
if [[ -n "${F0:-}" ]]; then
  "$BIN" --finder >/dev/null 2>&1 # toggle the palette away
  sleep 1
  ST="$(find_window "dev-console-finder" | awk '{print $6}')"
  [[ "$ST" == "2" ]] && note "finder still viewable after the hide toggle (map_state=$ST)" || pass "palette dismissed by the toggle"
fi
start_bg busy
sleep 1
if [[ -n "${LX:-}" ]]; then
  D="$("$PROBE" diff "$LX" "$LY" "$LW" "$LH" 400 | tail -1)"
  if [[ "$D" == "changed=0" ]]; then
    pass "over the BUSY background too: 0 pixels changed in 400ms at rest"
  else
    fail "changing at rest over the busy background: $D"
  fi
fi
scrot "$SHOTS/native-redesign-library__busy-xvfb.png"
if [[ -n "${LX:-}" ]]; then
  read -r _L2 _LX2 _LY2 LW2 LH2 _ <<<"$(find_window "Dev Console")"
  note "library geometry now: w=$LW2 h=$LH2"
  HITS="$("$PROBE" picheck "$LX" "$LY" "$LW" "$LH" "$BUSY_A" | awk -F: '{s+=$2} END {print s}')"
  if [[ "$HITS" == "0" ]]; then
    pass "library client area contains 0 background pixels (fully opaque shell on a busy X11 background)"
  else
    fail "library shows $HITS background-coloured pixels - bleed or unmapped window"
    note "$("$PROBE" list)"
  fi
fi
"$BIN" --finder >/dev/null 2>&1
sleep 1.5
if [[ -n "${FX:-}" ]]; then
  scrot "$SHOTS/native-redesign-finder__busy-xvfb.png"
  HITS="$("$PROBE" picheck "$FX" "$FY" "$FW" "$FH" "$BUSY_A" | awk -F: '{s+=$2} END {print s}')"
  if [[ "$HITS" == "0" ]]; then
    pass "palette client area contains 0 background pixels (fully opaque shell on a busy X11 background)"
  else
    fail "palette shows $HITS background-coloured pixels - bleed or unmapped window"
    note "$("$PROBE" list)"
  fi
fi
stop_app

echo
echo "phase B: GTK_THEME=Adwaita:dark (the dark palette) on both backgrounds"
start_bg dark
start_app darklib theme=Adwaita:dark --library || { cat "$WORK/darklib.log"; exit 1; }
sleep 4
read -r DL0 DLX DLY DLW DLH _ <<<"$(find_window "Dev Console")"
note "dark library window: x=$DLX y=$DLY w=$DLW h=$DLH"
scrot "$SHOTS/native-redesign-library__darktheme-xvfb.png"
"$BIN" --finder >/dev/null 2>&1
sleep 1.5
read -r DF0 DFX DFY DFW DFH _ <<<"$(find_window "dev-console-finder")"
note "dark finder window: x=$DFX y=$DFY w=$DFW h=$DFH"
scrot "$SHOTS/native-redesign-finder__darktheme-xvfb.png"
CORNER="$("$PROBE" sample $((DFX + 2)) $((DFY + 2)))"
note "dark palette corner paint: $CORNER"
[[ "$CORNER" == "#000000" ]] && fail "dark palette corner shows bare background" || pass "dark palette corner is app paint"
start_bg busy
sleep 1
scrot "$SHOTS/native-redesign-finder__darktheme-busy-xvfb.png"
HITS="$("$PROBE" picheck "$DFX" "$DFY" "$DFW" "$DFH" "$BUSY_B" | awk -F: '{s+=$2} END {print s}')"
if [[ "$HITS" == "0" ]]; then
  pass "dark palette over the busy background: 0 background pixels inside"
else
  fail "dark palette shows $HITS background-coloured pixels - bleed"
  note "$("$PROBE" list)"
fi

echo "== window tree before shutdown =="
"$PROBE" list
stop_app

if [[ "$FAILURES" == "0" ]]; then
  echo "NATIVE CAPTURE OK"
  exit 0
fi
echo "$FAILURES native capture check(s) failed"
exit 1
