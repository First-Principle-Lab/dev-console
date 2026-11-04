#!/usr/bin/env python3
"""Set the same release version in the frontend, Tauri config, and Rust package."""

import json
import re
import sys
from pathlib import Path


version = sys.argv[1] if len(sys.argv) == 2 else ""
if not re.fullmatch(r"0\.0\.[1-9][0-9]*", version):
    raise SystemExit("usage: set-release-version.py 0.0.<positive patch number>")

root = Path(__file__).resolve().parent.parent
for name in ("package.json", "src-tauri/tauri.conf.json"):
    path = root / name
    data = json.loads(path.read_text())
    data["version"] = version
    path.write_text(json.dumps(data, indent=2) + "\n")

for name, pattern in (
    ("src-tauri/Cargo.toml", r'(?m)^(version = ")[^"]+(".*)$'),
    ("src-tauri/Cargo.lock", r'(?m)^(name = "dev-console"\nversion = ")[^"]+(".*)$'),
):
    path = root / name
    updated, count = re.subn(pattern, lambda match: match[1] + version + match[2], path.read_text(), count=1)
    if count != 1:
        raise SystemExit(f"could not update {name}")
    path.write_text(updated)
