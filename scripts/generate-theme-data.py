#!/usr/bin/env python3
"""Generate the Rust theme data from the shipped stylesheets and contrast audit.

Plan section 2 / checkpoint C5 moves the design tokens out of `src/styles/*.css` and
`scripts/contrast-check.mjs` into shared Rust data in `crates/dev-console-core/src/theme/`
so the Rune app and the Svelte frontend cannot drift apart on a colour value.

The data is generated, not hand-transcribed: 101 token names x 24 palettes plus an 80-row
contrast table is exactly the kind of surface where a hand copy silently changes one digit.
Every structural promise the generator depends on is checked here and re-checked at test time
by `theme::inventory` (which re-parses the same two stylesheets from Rust), so a retune that
skips one palette fails the test suite even if nobody remembers to run this script.

Usage:
  python3 scripts/generate-theme-data.py            write the generated files
  python3 scripts/generate-theme-data.py --report   print the parsed inventory, write nothing

Fails loudly (exit 1) on any structural drift it cannot represent.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOKENS = ROOT / "src" / "styles" / "tokens.css"
OMARCHY = ROOT / "src" / "styles" / "omarchy.css"
AUDIT = ROOT / "scripts" / "contrast-check.mjs"
OUT = ROOT / "crates" / "dev-console-core" / "src" / "theme"

DECL = re.compile(r"(--[\w-]+)\s*:\s*([^;]+);")
BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.S)

failures: list[str] = []


def fail(message: str) -> None:
    failures.append(message)


def declarations(body: str) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = []
    for name, raw in DECL.findall(BLOCK_COMMENT.sub("", body)):
        value = raw.strip()
        if ";" in value:
            fail(f"declaration value still holds a semicolon: {name}: {value}")
        out.append((name, value))
    return out


def unique(rows: list[tuple[str, str]], label: str) -> dict[str, str]:
    seen: dict[str, str] = {}
    for name, value in rows:
        if name in seen:
            fail(f"{label}: token {name} is declared twice")
        seen[name] = value
    return seen


# --------------------------------------------------------------- stylesheets --


def parse_tokens() -> tuple[dict[str, str], list[tuple[str, dict[str, str], str]]]:
    """Return the shared `:root` block and the built-in dark/light palettes."""
    css = TOKENS.read_text()

    base = re.search(r"^:root\s*\{(.*?)\n\}", css, re.S | re.M)
    if not base:
        raise SystemExit("tokens.css: no base ':root' block")
    shared = unique(declarations(base.group(1)), "tokens.css :root")
    scheme = re.search(r"^\s*color-scheme:\s*(\w+)\s*;", base.group(1), re.M)
    if not scheme:
        raise SystemExit("tokens.css: the shared :root block declares no color-scheme")
    shared_scheme = scheme.group(1)

    def block(pattern: str, label: str) -> tuple[dict[str, str], str]:
        match = re.search(pattern, css, re.S)
        if not match:
            raise SystemExit(f"tokens.css: no block matching the {label} palette")
        rows = unique(declarations(match.group(1)), f"tokens.css {label}")
        scheme = re.search(r"^\s*color-scheme:\s*(\w+)\s*;", match.group(1), re.M)
        return rows, (scheme.group(1) if scheme else shared_scheme)

    dark, dark_scheme = block(r":root,\s*:root\[data-theme='dark'\]\s*\{(.*?)\n\}", "dark")
    light, light_scheme = block(r":root\[data-theme='light'\]\s*\{(.*?)\n\}", "light")
    system, _ = block(
        r"@media \(prefers-color-scheme: light\)\s*\{\s*:root\s*\{(.*?)\n {2}\}",
        "system light",
    )

    if not failures:
        if list(dark) != list(light) or list(dark) != list(system):
            fail("the dark and light palettes do not declare the same token names in the same order")
        if light != system:
            fail("the two light palettes disagree; tokens.css says they must stay identical")
        if dark_scheme != "dark" or light_scheme != "light":
            fail(f"unexpected color-scheme: dark={dark_scheme} light={light_scheme}")

    return shared, [("dark", dark, dark_scheme), ("light", light, light_scheme)]


def parse_omarchy(dark: dict[str, str]) -> list[tuple[str, dict[str, str], str]]:
    css = OMARCHY.read_text()
    out: list[tuple[str, dict[str, str], str]] = []
    for name, body in re.findall(
        r":root\[data-theme='(omarchy-[\w-]+)'\]\s*\{(.*?)\n\}", css, re.S
    ):
        rows = unique(declarations(body), f"omarchy.css {name}")
        scheme = re.search(r"^\s*color-scheme:\s*(\w+)\s*;", body, re.M)
        if not scheme:
            fail(f"omarchy.css {name}: the block declares no color-scheme")
            continue
        if list(rows) != list(dark):
            fail(f"omarchy.css {name}: its token names/order differ from the dark palette")
        if scheme.group(1) not in ("dark", "light"):
            fail(f"omarchy.css {name}: unsupported color-scheme {scheme.group(1)}")
        out.append((name, rows, scheme.group(1)))
    return out


# ---------------------------------------------------------- contrast audit --


def balanced(text: str, start: int) -> str:
    """Return the text between the bracket at `start` and its match (exclusive)."""
    opener = text[start]
    closer = {"[": "]", "{": "}", "(": ")"}[opener]
    depth = 0
    quote = ""
    index = start
    while index < len(text):
        char = text[index]
        if quote:
            if char == "\\":
                index += 2
                continue
            if char == quote:
                quote = ""
        elif char in "'\"`":
            quote = char
        elif char == "/" and text[index + 1] == "*":
            index = text.index("*/", index) + 1
        elif char in "[{(":
            depth += 1
        elif char in "]})":
            depth -= 1
            if depth == 0:
                return text[start + 1 : index]
        index += 1
    raise SystemExit("contrast-check.mjs: unbalanced bracket while parsing")


def split_items(body: str) -> list[str]:
    """Split a JS list/object body on top-level commas, ignoring quotes and comments."""
    items: list[str] = []
    current = ""
    depth = 0
    quote = ""
    index = 0
    while index < len(body):
        char = body[index]
        if quote:
            current += char
            if char == "\\":
                current += body[index + 1]
                index += 2
                continue
            if char == quote:
                quote = ""
            index += 1
            continue
        if char in "'\"":
            quote = char
            current += char
            index += 1
            continue
        if char == "/" and body[index : index + 2].startswith("/*"):
            index = body.index("*/", index) + 2
            continue
        if char in "[{(":
            depth += 1
        elif char in "]})":
            depth -= 1
        if char == "," and depth == 0:
            if current.strip():
                items.append(current.strip())
            current = ""
        else:
            current += char
        index += 1
    if current.strip():
        items.append(current.strip())
    return items


def unquote(text: str) -> str:
    text = text.strip()
    if text[0] in "'\"`" and text[-1] == text[0]:
        return text[1:-1]
    return text


def expand_layers(text: str, surfaces: dict[str, list[str]], context: str) -> list[str]:
    """Resolve one layer list: an inline `[a, ...NAME]`, a bare `NAME`, or a single token."""
    text = text.strip()
    parts = split_items(text[1:-1]) if text.startswith("[") else [text]
    layers: list[str] = []
    for part in parts:
        part = part.strip()
        if part.startswith("..."):
            name = part[3:].strip()
            if name not in surfaces:
                raise SystemExit(f"contrast-check.mjs: {context} spreads unknown surface {name}")
            layers.extend(surfaces[name])
        elif part in surfaces:
            layers.extend(surfaces[part])
        else:
            token = unquote(part)
            if not token.startswith("--"):
                raise SystemExit(f"contrast-check.mjs: {context} layer {part!r} is not a token")
            layers.append(token)
    if not layers:
        raise SystemExit(f"contrast-check.mjs: {context} declares no layers")
    return layers


def parse_audit() -> tuple[dict[str, float], list[dict], list[tuple[str, str]]]:
    js = AUDIT.read_text()

    thresholds = {}
    threshold_body = balanced(js, js.index("{", js.index("const THRESHOLD")))
    for item in split_items(threshold_body):
        key, _, value = item.partition(":")
        thresholds[unquote(key)] = float(value)

    surfaces: dict[str, list[str]] = {}
    if not {"text", "boundary"} <= set(thresholds):
        raise SystemExit("contrast-check.mjs: THRESHOLD must declare text and boundary")
    for name in ("SHELL", "FINDER_ROW", "FINDER_HOVER", "FINDER_SEARCH", "LIST_ROW",
                 "LIST_SELECTED", "FIELD", "INSET_FIELD"):
        marker = f"const {name} = ["
        body = balanced(js, js.index(marker) + len(marker) - 1)
        surfaces[name] = expand_layers("[" + body + "]", surfaces, name)

    marker = "const CHECKS = ["
    checks_body = balanced(js, js.index(marker) + len(marker) - 1)
    groups = []
    for match in re.finditer(r"group\(\s*'((?:[^'\\]|\\.)*)'\s*,\s*\[", checks_body):
        rows_start = checks_body.index("[", match.end() - 1)
        rows_body = balanced(checks_body, rows_start)
        rows = []
        for item in split_items(rows_body):
            if not item.startswith("["):
                raise SystemExit(f"contrast-check.mjs: unexpected check row {item!r}")
            fields = split_items(item[1:-1])
            if len(fields) != 4:
                raise SystemExit(f"contrast-check.mjs: check row needs 4 fields: {item!r}")
            token, layers_text, kind, where = fields
            layers = expand_layers(layers_text, surfaces, unquote(token))
            if unquote(kind) not in ("text", "boundary"):
                raise SystemExit(f"contrast-check.mjs: unknown check kind {kind!r}")
            if not unquote(token).startswith("--"):
                raise SystemExit(f"contrast-check.mjs: check row for {token!r} is not a token")
            rows.append((unquote(token), layers, unquote(kind), unquote(where)))
        groups.append((match.group(1), rows))

    marker = "const EXEMPT = {"
    exempt = []
    for item in split_items(balanced(js, js.index(marker) + len(marker) - 1)):
        key, _, value = item.partition(":")
        exempt.append((unquote(key), unquote(value)))
    return thresholds, groups, exempt


# ----------------------------------------------------------------- emitting --


def lit(text: str) -> str:
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


HEADER = """// @generated by scripts/generate-theme-data.py from src/styles/tokens.css,
// src/styles/omarchy.css and scripts/contrast-check.mjs. Do not edit by hand: the
// values are a transcription of the shipped stylesheet and `theme::inventory` re-parses
// that stylesheet at test time and fails if any row here no longer matches it.
"""

PALETTES_PER_FILE = 6


def emit_shared(shared: dict[str, str]) -> str:
    rows = "\n".join(f"    ({lit(name)},\n     {lit(value)})," for name, value in shared.items())
    return (
        HEADER
        + "\n/// Every non-colour token in `src/styles/tokens.css`'s shared `:root` block: the type\n"
        + "/// scale, spacing, radii, borders, control sizes and motion.\n"
        + "pub(crate) static SHARED_ROWS: &[(&str, &str)] = &[\n"
        + rows
        + "\n];\n"
    )


def emit_palette_set(name: str, rows: dict[str, str], scheme: str) -> list[str]:
    ident = name.replace("-", "_").upper()
    body = "\n".join(f"    ({lit(token)},\n     {lit(value)})," for token, value in rows.items())
    return [
        f"pub(crate) static {ident}_ROWS: &[(&str, &str)] = &[\n{body}\n];\n",
        f"pub(crate) static {ident}: Palette = Palette {{\n"
        f"    name: {lit(name)},\n"
        f"    appearance: Appearance::{scheme.title()},\n"
        f"    rows: {ident}_ROWS,\n}};\n",
    ]


def emit_palettes(palettes: list[tuple[str, dict[str, str], str]]) -> dict[str, str]:
    files: dict[str, str] = {"mod.rs": ""}
    registry: list[str] = []
    mods: list[str] = []
    for index in range(0, len(palettes), PALETTES_PER_FILE):
        chunk = palettes[index : index + PALETTES_PER_FILE]
        number = index // PALETTES_PER_FILE + 1
        filename = f"set_{number}.rs"
        mods.append(f"mod set_{number};\n")
        parts = [HEADER, "\nuse super::Palette;\nuse crate::theme::Appearance;\n"]
        for name, rows, scheme in chunk:
            parts.append("\n" + "\n".join(emit_palette_set(name, rows, scheme)))
            registry.append(f"    &set_{number}::{name.replace('-', '_').upper()},")
        files[filename] = "".join(parts) + "\n"

    files["mod.rs"] = (
        HEADER
        + "\n//! The palette registry, in the order the stylesheets declare the palettes: the\n"
        + "//! settings list renders this order, so re-sorting it is a user-visible change. The\n"
        + "//! rows themselves live in the `set_*` files to keep each one readable.\n\n"
        + "use crate::theme::Palette;\n\n"
        + "".join(mods)
        + "\n/// One entry per palette: `dark`, `light`, then the 22 Omarchy palettes.\npub(crate) static PALETTES: &[&Palette] = &[\n"
        + "\n".join(registry)
        + "\n];\n"
    )
    return files


def emit_checks(thresholds: dict[str, float], groups, exempt) -> str:
    out = [
        HEADER,
        "\n//! The contrast audit table, transcribed from `scripts/contrast-check.mjs`: the same\n"
        "//! pairings, the same thresholds, the same exemptions and reasons.\n",
        "\nuse crate::theme::contrast::{CheckGroup, CheckRow, Kind};\n",
        "\n/// `>= 4.5:1` for normal text, `>= 3.0:1` for a meaningful non-text boundary\n"
        "/// (WCAG 1.4.11). These are the thresholds the shipped audit measures against.\n"
        "pub(crate) const THRESHOLD_TEXT: f64 = " + f"{float(thresholds['text'])!r};\n",
        f"pub(crate) const THRESHOLD_BOUNDARY: f64 = {float(thresholds['boundary'])!r};\n",
        "\npub(crate) static CHECK_GROUPS: &[CheckGroup] = &[\n",
    ]
    for name, rows in groups:
        out.append(f"    CheckGroup {{\n        name: {lit(name)},\n        rows: &[\n")
        for token, layers, kind, where in rows:
            stack = ", ".join(lit(layer) for layer in layers)
            out.append(
                f"            CheckRow {{\n                token: {lit(token)},\n"
                f"                layers: &[{stack}],\n"
                f"                kind: Kind::{kind.title()},\n"
                f"                note: {lit(where)},\n            }},\n"
            )
        out.append("        ],\n    },\n")
    out.append("];\n\n/// Colour tokens the audit deliberately does not gate, with the shipped reason.\n")
    out.append("pub(crate) static EXEMPT: &[(&str, &str)] = &[\n")
    for token, reason in exempt:
        out.append(f"    ({lit(token)},\n     {lit(reason)}),\n")
    out.append("];\n")
    return "".join(out)


def report(shared, palettes, thresholds, groups, exempt) -> None:
    print(f"shared tokens       {len(shared)}")
    print(f"palettes            {len(palettes)}")
    for name, rows, scheme in palettes[:3]:
        print(f"  {name:<28} {scheme:<5} {len(rows)} tokens")
    print(f"thresholds          {thresholds}")
    print(f"check groups        {len(groups)}  rows {sum(len(r) for _, r in groups)}")
    for name, rows in groups:
        kinds = {kind for _, _, kind, _ in rows}
        print(f"  {name:<48} {len(rows):>3} rows  {sorted(kinds)}")
    print(f"exempt tokens       {len(exempt)}")


def main() -> int:
    shared, builtin = parse_tokens()
    dark = dict(builtin[0][1])
    omarchy = parse_omarchy(dark)
    palettes = builtin + omarchy
    thresholds, groups, exempt = parse_audit()

    if "--report" in sys.argv:
        report(shared, palettes, thresholds, groups, exempt)
        return 1 if failures else 0

    if failures:
        for problem in failures:
            print(f"generate-theme-data: {problem}", file=sys.stderr)
        return 1

    (OUT / "palettes").mkdir(parents=True, exist_ok=True)
    (OUT / "shared.rs").write_text(emit_shared(shared))
    for filename, text in emit_palettes(palettes).items():
        (OUT / "palettes" / filename).write_text(text)
    (OUT / "checks.rs").write_text(emit_checks(thresholds, groups, exempt))
    report(shared, palettes, thresholds, groups, exempt)
    print("wrote theme/shared.rs, theme/palettes/*, theme/checks.rs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
