#!/usr/bin/env python3
"""Exercise the Rune binary on a live Hyprland session with a disposable profile.

Uses synthetic data only. Does not edit compositor configuration or the user's profile.
Exit 77 means the native desktop or its input/capture tools are unavailable.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import time


def run(*args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, timeout=8, **kwargs)


def clients():
    return json.loads(run("hyprctl", "-j", "clients").stdout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", default="target/debug/dev-console")
    parser.add_argument("--output", help="Directory for synthetic app captures and the report")
    args = parser.parse_args()
    required = ["hyprctl", "wtype", "ydotool", "grim", "wl-paste", "busctl"]
    missing = [name for name in required if not shutil.which(name)]
    if missing or not os.environ.get("HYPRLAND_INSTANCE_SIGNATURE"):
        print("UNVERIFIED: live Hyprland and tools required: " + ", ".join(missing))
        return 77
    binary = str(Path(args.binary).resolve())
    output = Path(args.output or tempfile.mkdtemp(prefix="dev-console-native-evidence-"))
    output.mkdir(parents=True, exist_ok=True)
    output.chmod(0o700)
    with tempfile.TemporaryDirectory(prefix="dev-console-native-profile-") as folder:
        root = Path(folder)
        env = dict(os.environ, HOME=folder, DEV_CONSOLE_DATA_DIR=str(root / "data"),
                   DEV_CONSOLE_CONFIG_DIR=str(root / "config"),
                   DEV_CONSOLE_RUNTIME_DIR=str(root / "runtime"))
        with (output / "app.log").open("w") as log:
            app = subprocess.Popen([binary, "--library"], env=env, stdout=log, stderr=log)
            try:
                def own(title):
                    return next((c for c in clients() if c["pid"] == app.pid and c["title"] == title), None)

                def wait(title, visible=True):
                    for _ in range(80):
                        window = own(title)
                        if bool(window) == visible:
                            return window
                        assert app.poll() is None, "native process exited early"
                        time.sleep(0.05)
                    raise AssertionError(f"window {title!r}, visible={visible}")

                def key(window, *keys):
                    # Verify the actual recipient immediately before injecting any input.
                    run("hyprctl", "dispatch", 'hl.dsp.focus({window="address:' + window["address"] + '"})')
                    active = json.loads(run("hyprctl", "-j", "activewindow").stdout)
                    assert active["pid"] == app.pid and active["address"] == window["address"]
                    run("wtype", *keys)
                    time.sleep(0.15)

                def capture(title, filename):
                    time.sleep(0.5)
                    window = own(title)
                    x, y = window["at"]
                    w, h = window["size"]
                    run("grim", "-g", f"{x},{y} {w}x{h}", str(output / filename))

                library = wait("Dev Console")
                assert not library["floating"]
                key(library, "-M", "ctrl", "-k", "n", "-m", "ctrl")
                key(library, "native-desktop-probe", "-k", "Tab", folder, "-k", "Tab", "-k", "Tab",
                    "printf synthetic-native-output", "-M", "ctrl", "-k", "s", "-m", "ctrl")
                db = root / "data/commands.sqlite3"
                for _ in range(40):
                    with sqlite3.connect(db) as conn:
                        saved = conn.execute("SELECT name, command FROM entries").fetchall()
                    if saved:
                        break
                    time.sleep(0.05)
                assert saved == [("native-desktop-probe", "printf synthetic-native-output")]
                capture("Dev Console", "library.png")
                key(library, "-M", "ctrl", "-k", "2", "-m", "ctrl")
                key(library, "-M", "ctrl", "-k", "n", "-m", "ctrl")
                key(library, "synthetic-desktop-token", "-k", "Tab", "synthetic-desktop-value",
                    "-M", "ctrl", "-k", "s", "-m", "ctrl")
                assert (root / "data/secrets.vault").exists()
                capture("Dev Console", "secret-masked.png")
                key(library, "-M", "ctrl", "-k", "1", "-m", "ctrl")
                baseline = {c["address"]: (c["at"], c["size"], c["floating"]) for c in clients()}
                for iteration in range(12):
                    run(binary, "--finder", env=env)
                    finder = wait("dev-console-finder")
                    assert finder["size"] == [680, 420] and finder["floating"]
                    assert finder["class"] == "dev-console"
                    if iteration == 0:
                        key(finder, "native-desktop", "-k", "Tab", "-k", "Down", "-k", "Up")
                        capture("dev-console-finder", "finder.png")
                        key(finder, "-k", "Escape")
                    else:
                        run(binary, "--finder", env=env)
                    wait("dev-console-finder", False)
                    current = {c["address"]: (c["at"], c["size"], c["floating"]) for c in clients()}
                    assert all(current.get(address) == geometry for address, geometry in baseline.items()), "Finder changed tiled layout"
                run(binary, "--finder", env=env)
                finder = wait("dev-console-finder")
                x, y = finder["at"]
                for px, py in [(x + 50, y + 80), (x + 400, y + 140), (x + 20, y + 250), (x - 10, y + 180), (x + 100, y + 100)]:
                    run("ydotool", "mousemove", "--absolute", "-x", str(px), "-y", str(py))
                    time.sleep(0.15)
                    assert app.poll() is None
                finder = wait("dev-console-finder")
                assert finder["size"] == [680, 420] and finder["floating"]
                capture("dev-console-finder", "finder-after-hover.png")
                key(finder, "native-desktop-probe")
                key(finder, "-k", "Return")
                wait("dev-console-finder", False)
                assert run("wl-paste", "--no-newline").stdout == b"printf synthetic-native-output"
                # Find only this disposable process's tray connection.
                tray = None
                for line in run("busctl", "--user", "list", "--no-legend").stdout.decode().splitlines():
                    parts = line.split()
                    if len(parts) > 1 and parts[1] == str(app.pid):
                        try:
                            value = run("busctl", "--user", "get-property", parts[0], "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Id").stdout
                            if value.strip() == b's "dev-console"':
                                tray = parts[0]
                                break
                        except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                            pass
                assert tray, "tray registration not found"
                layout = run("busctl", "--user", "call", tray, "/MenuBar", "com.canonical.dbusmenu", "GetLayout", "iias", "0", "1", "0").stdout
                assert b'10 1 "label" s "Quit"' in layout
                # Quit must let a dirty editor stay open. Escape chooses Keep editing.
                library = own("Dev Console")
                key(library, "-M", "ctrl", "-k", "n", "-m", "ctrl")
                key(library, "native-unsaved-probe")
                run("busctl", "--user", "call", tray, "/MenuBar", "com.canonical.dbusmenu", "Event", "isvu", "10", "clicked", "i", "0", "0")
                capture("Dev Console", "unsaved-quit.png")
                key(library, "-k", "Escape")
                assert app.poll() is None
                key(library, "-k", "Tab", "-k", "Tab", "-k", "Tab", "true", "-M", "ctrl", "-k", "s", "-m", "ctrl")
                with sqlite3.connect(db) as conn:
                    assert conn.execute("SELECT count(*) FROM entries WHERE name = ?", ("native-unsaved-probe",)).fetchone()[0] == 1
                run(binary, "--finder", env=env)
                finder = wait("dev-console-finder")
                key(finder, "synthetic-desktop-token")
                key(finder, "-k", "Return")
                wait("dev-console-finder", False)
                assert run("wl-paste", "--no-newline").stdout == b"synthetic-desktop-value"
                run("busctl", "--user", "call", tray, "/MenuBar", "com.canonical.dbusmenu", "Event", "isvu", "10", "clicked", "i", "0", "0")
                assert app.wait(timeout=8) == 0
                clipboard = subprocess.run(["wl-paste", "--no-newline"], capture_output=True, timeout=3)
                assert not clipboard.stdout, "Quit left owned clipboard content"
                report = dict(finder_cycles=14, pointer_crossings=5, finder_size=[680, 420],
                              library_tiled=True, neighbouring_layout_unchanged=True,
                              create_save=True, completion_arrows_escape=True, exact_copy=True, exact_secret_copy=True,
                              unsaved_quit_cancel=True, tray_quit=True, owned_clipboard_cleared=True)
                (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
                print(f"PASS: native Hyprland lifecycle, keyboard, copy and tray Quit. Evidence: {output}")
                return 0
            finally:
                if app.poll() is None:
                    app.terminate()
                    try:
                        app.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        app.kill()
                        app.wait()


if __name__ == "__main__":
    raise SystemExit(main())
