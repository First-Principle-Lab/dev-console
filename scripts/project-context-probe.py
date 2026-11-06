"""Second half of scripts/test-project-context.sh; run it through that script, not directly.

Checks that this run can mean anything (the focused window is the terminal running it, and the
user's prompt is the only shell between that terminal and this process), then runs the in-process
detection test, then opens the palette of the running Dev Console and reads back what it ranked by.

argv: <hyprland socket> <expected repository root>. Exit 0 PASS, 77 UNVERIFIED, anything else FAIL.
"""

import json
import os
import socket
import subprocess
import sys
import time

SHELLS = {"zsh", "bash", "fish", "nu"}
UNVERIFIED = 77


def unverified(reason):
    print(f"UNVERIFIED: {reason}")
    sys.exit(UNVERIFIED)


def ask(path, request, timeout=1.0):
    with socket.socket(socket.AF_UNIX) as conn:
        conn.settimeout(timeout)
        conn.connect(path)
        conn.sendall(request)
        conn.shutdown(socket.SHUT_WR)
        reply = b""
        while chunk := conn.recv(4096):
            reply += chunk
    return reply.decode("utf-8", "replace")


def proc(pid, name):
    with open(f"/proc/{pid}/{name}", encoding="utf-8", errors="replace") as handle:
        return handle.read()


def parent(pid):
    stat = proc(pid, "stat")
    return int(stat[stat.rfind(")") + 1 :].split()[1])


def ancestors(pid):
    chain = []
    while pid > 1:
        pid = parent(pid)
        chain.append(pid)
    return chain


def control(command):
    runtime = os.environ.get("DEV_CONSOLE_RUNTIME_DIR") or os.path.join(
        os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"), "dev-console"
    )
    path = os.path.join(runtime, "control.sock")
    if not os.path.exists(path):
        return None
    return json.loads(ask(path, json.dumps({"command": command}).encode() + b"\n", timeout=5.0))


def main():
    # Interleaved with cargo's own output, so every line goes out as it is printed.
    sys.stdout.reconfigure(line_buffering=True)
    hypr, expected = sys.argv[1], sys.argv[2]

    window = json.loads(ask(hypr, b"j/activewindow"))
    chain = ancestors(os.getpid())
    if window.get("pid") not in chain:
        unverified(
            f"the focused window ({window.get('class') or 'nothing'}) is not the terminal running "
            "this harness; run it from the terminal you are looking at, and leave it focused"
        )
    between = chain[: chain.index(window["pid"])]
    shells = [pid for pid in between if proc(pid, "comm").strip().lstrip("-") in SHELLS]
    if len(shells) != 1:
        unverified(
            f"{len(shells)} shells between the terminal and this harness; detection would find the "
            "newest of them, not your prompt. Run `bash scripts/test-project-context.sh` directly at "
            "a prompt, not through verify.sh or another script"
        )

    print("== in-process: focused window over the socket, then /proc, as show_finder does ==")
    env = dict(os.environ, DEV_CONSOLE_EXPECT_ROOT=expected)
    test = subprocess.run(
        [
            "cargo", "test", "--manifest-path", "src-tauri/Cargo.toml", "--locked",
            "--test", "project-context", "--", "--ignored", "--exact", "--nocapture",
            "the_focused_terminal_reports_its_project",
        ],
        env=env,
    )
    if test.returncode != 0:
        print(
            "RESULT: FAIL (in-process detection). If this terminal has other tabs or windows, a "
            "newer shell may have been picked: the window title names no folder while this runs."
        )
        sys.exit(test.returncode)

    print("== live app: open the real palette and read what it ranks by ==")
    before = control("finder.context")
    if before is None:
        unverified("in-process detection passed, but no running Dev Console answers the control socket")
    if not before.get("ok"):
        unverified(
            "in-process detection passed, but the running Dev Console predates `finder.context`; "
            "start this branch's build and run again"
        )
    state = json.loads(before["message"])
    if not state["enabled"]:
        unverified("in-process detection passed, but project ranking is switched off in Settings")
    if state["visible"]:
        control("window.finder")
        time.sleep(0.3)

    opened = time.monotonic()
    control("window.finder")
    context, waited = None, 0.0
    try:
        while waited < 1.0:
            state = json.loads(control("finder.context")["message"])
            if state["context"]:
                context = state["context"]
                break
            time.sleep(0.01)
            waited = time.monotonic() - opened
    finally:
        if json.loads(control("finder.context")["message"])["visible"]:
            control("window.finder")

    if context is None:
        print("RESULT: FAIL (the palette opened with no project)")
        sys.exit(1)
    if context["root"] != expected:
        print(f"RESULT: FAIL (the palette ranked by {context['root']}, expected {expected})")
        sys.exit(1)
    print(
        f"RESULT: PASS (focused window -> shell -> project; the palette named {context['name']} "
        f"{(time.monotonic() - opened) * 1000:.1f} ms after it was asked to open)"
    )


if __name__ == "__main__":
    main()
