# Raycast inspired UI redesign brief

Status: ready for implementation on `dev`. This is a visual and interaction polish pass for the existing Svelte 5 and Tauri 2 app. Keep the command, secret, vault, and clipboard behavior specified in `PLAN.md` and implemented today.

## Design read

Treat Dev Console as a compact desktop utility for people who open the finder many times a day. Aim for the density, hierarchy, and keyboard clarity of Raycast's 2026 macOS UI, with restrained translucent material and crisp text. Use Raycast as a visual reference, not as a source of logos, icons, artwork, or code. On Linux, CSS and window compositor effects can approximate glass; they cannot reproduce Apple's native Liquid Glass material exactly.

Reference material to inspect and capture before coding:

- [The New Raycast](https://www.raycast.com/blog/the-new-raycast), especially its launcher screenshot and explanation of the new visual direction.
- [Raycast press kit](https://www.raycast.com/press), for official product screenshots at their original sizes.
- [Raycast's technical deep dive](https://www.raycast.com/blog/a-technical-deep-dive-into-the-new-raycast), for its distinction between native window material and WebView content.
- Existing Dev Console screenshots under `artifacts/ui-review/after/`, especially `13-finder-palette-680x420__680.png`, `01-library-commands__1440.png`, `02-library-secrets-masked__1440.png`, and `15-settings__1040.png`.

Save dated reference screenshots and a short observation sheet in `artifacts/ui-review/raycast-reference/`. Record the source URL, capture date, window size, and the specific visual decisions each image supports. Do not check in copyrighted Raycast screenshots unless redistribution is permitted; link to their source in the observation sheet instead. The agent should also capture the current app in its native window before changing styles. Browser screenshots alone cannot prove desktop transparency.

## Current UI audit

`src/styles/tokens.css` defines a compact dark graphite palette, indigo selection, small radii, and solid backgrounds. `src/windows/Finder.svelte` is a 680 x 420 borderless palette; its current screenshot shows a search header, roomy result rows, kind badges, and a keyboard footer. `src/windows/Library.svelte` contains the larger command and secret library. `src/components/SettingsPanel.svelte` has dense forms and import review. `src-tauri/src/windows.rs` creates the native windows; the finder currently has no declared transparent window background. The existing Playwright screenshot suite in `tests/e2e/screenshots.spec.ts` covers the major states.

The redesign should bring these screens into one coherent visual system. The finder is the visual anchor. Keep the library and settings practical for long text and form work; do not put translucent panels behind every field.

## Implementation sequence

1. **Capture and specify.** Compare current native and Playwright screenshots to the Raycast references. Create a compact design spec in this document or a linked file: dark and light palettes, type sizes, spacing, radii, border opacity, panel material, elevation, selected row, focus ring, and icon treatment. Sketch the finder, library, secret details, editor, and settings at their real window sizes. Note what can only be approximated on Linux. Get the chosen reference screenshots and measurements into the PR description.
2. **Build shared material tokens.** Update `src/styles/tokens.css` and shared components first. Use a restrained translucent outer shell, subtle edge highlights, low contrast separators, and clear foreground layers. Apply blur selectively to the shell or floating menus, while list rows and input text stay readable. Prefer existing Svelte and CSS code; do not add a React or Tailwind stack. Add a solid, high contrast fallback when backdrop filtering, compositor blur, or reduced transparency is unavailable. Support both dark and light themes.
3. **Redesign the finder.** Make the search field the dominant element. Tune row height, selection treatment, glyph size, metadata, keyboard hints, footer actions, empty state, locked vault state, no matches, loading, and errors. Preserve focus on the search input after pointer interaction, Escape dismissal, arrow navigation, Tab completion, Enter copy, and the explicit Run and Details actions. Never display secret values or locked secret metadata in screenshots or previews.
4. **Carry the system into the library.** Redesign the list, details, edit form, tabs, vault state, dialogs, and settings. Keep scanability for long commands, long paths, tags, and multiline notes. Preserve the current 1040 x 700 library size and usable 760 x 520 minimum, as well as the 680 x 420 finder. Adapt narrower browser review sizes without clipping or inaccessible actions.
5. **Verify native glass behavior.** Check whether Tauri's window transparency and the target Hyprland/Wayland compositor actually support the intended effect. If so, make the finder window transparent and test rounded corners, shadows, blur, focus, performance, and text legibility over light, dark, and busy desktop backgrounds. If the compositor cannot supply real background blur, ship a polished opaque or translucent fallback and document the difference. Avoid platform specific changes that break X11 or the library window.
6. **Review screenshots and finish.** Regenerate the existing screenshot suite with `SHOT_SET=after bun run test:e2e -- screenshots` and retain a separately named baseline before overwriting the current `after` set. Add native finder and library captures on the target desktop, plus a contact sheet or concise before/after comparison. Inspect at 100% scale, fix any clipped text, weak contrast, inconsistent radii, noisy glass, or broken focus states. Run `bun run check`, `bun run lint`, relevant Playwright interactions, and `bash scripts/verify.sh`; report environment skips honestly.

## Acceptance criteria

- A reviewer can compare the committed before and after Dev Console captures against linked Raycast references for the finder, library, settings, and locked vault.
- The finder reads as one compact palette with a clear search field, selection, results, and action footer. Glass treatment supports the hierarchy without obscuring names or keyboard hints.
- Dark and light themes remain readable. Text, controls, error messages, focus indicators, and selected rows meet WCAG AA contrast where applicable. Reduced motion and reduced transparency settings have usable fallbacks.
- Existing command and secret flows, keyboard behavior, window lifecycle, and native IPC remain intact. Synthetic fixture secrets stay masked in every screenshot.
- The native desktop captures show the actual compositor result. The handoff states which effects are true background transparency and which are CSS approximations.

Keep this work on `dev` until the visual and native checks pass. Do not broaden the task into new product features or security changes.
