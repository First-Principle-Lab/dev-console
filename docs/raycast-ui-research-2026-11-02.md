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

## Contrast pass as landed (same date, brief "Contrast and motion", checkpoint C6)

The contrast promise is now measured, not asserted. `scripts/contrast-check.mjs` reads the three
palette blocks straight out of `src/styles/tokens.css` (it carries no colour values of its own),
resolves `var()` chains including the fallbacks, composites every translucent layer over the
surface it is really painted on, and computes WCAG 2.1 relative-luminance ratios for 76 pairings
per theme. Each pairing names the rule that produces it, e.g. `--footer-fg` on
`--search-focus-bg + --shell-bg` is the Finder footer over the shell. Thresholds: 4.5:1 for text,
3.0:1 for non-text state indicators. Nothing in this UI qualifies as WCAG "large text" (the
largest token is `--text-xl` at 18px/400, and the 3:1 relaxation starts at 24px or 18.66px bold),
so every text pair is held to 4.5:1.

Minimum measured ratios on the landed tokens:

| group | pairs | dark min | light min | threshold |
| --- | --- | --- | --- | --- |
| text on every surface family it is painted on | 46 | 4.71:1 | 4.57:1 | 4.5:1 |
| `:focus-visible` ring on each surface it can land on | 8 | 6.83:1 | 8.00:1 | 3.0:1 |
| selection marker vs its own band and vs an unselected row | 5 | 4.33:1 | 4.80:1 | 3.0:1 |
| caret and field affordance | 4 | 5.02:1 | 4.99:1 | 3.0:1 |
| kind / status tints that carry meaning | 13 | 3.45:1 | 4.01:1 | 3.0:1 |

Three real failures surfaced and were fixed rather than exempted:

- `--fg-dim` (dark) was 4.14:1 on the selected band, where the `EntryList` `.tag`/`.dot` and the
  Finder `.ctx-part` separator actually render. Retuned `#8b91a0` to `#959bac`; the quietest text
  in the app now clears 4.5:1 on the darkest place it appears (`--bg-raised` 5.45:1, selected band 4.76:1)
  while staying visibly below `--fg-muted` (#a6acb9).
- `--search-placeholder` (light) was 4.47:1 over the `--search-focus-bg` wash that the redesign
  puts under it. Retuned `#6b7280` to `#626976` in *both* light palettes.
- Finder's `.hintkeys { opacity: 0.8 }` multiplied the footer keycap's contrast down to roughly
  4.0:1 behind a token the audit cannot see. The rule is deleted (and the now-unused class with
  it); keycaps read at `kbd`'s own `--fg-muted`, and the row action lost only its 20% fade.
  Any future "quieten it with opacity" is now visible to the checker, because opacity is not a
  token and the pair it produces is measured flat.

One pairing was corrected rather than weakened: `.tab.active .count` was `--accent` text on an
`--accent-soft` pill (4.33:1 dark). The tab already declares itself with the `--selected` band,
`--selected-edge` and the 2px `--accent` underline, so the badge is now `--fg` text (8.56:1 dark, 11.69:1 light) and
keeps the tinted pill. The same reasoning is written down for the two remaining sub-3:1 pairs:
`.pin` and the kind glyph tiles are *icons with a text label alongside* (title="Pinned", the name
and type on the same row), so they are audited in the 3:1 non-text class, where they measure
4.33:1 and 3.45:1 in dark.

Exemptions are listed in the script with a reason and are still printed with their measured ratio,
so the run cannot hide one: region separators (`--line`, `--line-shell`, `--line-hair`,
`--shell-edge`), the soft `--selected-edge` (state is carried by the checked inset marker), the
`--accent-line` focus border (it sits on top of the 3:1 `--focus-ring` outline), status *fills*
(`--*-soft`, audited as surfaces with their text on them), `--backdrop`, and `--material-tint`
(transparent by default; the opt-in value is the unverified native prototype). WCAG 1.4.11 does
not require a separator, an inactive component or a boundary that is not the only way to identify
a control to reach 3:1, and every text field this outlines carries a visible `.label` plus the
`--focus-ring` outline. The sweep also *fails* if a colour token is added to a palette and appears
in neither the pairing table nor the exempt list, and if the two light palettes drift apart.

Wiring and live-DOM proof:

- `tests/unit/contrast.test.ts` runs the script inside `bun run test` - i.e. inside
  `scripts/verify.sh` - and prints its grouped output, so a future retune that breaks contrast
  fails the single documented gate. `bun run check:contrast` prints the full 152-row table.
  Verified as a gate: darkening `--fg-dim` to `#7d8390` makes the script exit 1 with five named
  failures.
- Three new Playwright tests measure the same promises from the rendered page instead of the
  stylesheet: (1) both themes apply through the same tokens and the light palette really resolves
  - `.palette` background, footer colour and radius are compared across `data-theme`, the explicit
  light and `prefers-color-scheme` light values are asserted identical, and the footer-to-shell
  ratio is recomputed at >= 4.5:1 from what the browser painted; (2) `reducedMotion: reduce`
  collapses the row transition to < 1ms, `--material-blur` resolves to `0px` with
  `--material-tint: transparent`, `backdrop-filter` is `none` on the shell and on the rows, and
  flipping the unverified `data-transparency='on'` prototype switch to `24px` changes *no painted
  CSS* (the shell keeps the identical opaque fill) because no rule reads the blur token; (3) Tab
  focus draws a solid >= 2px outline, i.e. the focus indicator is a shape, not a hue.
  Playwright's `emulateMedia` has no `prefers-reduced-transparency` knob, so that fallback is
  proved statically: the script's structural section matches the media block and the
  `data-transparency='off'` block and requires both to set `--material-blur: 0px` *and*
  `--material-tint: transparent`.

Suite on this state: `bun run check` 0/0, eslint clean, vitest 22/22, Playwright 63/63 (the 60
prior tests unchanged plus the three new ones), colour literals outside `tokens.css` still 0/0,
`backdrop-filter` occurrences still 0, and `before/`, `after/`, `native/` and `raycast-baseline/`
byte-untouched; `raycast-after/` regenerated (41 PNGs) with the marker-absence assertion passing
on every captured DOM.

## Library, editor, settings and dialogs pass as landed (same date, brief step 4, checkpoint C5)

Five components and one window changed, markup and styles only: `Library.svelte`,
`EntryDetails.svelte`, `EntryEditor.svelte`, `SettingsPanel.svelte`, `VaultStatus.svelte`.
`git diff` has **zero hunks inside any `<script>` block** of those files, so no form field,
save/cancel/dirty rule, backup warning, reveal, lock or clipboard path moved; every one of the 63
earlier Playwright tests still passes unaltered.

New structural tokens (in `:root`, not palettes, next to `--border`/`--border-hair`):
`--border-warn` (the one deliberate frame, `1px solid var(--warn)`, used by every card that asks
before it destroys), `--label-tracking` (0.06em, the group/field label tracking) and
`--field-icon-indent` (26px, the leading-icon inset a filter field needs). Spacing literals in
`SettingsPanel` (`10px`, `12px 16px`, `4px 16px 16px`, `8px`, `5px`, `4px`, `1px`, `3px` and the
`10px`/`1.15` pair in `VaultStatus`) are now `--gap`, `--sp-*`, `--gap-sm`, `--gap-xs`,
`--gap-tags`, `--text-xs` and `--leading-tight`.

- Quiet header, one rule per boundary. The header dropped its own `--border` and its
  `--bg-panel` fill (it is `--bg` now) so the top of the window reads as one chrome area: the only
  lines under the chrome are the tab strip's own track and the toolbar's single `--border-hair`.
  The brand glyph is the shared `--tile-sm`/`--radius-sm` tile on `--accent-soft` with
  `--kind-command`, no frame; the wordmark is `--fg-muted`. `VaultStatus` is a hairline pill whose
  edge only strengthens to `--line-strong` in the unhealthy states. The list/detail split moved
  from `--border` to `--border-shell`: region separators use the shell line, internal rows use the
  hairline, controls use `--line`.
- Detail header: six-to-nine equally prominent chips became **one** chip (the kind, because the
  brief asks for commands and secrets to stay distinguishable) plus one quiet context line
  `type · #tag · ⬆ Pinned · rN · updated …` built from the *same* `.ctx-part` separator idiom,
  `--text-row-meta`, `--fg-muted`/`--fg-dim` and `.pin` accent glyph as a palette row and the entry
  list. Nothing was dropped: `redesign.spec.ts` asserts type, tag, `Pinned`, `rN` and `updated` are
  all still rendered while the number of chips in the inspector is exactly one.
- Stable action area: the inspector's footer is `position: sticky; bottom: 0` on `--bg` behind one
  `--border-hair` (and `.details` gave up its bottom padding so the bar sits flush). Copy / Run /
  Edit / Delete are therefore inside the 760x520 window whether the record is short or scrolling,
  and keyboard focus can never land on an action that is off screen - proved by the new test that
  selects a row by `Enter` at 760x520, tabs to `copy` and asserts its box bottom is within 520px.
- One box recipe: `.code` (command and masked value) moved from `--border` to the `--border-hair`
  + `--bg-sunken` recipe the shared `.block` utility uses, so a command block and the run
  confirmation are the same object; the kind tiles in the inspector and the editor are now the
  shared `--tile-lg`/`--radius-row` tile with `--kind-*`, matching the palette and list rows.
- Editor groups are grouped the way the settings panel groups: a label and a gap, not a box inside
  a box - three frames and their `--bg-panel` fills are gone. A group header is `--text-sm` +
  `--label-tracking` while a field label stays `--text-xs`, so `IDENTITY` and `NAME` can never read
  as two labels of the same rank; `SettingsPanel h4` uses the identical header recipe.
- Settings is now the same floating object as the delete dialog and the palette: its local `.panel`
  frame was replaced by the shared `.surface-shell` utility (the local rule keeps only size and
  flex layout, so it cannot out-spec the shared frame), with `--border-hair` dividers in the header
  and footer. Its argv chips render through the global `.chip` pill (local override deleted) with
  `--gap-tags` rhythm; every confirm card (`export overwrite`, `import review`) draws
  `--border-warn`; the recovery block keeps the `--inset-fill` + hairline inset recipe.
  `redesign.spec.ts` proves the shared frame from computed styles: settings, the delete dialog and
  the palette all report `--radius-shell` and a non-`none` shell shadow.
- The empty library shares the same tokens too (kind tile fill/accent equal to a populated row's,
  the `Super + F` hint chip on `--inset-fill` so a `--bg-raised` keycap does not vanish into it).

Captures inspected at 100% after regeneration (all 42 DOM dumps re-proved free of every synthetic
secret marker by `capture()`): `artifacts/ui-review/raycast-after/01`-`07` at 1440/768/390,
`14-library-long-content__1440.png`, `15-settings__1040.png`, `15-settings-minimum__760.png`,
`16-settings-import-review__1040.png`, and the new
`17-library-minimum-window__760.png` - the smallest real window, one-pane navigation with the
action area on screen. Before-state comparison: the same filenames in
`artifacts/ui-review/raycast-baseline/`.

Suite on this state: `bun run check` 0/0, eslint clean, vitest 22/22 (includes the contrast gate),
Playwright 68/68 (the 57 baseline tests plus 3 from C4, 3 from C6, 4 new here and the new capture),
colour literals outside `tokens.css` still 0 hex / 0 `rgb*(`, `backdrop-filter` still 0, and
`before/`, `after/`, `native/`, `raycast-baseline/` byte-untouched; `src-tauri/` still untouched.

## Project fit - how every change follows the repo's established patterns (checkpoint C7, same date)

This section re-maps the conventions list to the concrete changes, the existing file or pattern each
one follows, and the command that proves it. Every count below was re-measured on this tree, not
carried over from earlier sections.

### (a) Every new visual value is a tokens.css token

- Count proof: hardcoded hex outside `tokens.css` is **0** (baseline 2), `rgb(`/`rgba(` (slash syntax
  included, per the C3 note) outside `tokens.css` is **0** (baseline 3), `backdrop-filter` is **0**
  (baseline 0). Repro: `grep -rEo '#[0-9a-fA-F]{3,8}\b' src | grep -v styles/tokens.css` and
  `grep -rnE '\brgba?\(' src | grep -v styles/tokens.css` (each prints nothing),
  `grep -rn backdrop-filter src | wc -l` prints 0.
- Per-rule token names: the three "as landed" sections above enumerate every changed selector with
  the exact token it now consumes (`--shell-bg`/`--surface-shell`, `--search-h`/`--search-focus-bg`,
  `--selected`/`--selected-edge`/`--selected-marker`/`--hover-fill`, `--border-hair`/`--border-shell`/
  `--border-warn`, `--tile-*`/`--row-h`/`--text-row*`/`--leading-tight`, `--label-tracking`,
  `--field-icon-indent`, `--inset-fill`, `--gap-*`).
- Enforcement proof (beyond grep): `tests/unit/design-tokens.test.ts` fails the unit run if any
  `.svelte`/`.ts` under `src/` reintroduces a colour literal and if the dark and light palette blocks
  drift apart by property name, and `scripts/contrast-check.mjs` fails if a colour token is added
  that appears in neither its audited pairing table nor its reasoned exemption list. "Values come
  from tokens.css" is now a gate, not a convention - exactly the header comment's own rule
  ("add a token instead of a literal") made executable.

### (b) Svelte 5 runes style matches Finder.svelte / Library.svelte

- Mechanical proof across the whole branch: **zero** diff hunks fall inside the `<script>` block of
  any of the 13 changed `.svelte` files (`git diff -U0 291a6a6..HEAD -- <file>` hunk start line vs
  the file's `</script>` line, per file - all 0). The runes code the baseline already had
  (`$state`/`$derived`/`$effect`: Finder 5, Library 14, SettingsPanel 11, EntryEditor 5, ...) is
  byte-identical, so the redesign cannot have introduced legacy store syntax; independently, no added
  line anywhere in `src/` contains `writable(`/`readable(`/`derived(`/`$:` (grep of the added lines
  reports 0). Every change is markup and `<style>` only, reading tokens through `var(--...)`.

### (c) New e2e follows the suite's helpers-and-marking conventions

- All new tests boot through the existing `boot(page, SEED, view)` / `mockState(page)` from
  `tests/e2e/helpers.ts` (redesign.spec.ts:4, screenshots.spec.ts:5); `helpers.ts` itself is
  untouched (`git diff --stat 291a6a6..HEAD -- tests/e2e/helpers.ts` empty), and no test hand-rolls
  its own state injection.
- Plaintext-marker absence is kept and extended: the baseline assertion
  `expect(body).not.toContain('synthetic-value-1')` in "secret rows stay masked ..." survives at
  redesign.spec.ts:47, C4 adds the locked-vault metadata absence and the masked-detail check at
  redesign.spec.ts:409, and `screenshots.spec.ts` `capture()` now asserts every seeded marker
  (`synthetic-masked-value`, `synthetic-db-password`, `synthetic-long-secret`) is absent from the
  serialized DOM of *every* capture (screenshots.spec.ts:171-175) before writing the PNG.
- Nothing was weakened to get there: assertion-call counts per spec base -> now are finder 40 -> 40,
  library 39 -> 39, settings 57 -> 57, redesign 51 -> 118, screenshots 0 -> 1. Total removed lines
  across all of `tests/` is 5, every one inside `screenshots.spec.ts` header comments and its
  `SHOT_SET` default (the change that stops a plain gate run from rewriting the frozen `after/`
  set); zero assertions were deleted from any baseline spec.

### (d) Existing data-testid / aria hooks preserved

- Machine check: for each of `data-testid`, `role`, `aria-selected`, `aria-activedescendant`,
  `aria-label`, `aria-hidden`, `aria-live`, `aria-expanded`, `aria-describedby`, `aria-controls`,
  every quoted attribute value present in `src/` at 291a6a6 is still present at HEAD
  (`comm -23` of the two sorted `git grep -ho` sets is empty for all ten attributes); the single
  dynamic binding (`data-testid={...}` in Tabs.svelte) is untouched. New hooks are additive only
  (e.g. `data-testid="shortcut-hint"` on the empty-state chip).
- Behavioural proof: all 57 baseline e2e tests pass unmodified against the redesigned DOM
  (full run on this state: `Total: 68 tests in 5 files`, 68 passed / 0 failed / 0 flaky /
  0 skipped - 57 baseline + 11 additions from C4/C6/C5).

### (e) src-tauri untouched

- `git diff --stat 291a6a6..HEAD -- src-tauri` is empty: the Rust backend, including `windows.rs`,
  is byte-identical to the commit at which `cargo fmt --check` and `cargo clippy -D warnings` were
  clean at baseline, so no native-side re-verification is required. Every material decision (opaque
  shell, `--material-blur: 0px`, no `backdrop-filter`) was made in CSS alone.

## Native material pass as landed (checkpoint C9)

Capture tooling added: `scripts/native-capture.sh` (+ `scripts/_native-capture-session.sh`,
`scripts/native/xsession-probe.c`, `scripts/native/seed-commands.mjs`). It launches the compiled
release binary inside a private dbus session + Xvfb (1280x800x24), paints the root window with
the `xsession-probe bg dark|busy` helper (xsetroot/xdotool/xwininfo are not installed here),
seeds four synthetic COMMANDS into the app's own freshly migrated database (command metadata
only - no secret is ever created, and the vault is never unlocked in this session), drives the
keyboard with XTEST through the same probe (`focus` + `type`), and measures the real screen with
`scrot` plus pixel-level `sample` / `picheck` / `diff` grabs.

### The material level actually achieved on this host: OPAQUE SHELL ON X11

- The shipped default is a carefully shaded OPAQUE shell and that is what the real binary paints:
  over a saturated red/green/cyan/magenta/orange mosaic (`busy`) the client areas of BOTH windows
  contain ZERO background-coloured pixels (`picheck` sums 0 for library and palette, in BOTH
  themes), all four window corners are app paint (square - there is no compositor here to round
  or blur anything), and the palette centre sampled 0.4s after mapping is already app paint
  (`#ffffff` light / `#15161a` dark), i.e. no blank or white flash frame on map.
- No glass is claimed anywhere. `backdrop-filter` occurrences in `src/` remain 0 and
  `--material-blur` ships at `0px`; `src-tauri/src/windows.rs` was NOT touched (it stays
  byte-identical to base - there is no transparency prototype in it to mark as unverified).
- True Wayland/Hyprland compositor translucency and blur are UNVERIFIED here: no Wayland
  session, no Hyprland, no compositor exists in this sandbox. That is an environment limit, not
  a result, and it is why the manual checklist below stays a written handoff step.

### Captures (real release binary, 1280x800 Xvfb; library window 1040x700 at 0,0; palette 680x420 at 300,190)

- `artifacts/ui-review/native-redesign/native-redesign-library__dark-xvfb.png` - light palette, dark X background
- `.../native-redesign-finder__dark-xvfb.png` - locked palette over the dark background (footer shows Up/Down select, Tab complete, Enter copy, Ctrl+O details, Esc dismiss)
- `.../native-redesign-finder-query__dark-xvfb.png` - palette after XTEST-typed `docker` (caret visible, results filtered, "Unlock vault to search secrets" row: no secret metadata while the vault is sealed)
- `.../native-redesign-library__busy-xvfb.png` + `.../native-redesign-finder__busy-xvfb.png` - opaque windows over the busy mosaic
- `.../native-redesign-library__darktheme-xvfb.png` + `.../native-redesign-finder__darktheme-xvfb.png` - `GTK_THEME=Adwaita:dark` over the dark background (WebKitGTK maps the GTK theme to prefers-color-scheme, so the same tokens render the dark palette unmodified)
- `.../native-redesign-finder__darktheme-busy-xvfb.png` - dark palette over the busy mosaic

No capture contains a plaintext secret: the native database holds only the four synthetic
command rows above, the vault was never unlocked, and the locked UI states are exactly what the
captures show (same masking rule the e2e DOM dumps assert).

### Two native findings worth recording

1. **"Native finder paints no results" (iteration 8 blocker) is a BUILD-RECIPE artifact, not a
   redesign regression and not a keyring issue.** A plain `cargo build --release` binary of a
   Tauri 2 app without the `custom-protocol` feature compiles in `cfg(dev)` mode and loads the
   `devUrl` instead of the embedded assets: every window shows WebKitGTK's "Could not connect to
   127.0.0.1: Connection refused" page, which is what earlier native probes were capturing. The
   only correct capture binary here is the one `scripts/verify.sh` itself builds -
   `bunx tauri build --no-bundle` with the four memory-tuning env vars (incremental: ~46s with a
   warm dependency cache). The committed pre-redesign `artifacts/ui-review/native/native-library__xvfb.png`
   proves the CLI-built binary renders the app. With the CLI build, search, typing, selection,
   lock state and dismissal all work in the real window.
2. **A phantom horizontal scrollbar in the library inspector, found by the native pass.**
   WebKitGTK counts the sticky action bar's negative inline margins (the C5 flush-edge recipe)
   as scrollable overflow that Blink ignores, and kept repainting a 3px overlay scrollbar
   (pixel-diff between two grabs 400ms apart: 2070 changed pixels at y=694..696 - a "changing at
   rest" frame). Fixed in `EntryDetails.svelte` styles only: the pane declares
   `overflow-x: hidden; overflow-y: auto`. After the fix the same probe reports `changed=0` at
   rest for both windows on both backgrounds, 68/68 Playwright tests still pass (the 760x520
   reachability test included) and Chromium-rendered captures are unaffected because Blink never
   showed the bar. This is why the browser e2e suite could not catch it: the bug only exists in
   the engine the product actually ships on.

### Manual desktop checklist (Hyprland + Wayland - the part this host cannot execute)

Run the installed `dev-console` (or `bunx tauri build --no-bundle` output) under a real Hyprland
session and record each item; any failure is a redesign bug, not an environment limit:

1. Both themes (`GTK_THEME` unset and `Adwaita:dark`): shell, rows, footer and dialogs read as
   designed; no half-light/half-dark flashes at startup.
2. Backgrounds: dark wallpaper, light wallpaper, a busy/animated wallpaper; the opaque shell
   never shows wallpaper through its body, its text stays legible, and no halo/shadow seam
   appears at window edges when the wallpaper changes under it.
3. 20 open/dismiss cycles (global shortcut or `dev-console --finder`), switching focus to
   another application between opens: the palette appears focused with an empty query every
   time, disappears on Escape/select/copy, and never steals focus back after it hides.
4. Hyprland `decoration:rounding` > 0: window corners round cleanly with no square corner peek
   and no gap between shell edge and rounded mask (the CSS radius is `--radius-shell` 14px; the
   compositor clips the actual window - they must look coherent, not fight).
5. Flash frames: no white/black first frame on any of the 20 opens (the X11 probe checks the
   centre pixel 0.4s after map; on Wayland observe with the naked eye and a screen recording).
6. Stale frames: after dismissing and reopening, the palette shows fresh results, never the
   previous query's rows; the library updates its vault pill without a manual refresh.
7. Focus loss: start a long-running operation in another app, open the palette, type, Enter to
   copy; the clipboard receives exactly one copy and the other app does not lose keyboard focus
   permanently.
8. Lag: typing a 40-character query feels immediate; scroll the library list while the palette
   is open - no dropped frames or scroll-wheel focus confusion.
9. Tray: with a StatusNotifierWatcher host running, the tray icon appears, the menu actions
   (open library / lock / quit) work, and the app keeps running when all windows are closed.
10. Lock/unlock with a real Secret Service provider: the vault pill transitions locked ->
    unlocked without repaint artifacts and secret metadata appears only after unlock.
11. If a future pass flips the opt-in `data-transparency='on'` prototype on: blur only behind
    the shell (never per row), and on any compositor without blur support the forced-opaque
    fallback must paint exactly the captures above. Until items 1-10 pass on a real desktop,
    "native glass" must not be claimed anywhere.
