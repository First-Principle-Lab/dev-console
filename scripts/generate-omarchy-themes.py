#!/usr/bin/env python3
"""Snapshot installed Omarchy colors.toml files into Dev Console theme tokens."""
import sys
import tomllib
from pathlib import Path

source = Path(sys.argv[1] if len(sys.argv) > 1 else '/usr/share/omarchy/themes')
out = Path(__file__).resolve().parents[1] / 'src/styles/omarchy.css'

def rgb(value):
    return tuple(int(value.lstrip('#')[i:i+2], 16) for i in (0, 2, 4))

def hexcolor(value):
    return '#' + ''.join(f'{round(x):02x}' for x in value)

def mix(a, b, portion):
    return tuple(a[i] * (1 - portion) + b[i] * portion for i in range(3))

def luminance(value):
    channels = [x / 255 for x in value]
    linear = [x / 12.92 if x <= .04045 else ((x + .055) / 1.055) ** 2.4 for x in channels]
    return sum(x * y for x, y in zip(linear, (.2126, .7152, .0722)))

def contrast(a, b):
    one, two = sorted((luminance(a), luminance(b)), reverse=True)
    return (one + .05) / (two + .05)

def legible(value, backgrounds, threshold, light):
    destination = (0, 0, 0) if light else (255, 255, 255)
    for step in range(101):
        candidate = mix(value, destination, step / 100)
        if all(contrast(candidate, bg) >= threshold for bg in backgrounds):
            return candidate
    return destination

def rgba(value, opacity):
    return f'rgba({round(value[0])}, {round(value[1])}, {round(value[2])}, {opacity})'

blocks = ['/* Snapshot of stock Omarchy themes. Regenerate with scripts/generate-omarchy-themes.py. */']
for path in sorted(source.glob('*/colors.toml')):
    data = tomllib.loads(path.read_text())
    light = data['mode'] == 'light'
    bg = rgb(data['background'])
    raised = rgb(data['lighter_background'])
    sunken = rgb(data['dark_background'])
    active = rgb(data['selection'])
    selected = mix(bg, rgb(data['accent']), .16)
    surfaces = [bg, raised, sunken, active, selected]
    fg = legible(rgb(data['foreground']), surfaces, 5.2, light)
    muted = legible(rgb(data['light_foreground']), surfaces, 4.6, light)
    dim = legible(rgb(data['dark_foreground']), surfaces, 4.6, light)
    accent = legible(rgb(data['accent']), surfaces, 4.6, light)
    accent_strong = legible(mix(accent, (0, 0, 0) if light else (255, 255, 255), .09), surfaces, 4.6, light)
    accent_fg = (0, 0, 0) if contrast((0, 0, 0), accent) > contrast((255, 255, 255), accent) else (255, 255, 255)
    for value in (accent, accent_strong):
        if contrast(accent_fg, value) < 4.5:
            accent_fg = (0, 0, 0) if light else (255, 255, 255)
    danger = legible(rgb(data['red']), surfaces, 4.6, light)
    ok = legible(rgb(data['green']), surfaces, 4.6, light)
    warn = legible(rgb(data['yellow']), surfaces, 4.6, light)
    secret = legible(rgb(data.get('orange', data['magenta'])), surfaces, 4.6, light)
    line = mix(bg, fg, .18)
    line_strong = mix(bg, fg, .3)
    soft_opacity = .09
    tokens = {
        'bg': hexcolor(bg), 'bg-panel': hexcolor(bg), 'bg-raised': hexcolor(raised),
        'bg-sunken': hexcolor(sunken), 'bg-hover': hexcolor(mix(raised, fg, .05)),
        'bg-active': hexcolor(active), 'line': hexcolor(line), 'line-strong': hexcolor(line_strong),
        'fg': hexcolor(fg), 'fg-muted': hexcolor(muted), 'fg-dim': hexcolor(dim),
        'shell-bg': hexcolor(bg), 'shell-edge': rgba(fg, .08),
        'line-shell': hexcolor(line), 'line-hair': rgba(fg, .09),
        'shadow-shell': '0 24px 60px rgba(0, 0, 0, 0.45), 0 2px 8px rgba(0, 0, 0, 0.25)' if not light else '0 20px 48px rgba(0, 0, 0, 0.18), 0 2px 6px rgba(0, 0, 0, 0.1)',
        'shadow-raised': '0 4px 14px rgba(0, 0, 0, 0.3)' if not light else '0 3px 10px rgba(0, 0, 0, 0.12)',
        'inset-fill': rgba((0,0,0) if not light else fg, .17 if not light else .05),
        'search-fg': 'var(--fg)', 'search-placeholder': 'var(--fg-dim)',
        'search-focus-bg': rgba(fg, .04), 'caret': 'var(--accent)',
        'accent': hexcolor(accent), 'accent-strong': hexcolor(accent_strong),
        'accent-fg': hexcolor(accent_fg), 'accent-soft': rgba(accent, soft_opacity),
        'accent-line': rgba(accent, .45),
        'selected': rgba(accent, .16), 'selected-fg': 'var(--fg)',
        'selected-edge': rgba(accent, .3),
        'selected-marker': 'inset var(--edge-w) 0 0 var(--accent)',
        'hover-fill': rgba(fg, .05), 'material-blur': '0px', 'material-tint': 'transparent',
        'danger': hexcolor(danger), 'danger-soft': rgba(danger, soft_opacity),
        'ok': hexcolor(ok), 'ok-soft': rgba(ok, soft_opacity),
        'warn': hexcolor(warn), 'warn-soft': rgba(warn, soft_opacity),
        'command-accent': 'var(--accent)', 'secret-accent': hexcolor(secret),
        'secret-soft': rgba(secret, soft_opacity),
        'kind-command': 'var(--command-accent)', 'kind-secret': 'var(--secret-accent)',
        'focus-ring': hexcolor(accent),
        'backdrop': 'rgba(8, 9, 12, 0.62)' if not light else 'rgba(24, 26, 32, 0.4)',
        'footer-fg': 'var(--fg-dim)', 'shadow-dialog': 'var(--shadow-shell)',
    }
    blocks.append(f"\n:root[data-theme='omarchy-{path.parent.name}'] {{")
    blocks.append(f"  color-scheme: {'light' if light else 'dark'};")
    blocks += [f'  --{key}: {value};' for key, value in tokens.items()]
    blocks.append('}')
out.write_text('\n'.join(blocks) + '\n')
print(f'Wrote {len(list(source.glob("*/colors.toml")))} themes to {out}')
