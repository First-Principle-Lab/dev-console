# Raycast inspired UI redesign: implementation brief

Status: implementation brief for `dev`. Scope is the existing Svelte 5 and Tauri 2 interface. Preserve command, secret, vault, clipboard, and window behavior. Start with the finder, then carry the visual language into the library and settings.

## Research and visual target

I reviewed Raycast's [May 2026 launcher image](https://www.raycast.com/uploads/blog-new-raycast/app.png), [launch article](https://www.raycast.com/blog/the-new-raycast), [v2 manual](https://manual.raycast.com/new-in-v2), and [technical deep dive](https://www.raycast.com/blog/a-technical-deep-dive-into-the-new-raycast). The launcher image is the primary reference. The [press kit](https://www.raycast.com/press) has additional dark and light images, but some depict the older UI; verify the version before treating one as a v2 target.

Visible in the 2026 launcher image:

- One floating, rounded, dark window over the desktop, with a fine light edge, broad soft shadow, and subtle variation in its material. Text remains legible over a busy background.
- A prominent search line with a large placeholder and generous space. There is no heavy boxed input inside the window.
- Muted group labels. Results have an icon, clear name, quieter secondary text, and a right aligned type or action. The selected row is a soft rounded band, not a bright outline.
- A compact bottom action area with a primary action and shortcut hints. It feels attached to the palette rather than like a separate toolbar.
- Restrained glass. Raycast says Liquid Glass supports the launcher, and its technical article explains that native window material and a transparent WebView work together. CSS blur by itself cannot reproduce the desktop effect.

These observations are not an official token sheet. Do not claim exact Raycast colors, blur radii, fonts, or spacing. The values below are starting points for Dev Console, to refine from screenshots at 100% scale. Do not copy Raycast logos, icons, branded red, imagery, or macOS controls into the product.

## Current app and intended outcomes

The current `artifacts/ui-review/after/13-finder-palette-680x420__680.png` shows a solid graphite window, small search line, bright indigo rule, tall two line results, outlined selected row, kind pills, and full width footer. Those treatments split attention across the palette. The current `01-library-commands__1440.png` shows a dense two pane library with many borders, chips, and action buttons. It needs calmer hierarchy while remaining useful for long commands and metadata.

Relevant files are `src/styles/tokens.css`, `src/windows/Finder.svelte`, `src/windows/Library.svelte`, shared components in `src/components/`, `src-tauri/src/windows.rs`, and `tests/e2e/screenshots.spec.ts`. The Rust builder makes the 680 x 420 finder borderless but does not request a transparent content area. The library starts at 1040 x 700 with a 760 x 520 minimum.

### Finder outcome

At 680 x 420, the search input must dominate the first glance. Give its line a generous hit area and a visible caret/focus state without a second box or the current bright rule. Keep vault status visible but quiet. Group pinned or recent entries on an empty query only if the current data supports it; do not change ranking for a screenshot.

Use one consistent result pattern: icon at left, name first, type or short context second, available action at right. Distinguish commands from secrets without a badge on every element. Use a soft selected row fill and subtle edge. Hover must not look identical to selection. Long names must truncate or wrap without losing the action. Keep footer labels aligned with actual behavior: Up/Down select, Tab completes, Enter copies, Ctrl+O opens details, Escape dismisses.

Style empty, no match, loading, copied, copy failure, and locked vault states with the same hierarchy. Locked mode must show no secret names or metadata. Never put secret values in previews, tooltips, screenshots, or errors. Preserve focus after pointer interaction and on reopen.

### Native material outcome

Treat native window, WebView/page background, and content panels as separate layers. `backdrop-filter` blurs pixels behind an element within the page. Seeing another desktop window through the finder also requires native window and WebView transparency plus compositor support. Prototype a small change in `src-tauri/src/windows.rs`, clear the page background only where needed, and inspect on the target Hyprland/Wayland session.

Capture the finder over dark, light, and busy desktop backgrounds. Check rounded corners, shadows, blur, focus, and text contrast. If desktop blur is unavailable, use a carefully shaded opaque shell. Do not fake glass with a blurred wallpaper image or blur every row. Reopen repeatedly to catch white flashes, black square corners, stale frames, and focus loss. Keep X11 and other platforms on a readable fallback. Any Hyprland configuration change belongs to a separate, explicit desktop step.

### Shared system outcome

Define colors, typography, spacing, radii, border opacity, elevation, and selection in `src/styles/tokens.css`, then apply them to shared components and both windows. Start with graphite/charcoal, neutral white text, muted gray secondary text, one restrained indigo action accent, low opacity dividers, and thin shell highlights. Tune light theme tokens separately. Try a 12 to 16 px shell radius and 6 to 10 px control/selection radii, then adjust from native captures. These are proposed Dev Console values, not measured Raycast values.

Use translucent material for the finder shell and perhaps floating menus or library chrome. Keep edit fields and long text on stable, readable surfaces. Avoid stacked glass cards, neon gradients, excessive pills, and bright borders everywhere. Keep visible focus rings, a solid fill when blur is unsupported, and reduced transparency and reduced motion fallbacks.

### Library outcome

At 1040 x 700, the library needs a quiet header, clear Commands/Secrets selection, readable list/detail split, and stable action area. Replace redundant borders and chips with a smaller number of meaningful separators. Make selection clear without making every tag equally prominent. Preserve monospace command text, working directory, notes, custom fields, and masked secret values. Reveal stays deliberate; lock and unlock remain clear.

The editor, delete dialog, empty library, settings, import review, and recovery controls must share the same tokens. At 760 x 520, footer and confirmation controls must be reachable by scrolling and keyboard. At narrow browser sizes, retain the existing one pane navigation pattern. Do not change form fields, save/cancel behavior, backup warnings, or native clipboard paths to fit the visual design.

## Implementation order and evidence

1. Add a dated research note under `docs/` with links to the exact Raycast images used and observations behind the chosen Dev Console design. Link to official images instead of committing their copyrighted files.
2. Capture untouched Dev Console screens under a new `artifacts/ui-review/raycast-baseline/` directory with synthetic data and masked secrets. Include native 680 x 420 finder and 1040 x 700 library images if the target desktop is available. Keep the existing `after` set until the baseline is secured.
3. Implement shared tokens and finder. Review at real size. Resolve native transparency and fallback before propagating the material to the library.
4. Implement library, editor, settings, dialogs, and alternate states. Inspect every regenerated screenshot difference. Include before/after image paths and reference links in the PR or handoff.
5. Run the tests below and report exact results, failures, and desktop checks still required. A browser screenshot cannot prove native glass.

## Test matrix

| Area | Procedure | Pass condition |
| --- | --- | --- |
| Finder visual states | Run `SHOT_SET=raycast-after bun run test:e2e -- screenshots`. Inspect captures `08` through `12` and the 680 x 420 `13` at 100% scale. | Search, selection, completion, copied, no matches, and locked states share one hierarchy; no clipping or leaked secret text. |
| Library visual states | Inspect captures `01` through `07`, `14`, `15`, and `16` at desktop, 768, 390, and real minimum window size. | Long content, forms, dialogs, settings, and action controls remain readable and reachable. |
| Keyboard and accessibility | Run `bun run test:e2e -- finder redesign library settings`. Manually test Up/Down, Tab, Enter, Ctrl+O, Escape, click then type, focus rings, and screen reader labels. | Existing interactions pass; focus returns to search; selection is perceivable without color alone. |
| Static and integration | Run `bun run check`, `bun run lint`, then `bash scripts/verify.sh`. | No new Svelte, lint, Rust, or integration failures; report environment skips. |
| Native material | Launch Tauri on the target desktop. Capture finder and library over dark, light, and detailed backgrounds in both themes. Open and dismiss finder at least 20 times, switching focus to another app between opens. | No square corners, flashes, stale frames, illegible text, focus loss, or severe lag. State whether the result is truly translucent, compositor blurred, or opaque fallback. |
| Contrast and motion | Check normal text at 4.5:1 and large text or meaningful control boundaries at 3:1 where WCAG AA applies. Test reduced motion and opaque fallback. | Search, rows, buttons, forms, errors, and selection remain usable in both themes. |
| Security regression | Run existing locked vault, masking, reveal, copy, and recovery tests; inspect screenshots for fixture secret values. | Lock removes metadata; reveal remains deliberate; no screenshot includes a plaintext secret. |

The screenshot suite uses `tests/e2e/ipc-mock.js`, so it tests layout and UI interactions, not native IPC or compositor material. Native checks require the Tauri window on the target machine. If that session is unavailable, mark native material unverified and leave a concrete manual check list in the handoff.

## Definition of done

A reviewer can open the reference links and before/after captures and see one consistent Raycast inspired palette and library. The finder stays fast and keyboard driven. Existing workflows pass. Dark, light, reduced motion, and opaque fallback states work. The handoff names the actual material achieved on Linux and includes test output and screenshot paths. Do not claim pixel identity with macOS Raycast: its native Liquid Glass material is platform specific.
