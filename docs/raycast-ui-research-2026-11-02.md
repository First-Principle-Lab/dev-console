# Raycast research note - 2026-11-02

Dated companion to `docs/raycast-ui-redesign.md` (step 1 of that brief's implementation order).
It records what was actually looked at, what was taken from it, and the exact Dev Console token
that each decision turns into. Reference images are linked, not committed: the Raycast artwork is
copyrighted and this product must not ship a copy of it.

## What was reviewed

| Reference | What it is | Used for |
| --- | --- | --- |
| <https://www.raycast.com/uploads/blog-new-raycast/app.png> | The May 2026 launcher screenshot. Primary reference, as named in the brief. | Overall shell, search line prominence, selected band, row pattern, footer weight. |
| <https://www.raycast.com/blog/the-new-raycast> | Launch article for the current launcher. | Confirmation that the launcher is one floating panel with a dominant search line and a compact action row. |
| <https://manual.raycast.com/new-in-v2> | v2 manual. | Which surfaces carry material and which stay flat/stable. |
| <https://www.raycast.com/blog/a-technical-deep-dive-into-the-new-raycast> | Technical deep dive on the material. | Why CSS `backdrop-filter` alone cannot reproduce desktop blur, and therefore why the shipped Linux default stays an opaque shell. |
| <https://www.raycast.com/press> | Press kit (dark and light images, some of the older UI). | Checked, then set aside: some frames depict the pre-v2 UI, so they were not used for any value below. |

Nothing below is an official Raycast token. There is no published Raycast colour, radius, blur,
font or spacing sheet, so **no value in this note may be described as a measured Raycast value**.
Each value is a Dev Console starting point chosen from our own screenshots at 100% scale, inside
the ranges the brief proposes.

## Our own before state, read at 100%

Files: `artifacts/ui-review/raycast-baseline/13-finder-palette-680x420__680.png` and
`.../01-library-commands__1440.png` (regenerated from the unmodified source at `291a6a6`; see
Capture protocol below for how they relate to the `after/` set the brief cites).

Finder at the real 680 x 420 window size:

1. The search line is one text-height tall with a thin bright indigo rule under it, so the rule -
   not the input - is the brightest thing in the window.
2. The selected row is a 1px bright outline plus a solid left bar, i.e. the loudest element after
   the rule, and it reads as "focused" rather than "selected"; there is no soft fill to carry the
   selection once focus styling is ignored.
3. Every row is two lines (name + description) and carries a kind pill on the right; the selected
   row additionally carries an "Enter" chip. Bordered and chipped surfaces compete inside one list.
4. Rows are tall, so five results already leave a large dead band above the footer at 420 px.
5. The footer is a full-width bar with its own rule, reading as a second window rather than part
   of the palette.

Library at 1440 x 900:

6. The detail header shows six chips of equal prominence (`Command`, `Shell`, `#work`, `#ci`,
   `Pinned`, `r4`), so the one chip that separates kinds is not more readable than a tag.
7. Borders stack: pane divider, boxed search field, boxed command block, two boxed custom fields,
   outlined selected row - five nested rectangles before any content.
8. The active tab underline and the header rule are both bright, so the header owns more attention
   than the entry title.

## Observations mapped to Dev Console decisions

| # | Observation in the reference | Decision for Dev Console | Token(s) | Value (dark / light) |
| --- | --- | --- | --- | --- |
| 1 | One floating rounded panel with a fine light edge and a broad soft shadow; text stays legible over a busy desktop. | Give the shell its own radius, edge and elevation instead of reusing control tokens; keep the shell fill an opaque shaded graphite because native compositor blur is unproven on this host. | `--radius-shell`, `--shell-edge`, `--shell-bg`, `--shadow-shell`, `--shadow-raised` | shell radius 14px; edge `rgba(255,255,255,0.07)` / `rgba(255,255,255,0.75)`; shell bg `#1b1d22` / `#ffffff`; shell shadow `0 24px 60px rgba(0,0,0,0.55), 0 2px 8px rgba(0,0,0,0.35)` / `0 20px 48px rgba(20,22,28,0.20), 0 2px 6px rgba(20,22,28,0.10)` |
| 2 | A prominent search line with a large placeholder and generous space, and no heavy boxed input inside the window. | Make the search line the tallest thing in the palette, drop its box and the bright rule, and express focus with a caret colour plus a soft field wash. | `--search-h`, `--text-search`, `--search-fg`, `--search-placeholder`, `--search-focus-bg`, `--caret` | search height 46px; search text 17px; placeholder `#8d93a1` / `#6b7280`; focus wash `rgba(255,255,255,0.04)` / `rgba(27,29,35,0.04)`; caret = `var(--accent)` |
| 3 | Muted group labels; the selected row is a soft rounded band, not a bright outline. | Selection becomes a filled rounded band at low accent opacity with a quiet inner edge, and a 2px accent marker so it is still perceivable without colour. Hover becomes a neutral fill, visibly weaker than selection. | `--selected`, `--selected-fg`, `--selected-edge`, `--selected-marker`, `--hover-fill`, `--radius-row` | selected `rgba(124,140,248,0.16)`, edge `rgba(124,140,248,0.28)`, marker 2px solid `var(--accent)`; hover `rgba(255,255,255,0.05)` / `rgba(27,29,35,0.05)`; row radius 8px |
| 4 | Result rows read as: icon, name, quieter secondary text, one right-aligned action. | One row pattern only: kind is carried by the icon tint plus the right-aligned action, so the per-row kind pill goes away; secondary context is one line, dimmer and smaller than the name. | `--text-row`, `--text-row-meta`, `--row-h`, `--kind-command`, `--kind-secret` | row text 13.5px, meta 11.5px, row height 40px; kind colours reuse `--command-accent` / `--secret-accent` |
| 5 | A compact bottom action area that feels attached to the palette, not a separate toolbar. | Footer loses its own rule and full-bleed bar treatment; it becomes quiet text on the shell with the existing keycap styling, and its labels must match real bindings (Up/Down select, Tab completes, Enter copies, Ctrl+O details, Escape dismisses). | `--footer-h`, `--footer-fg`, `--line-footer` (removed usage) | footer height 34px; footer text `var(--fg-dim)` |
| 6 | Restrained glass: only the shell and floating surfaces carry material. | Material is opt-in and off by default. Panels that hold editable or long text keep a stable opaque surface. `backdrop-filter` is never applied to rows. | `--material-blur`, `--material-tint`, `[data-transparency='off']` fallback | `--material-blur: 0px` shipped default; prototype value 24px is documented as UNVERIFIED, not shipped |
| 7 | Fine light edge and low-contrast dividers rather than full-strength borders. | Split the single border token into a hairline for internal separators and keep the strong border for real control edges. | `--line-hair`, `--line-shell`, `--border-hair` | hairline `rgba(255,255,255,0.06)` / `rgba(27,29,35,0.10)`; shell line `#2a2e36` / `#dfe2e9` |
| 8 | Legible text over a busy background: body text is near-white, secondary is clearly secondary but not greyed out. | Re-tune the neutral ramp so `--fg-muted` clears 4.5:1 on both shell and panel fills, and reserve `--fg-dim` for non-essential metadata only (it clears 3:1, which is the correct bar for large/decorative text, never body copy). | `--fg`, `--fg-muted`, `--fg-dim`, `--bg-shell` | muted `#a6acb9` / `#575d6b`; dim `#8b91a0` / `#6b7280` |

## Deliberately not copied

- Raycast's logo, icon set, branded red, artwork and macOS window controls.
- Any claim of pixel or token identity with Raycast, and any claim that Dev Console has native
  "glass" on Linux: compositor blur needs a Wayland/X11 compositor plus transparent window and
  WebView support, which this environment cannot prove. See the material level recorded in
  `docs/raycast-ui-redesign.md` and the handoff checklist.

## Capture protocol used for this redesign

Two new `SHOT_SET` values in `tests/e2e/screenshots.spec.ts`; the existing `before`, `after` and
`native` sets and their PNGs are left in place and are never rewritten again:

- `raycast-baseline` - the untouched pre-redesign UI, generated at commit `291a6a6` with
  `SHOT_SET=raycast-baseline SHOT_HTML=1 bunx playwright test screenshots`.
- `raycast-after` - the redesigned UI, captured the same way after the token and component work.
  This is now also the spec's default `SHOT_SET` (it used to be `after`), so that a plain
  `bunx playwright test` - which is what `scripts/verify.sh` runs - writes the new set instead of
  overwriting the frozen `after/` record with this machine's font rendering. `raycast-after/` is
  intentionally not generated yet: until the visual work lands it would show the old UI.

What "untouched baseline" is proved by, and what it is not:

- Proved: `git diff --stat artifacts/ui-review/after` is empty and `git status` lists nothing new
  under `artifacts/ui-review/` except this baseline set and its checksum snapshot, so the 41
  tracked `after/` PNGs are still byte-for-byte the committed ones; and the captures were taken
  with `src/`, `src-tauri/` and `tests/` at their `291a6a6` content (the only test change is the
  capture helper in `tests/e2e/screenshots.spec.ts`, which cannot alter what is drawn).
- Not claimed: byte parity between `raycast-baseline/` and `after/`. They are the same UI, but the
  committed set was captured on another machine, so font hinting differs (all 41 files differ by
  md5; total size within 0.3%, worst single file 5.9%). Byte parity is also not available inside
   one host: re-running the set twice on this machine reproduced 39 of 41 PNGs exactly, with
   `04-vault-locked__1440.png` and `11-finder-copied__768.png` differing run to run; both are the
   time-sensitive states (a relative timestamp, and the copy confirmation). Treat pixel parity as
   a visual review, or compare a pair captured on the same host in the same session.

`capture()` also asserts that no synthetic secret value from the seed appears in the serialized DOM
of any capture, so masking is proved for every screen rather than asserted in prose. With
`SHOT_HTML=1` it writes that page source per capture under `test-results/screenshots/<set>/`
(gitignored, and wiped by Playwright at the start of the next run, so grep it in the same
invocation):

```sh
SHOT_SET=raycast-baseline SHOT_HTML=1 bunx playwright test screenshots \
  && grep -ho 'synthetic-[a-z0-9-]*' test-results/screenshots/raycast-baseline/*.html | sort -u
```

The grep is empty for all 41 baseline captures, including `06-editor-secret-masked` and
`04-vault-locked`.

`artifacts/ui-review/raycast-baseline.sha256` snapshots the baseline set so a later run can tell
which captures actually moved: `cd artifacts/ui-review/raycast-baseline && sha256sum -c
../raycast-baseline.sha256`. It is a snapshot, not a lock - the two time-sensitive captures listed
above may differ on their own.

## Token pass as landed (same date, step 3)

The token names in the table above are now in `src/styles/tokens.css` and are consumed by the
shared components (`Dialog`, `Tabs`, `EntryList`, `EntryDetails`, `EmptyState`, `StatusLine`,
`VaultStatus`, `Keycap`, `ExtraFields`, `EntryEditor`, `SettingsPanel`) plus the base control rules
in the same file. Where the landed value differs from the table, the reason is measured contrast,
not taste:

| Token | Table | Landed | Why |
| --- | --- | --- | --- |
| `--fg-muted` (light) | `#575d6b` | `#4b5160` | The ramp needs room under `--fg-dim`; 7.94:1 on white, 5.84:1 on the darkest light surface. |
| `--fg-dim` (light) | `#6b7280` | `#5f6673` | `#6b7280` is 4.16:1 on `--bg-raised` and 3.98:1 on `--bg-sunken`, i.e. below the 4.5:1 the brief requires for normal text wherever metadata sits on a filled surface. `#5f6673` is >= 4.62:1 on every light surface it is actually used on. |
| `--bg-shell` | listed as a name | `--shell-bg` | One name for the shell fill; `--shell-edge`, `--line-shell` and `--shadow-shell` keep the `--shell-` prefix. |
| `--selected-marker` | "2px solid" | `inset var(--edge-w) 0 0 var(--accent)` | Stored as the ready-to-use box-shadow so a row cannot accidentally render it as a border. |

Sizing tokens the table implied but did not name: `--edge-w` (2px), `--gap-xs`, `--gap-keys`,
`--gap-tags`, `--badge-min` and the `--tile-sm/md/lg/xl` glyph steps, so that control heights and
gaps stop being per-component literals (the file's own header rule). `--row-h` moved 46px -> 40px,
which is the row height observation 4 asked for. `--search-h`, `--text-search`, `--search-fg`,
`--search-placeholder`, `--search-focus-bg`, `--footer-h` and `--footer-fg` land here as part of the
shared system but are consumed by the window passes (checkpoints C4/C5), not by this step.

Two things this pass deliberately did **not** do:

- It paints no `backdrop-filter` anywhere. `--material-blur` is `0px` in both themes, and
  `data-transparency="on"` / `"off"` / `prefers-reduced-transparency` only move the tint and the
  radius candidate; no component reads the blur until a native capture proves compositor blur
  (step 3 of the brief and checkpoint C9).
- It leaves the per-row kind pill and the bright search rule in `Finder.svelte` in place; those are
  the Finder outcome (checkpoint C4), not the token system.

## Finder pass as landed (same date, brief step 3, checkpoint C4)

`src/windows/Finder.svelte` now renders through the tokens above; no search, selection, copy,
completion, unlock or hide code path changed (only markup and styles). Landed decisions:

- Shell: the palette frame is the shared `.surface-shell` utility (fill, edge, 14px radius,
  elevation from `--shell-bg` / `--shell-edge` / `--radius-shell` / `--shadow-shell`); the
  component's own `background`/`border`/`border-radius` declarations were deleted so the
  global utility is not out-speced by the scoped styles.
- Search line: `--search-h`/`--text-search`/`--search-fg`/`--search-placeholder` are consumed
  here; the boxed `--bg-raised` band, the `inset 0 -2px 0 var(--accent)` focus rule and the
  accent icon are gone. Focus now reads as `--search-focus-bg` across the line plus the
  `--caret`-coloured caret. The completion mirror re-leads on `--search-h` and stays aligned.
- One row pattern: kind glyph tile (`--tile-md`, tinted via `--kind-command`/`--kind-secret`),
  name on `--text-row`, then one context line `type · description · #tags` on
  `--text-row-meta`/`--fg-muted`, then the right-aligned action. The per-row `Command`/`Secret`
  pill and the separate `Unlock` chip are deleted; the action is the `Enter` keycap, shown on
  the selected row only. Both lines lead at `--leading-tight` so the pair fits `--row-h` (40px
  at 680x420 shows seven rows instead of five, closing observation 4's dead band).
- Selection/hover: `.row.active` reuses the `EntryList` recipe verbatim (soft `--selected`
  band + `--selected-edge` + inset `--selected-marker`); hover is the weaker neutral
  `--hover-fill` and never shows the marker. `redesign.spec.ts` now proves both mechanically:
  the computed `box-shadow` of the selected row contains the inset marker and no unselected
  row's does, and a hovered unselected row's computed background differs from the selected
  band. A third new test proves the locked finder keeps the secret's name, type, tag and
  value marker (`Registry token`, `API key`, `infra`, `synthetic-value-1`) out of the
  serialized DOM; a fourth truncates a 116-character name at the real 680px palette width and
  asserts the action stays inside the row box.
- Footer/copied/error: full-width `--bg-raised` bars and their strong rules replaced by quiet
  hairlines (`--border-hair`, `--footer-h`, `--footer-fg`); the copied and copy-failure lines
  use the same attached-line hierarchy. The last hint is labelled `dismiss`, matching what
  Escape does (it was `close`; no test or binding depended on the word).
- Fixed while landing this: the narrow-screen fallback fired at `max-width: 720px`, i.e. at
  every width the *real* 680px window ever has, so the shipped palette was rendering without
  its `Ctrl+O details` / `Esc dismiss` hints. The breakpoint is now 520px and capture 13 shows
  all five hints on one line.

Captures reviewed at 100% (regenerated with `SHOT_HTML=1 bunx playwright test screenshots`;
`capture()` re-proved the synthetic-marker absence on all 41 DOMs):
`artifacts/ui-review/raycast-after/13-finder-palette-680x420__680.png` (the real window size),
`08-finder-results__{1440,768,390}.png`, `09-finder-inline-completion__…`,
`10-finder-no-matches__…`, `11-finder-copied__…`, `12-finder-locked-row__…`.
Pre-change comparison: the same filenames under `artifacts/ui-review/raycast-baseline/`.
Suite on this state: `bun run check` 0/0, eslint clean, vitest 21/21, Playwright 60/60
(57 baseline tests, none altered or removed, plus the three new C4 tests).

`tests/unit/design-tokens.test.ts` now enforces the two mechanical halves of this file's header
promise: dark and *both* light palettes declare the same token names, and no `.svelte`/`.ts` file
under `src/` contains a colour literal. Colour literals outside `tokens.css` went from the recorded
baseline (2 hex, 3 `rgb*(`) to 0 and 0: the last two hex were `var(--warning, #...)` fallbacks for
a token that never existed - the real token is `--warn` - and the `rgb()` triple was three ad-hoc
scrim/inset fills that now read `--backdrop`, `--bg-sunken` and `--inset-fill`.
