#!/usr/bin/env python3
"""Set the same release version in the frontend, Tauri config, and every Rust package.

The Cargo workspace at the repo root inherits one version from `[workspace.package]`, and
`dev-console-core` stamps it into the sealed-backup header at compile time. That string has to keep
matching the release the Tauri app reports, so the workspace is stamped here together with
`src-tauri` for as long as both builds exist (plan section 4.6 removes the Tauri one)."""

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

for name, pattern, expected in (
    ("src-tauri/Cargo.toml", r'(?m)^(version = ")[^"]+(".*)$', 1),
    ("src-tauri/Cargo.lock", r'(?m)^(name = "dev-console"\nversion = ")[^"]+(".*)$', 1),
    # `[workspace.package] version` is the only bare `version =` line in the workspace manifest;
    # dependency requirements there are always inline tables.
    ("Cargo.toml", r'(?m)^(version = ")[^"]+(".*)$', 1),
    (
        "Cargo.lock",
        r'(?m)^(name = "dev-console-(?:core|desktop|release|ui-tests)"\nversion = ")[^"]+(".*)$',
        4,
    ),
):
    path = root / name
    updated, count = re.subn(
        pattern, lambda match: match[1] + version + match[2], path.read_text(), count=expected
    )
    if count != expected:
        raise SystemExit(f"could not update {name} (found {count}, expected {expected})")
    path.write_text(updated)
