#!/usr/bin/env bash
# Started by scripts/smoke-native.sh inside Xvfb + a private session bus. Never run directly.
# Usage: _smoke-session.sh <binary> <workdir>
set -uo pipefail

BIN="$1"
WORK="$2"
SOCK="${XDG_RUNTIME_DIR}/dev-console/control.sock"
FAILURES=0

fail() {
  echo "FAIL: $*"
  FAILURES=$((FAILURES + 1))
}
pass() { echo "ok: $*"; }
mode_of() { stat -c '%a' "$1"; }

start_app() {
  local tag="$1"
  "$BIN" --library >"$WORK/$tag.log" 2>&1 &
  APP_PID=$!
  for _ in $(seq 1 80); do
    [[ -S "$SOCK" ]] && return 0
    kill -0 "$APP_PID" 2>/dev/null || return 1
    sleep 0.25
  done
  return 1
}

stop_app() {
  [[ -n "${APP_PID:-}" ]] || return 0
  kill -TERM "$APP_PID" 2>/dev/null || true
  for _ in $(seq 1 40); do
    kill -0 "$APP_PID" 2>/dev/null || break
    sleep 0.25
  done
  kill -KILL "$APP_PID" 2>/dev/null || true
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
}

echo "== 1. the real release binary starts and opens its control socket =="
if ! start_app app1; then
  echo "FAIL: no control socket after 20s; app log follows"
  cat "$WORK/app1.log"
  exit 1
fi
pass "control socket present at $SOCK (mode $(mode_of "$SOCK"))"
[[ "$(mode_of "$SOCK")" == "600" ]] && pass "socket is 0600" || fail "socket mode is $(mode_of "$SOCK")"

echo "== 2. the CLI reaches the running instance instead of the offline fallback =="
OUT="$("$BIN" vault status 2>&1)"; CODE=$?
echo "   status -> exit $CODE: $OUT"
grep -q "no running instance" <<<"$OUT" && fail "CLI fell back to the offline path while an instance was running" || pass "answered by the live instance"
[[ "$CODE" == "0" || "$CODE" == "10" || "$CODE" == "20" ]] || fail "unexpected status exit $CODE"

echo "== 3. manual lock is executed by the running instance and persisted =="
OUT="$("$BIN" vault lock 2>&1)"; CODE=$?
echo "   lock -> exit $CODE: $OUT"
grep -q "no running instance" <<<"$OUT" && fail "the running instance did not handle vault lock" || pass "live instance performed the lock"
FLAG="$WORK/config/dev-console/manual-lock"
[[ -f "$FLAG" ]] || fail "manual lock flag missing at $FLAG"
[[ "$(mode_of "$FLAG")" == "600" ]] && pass "lock flag is 0600" || fail "lock flag mode $(mode_of "$FLAG")"
OUT="$("$BIN" vault status 2>&1)"; CODE=$?
echo "   status -> exit $CODE: $OUT"
grep -q "locked (manual)" <<<"$OUT" || fail "live status did not report the manual lock"
[[ "$CODE" == "10" ]] && pass "locked status exits 10" || fail "locked status exited $CODE"

echo "== 4. secret operations stay refused while locked (no secret path opens) =="
grep -qi "panic\|segmentation" "$WORK/app1.log" && fail "app panicked while handling the control channel" || pass "app log clean"

echo "== 5. the manual lock survives a full restart =="
stop_app
rm -f "$SOCK"
if ! start_app app2; then
  echo "FAIL: second start did not produce a control socket; log follows"
  cat "$WORK/app2.log"
  exit 1
fi
OUT="$("$BIN" vault status 2>&1)"; CODE=$?
echo "   status after restart -> exit $CODE: $OUT"
grep -q "locked (manual)" <<<"$OUT" || fail "the vault auto-unlocked after a restart"
[[ "$CODE" == "10" ]] && pass "still locked after restart (exit 10)" || fail "post-restart status exited $CODE"

echo "== 6. single-instance handoff keeps one resident process =="
"$BIN" --library >"$WORK/second.log" 2>&1 &
SECOND=$!
for _ in $(seq 1 40); do kill -0 "$SECOND" 2>/dev/null || break; sleep 0.25; done
if kill -0 "$SECOND" 2>/dev/null; then
  kill -KILL "$SECOND" 2>/dev/null
  fail "a second instance kept running instead of handing off"
else
  pass "second invocation exited after handing off"
fi
kill -0 "$APP_PID" 2>/dev/null && pass "the original instance is still resident" || fail "the original instance died"
OUT="$("$BIN" vault status 2>&1)"; grep -q "no running instance" <<<"$OUT" && fail "control socket stopped answering after handoff" || pass "control socket still answers"

echo "== 6b. a second window really opens in the same resident process =="
if command -v xwininfo >/dev/null; then
  "$BIN" --finder >/dev/null 2>&1 &
  FINDER_CALL=$!
  wait "$FINDER_CALL" 2>/dev/null || true
  sleep 2
  TREE="$(xwininfo -root -tree 2>/dev/null)"
  echo "$TREE" | grep -ciE "dev[ -]?console|dev-console" >/dev/null && pass "top-level windows are visible on the X server" || fail "no Dev Console window found on the X server"
  WINDOWS=$(echo "$TREE" | grep -cE "^\s+0x[0-9a-f]+ .*dev[ -]?console" || true)
  echo "   dev-console related windows on the tree: $WINDOWS"
  OUT="$("$BIN" vault status 2>&1)"
  grep -q "no running instance" <<<"$OUT" && fail "the instance stopped answering after opening the finder" || pass "the resident instance still serves both intents"
  # A lock taken from another process must be seen by the running instance (both windows read
  # the same state and receive the same vault-state event).
  OUT="$("$BIN" vault lock 2>&1)"; grep -q "no running instance" <<<"$OUT" || pass "cross-process lock handled by the running instance"
  OUT="$("$BIN" vault status 2>&1)"
  grep -q "locked (manual)" <<<"$OUT" || fail "status did not report the lock the running instance applied"
  pass "the running instance reports the lock that both windows are subscribed to"
else
  echo "skip: xwininfo not installed"
fi

echo "== 7. app data files are private =="
DB="$WORK/data/dev-console/commands.sqlite3"
if [[ -f "$DB" ]]; then
  [[ "$(mode_of "$DB")" == "600" ]] && pass "database is 0600" || fail "database mode $(mode_of "$DB")"
  [[ "$(mode_of "$WORK/data/dev-console")" == "700" ]] && pass "app data directory is 0700" || fail "data dir mode $(mode_of "$WORK/data/dev-console")"
else
  echo "$WORK/data/dev-console" | xargs ls -la
  fail "the running app did not create $DB"
fi

echo "== 8. unlock without an authentication mechanism fails closed =="
stop_app
OUT="$("$BIN" vault unlock 2>&1)"; CODE=$?
echo "   unlock with no instance -> exit $CODE: $OUT"
[[ "$CODE" != "0" ]] && pass "unlock refused (exit $CODE)" || fail "unlock claimed success with no authentication mechanism"
[[ -f "$FLAG" ]] && pass "the manual lock flag survived the refused unlock" || fail "the refused unlock cleared the lock flag"

echo "== 9. the window actually renders =="
if command -v scrot >/dev/null; then
  start_app app3 || fail "third start failed"
  sleep 3
  scrot "$WORK/screen.png" 2>/dev/null && {
    SIZE=$(stat -c '%s' "$WORK/screen.png")
    [[ "$SIZE" -gt 20000 ]] && pass "screenshot captured ($SIZE bytes)" || fail "screenshot suspiciously small ($SIZE bytes)"
  } || fail "scrot failed"
  stop_app
else
  echo "skip: scrot not installed"
fi

if [[ "$FAILURES" == "0" ]]; then
  echo "ALL NATIVE SMOKE CHECKS PASSED"
  exit 0
fi
echo "$FAILURES native smoke check(s) failed"
exit 1
