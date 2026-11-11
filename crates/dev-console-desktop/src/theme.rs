//! The Dev Console palette as the native views paint it.
//!
//! Every colour here comes from `dev_console_core::theme`, the generated port of
//! `src/styles/tokens.css` and `src/styles/omarchy.css`, so the two shells cannot drift apart on a
//! value. [`Look`] resolves the tokens a view needs once per frame; `system` follows the desktop's
//! dark/light preference the way the stylesheet's `prefers-color-scheme` block does.

use dev_console_core::theme::Theme;
use rune::color::srgb_decode;
use rune::tok::{Family, TextStyle};
use rune::Color;

/// The setting value that follows the desktop rather than naming a palette.
pub const SYSTEM: &str = "system";

/// One palette's colours, resolved from its tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub dark: bool,
    pub bg: Color,
    pub panel: Color,
    pub raised: Color,
    pub sunken: Color,
    pub hover: Color,
    pub active: Color,
    pub line: Color,
    pub line_strong: Color,
    pub line_hair: Color,
    /// The edge of a floating surface: `.surface-shell { border: var(--border-shell) }`.
    pub line_shell: Color,
    /// The 1px inner top highlight the floating surfaces carry (`--shell-edge`).
    pub shell_edge: Color,
    pub fg: Color,
    pub muted: Color,
    pub dim: Color,
    pub shell: Color,
    pub accent: Color,
    pub accent_strong: Color,
    pub accent_fg: Color,
    pub accent_soft: Color,
    pub accent_line: Color,
    pub selected: Color,
    pub selected_fg: Color,
    pub selected_edge: Color,
    pub danger: Color,
    pub danger_soft: Color,
    pub ok: Color,
    pub ok_soft: Color,
    pub warn: Color,
    pub warn_soft: Color,
    pub kind_command: Color,
    pub kind_secret: Color,
    pub secret_soft: Color,
    pub focus: Color,
    pub backdrop: Color,
    pub placeholder: Color,
    pub inset: Color,
}

impl Look {
    /// The palette a saved `theme` setting names, with `system` resolved against `system_dark`.
    /// An unknown name (a palette removed in a later release) falls back to the system choice
    /// rather than failing to draw.
    pub fn for_setting(setting: &str, system_dark: bool) -> Look {
        let fallback = if system_dark { "dark" } else { "light" };
        let theme = if setting == SYSTEM {
            Theme::named(fallback)
        } else {
            Theme::named(setting).or_else(|_| Theme::named(fallback))
        }
        .expect("the dark and light palettes are always registered");
        Look::from_theme(theme)
    }

    pub fn from_theme(theme: Theme) -> Look {
        let c = |token: &str| colour(theme, token);
        Look {
            dark: theme.appearance().is_dark(),
            bg: c("--bg"),
            panel: c("--bg-panel"),
            raised: c("--bg-raised"),
            sunken: c("--bg-sunken"),
            hover: c("--bg-hover"),
            active: c("--bg-active"),
            line: c("--line"),
            line_strong: c("--line-strong"),
            line_hair: c("--line-hair"),
            line_shell: c("--line-shell"),
            shell_edge: c("--shell-edge"),
            fg: c("--fg"),
            muted: c("--fg-muted"),
            dim: c("--fg-dim"),
            shell: c("--shell-bg"),
            accent: c("--accent"),
            accent_strong: c("--accent-strong"),
            accent_fg: c("--accent-fg"),
            accent_soft: c("--accent-soft"),
            accent_line: c("--accent-line"),
            selected: c("--selected"),
            selected_fg: c("--selected-fg"),
            selected_edge: c("--selected-edge"),
            danger: c("--danger"),
            danger_soft: c("--danger-soft"),
            ok: c("--ok"),
            ok_soft: c("--ok-soft"),
            warn: c("--warn"),
            warn_soft: c("--warn-soft"),
            kind_command: c("--kind-command"),
            kind_secret: c("--kind-secret"),
            secret_soft: c("--secret-soft"),
            focus: c("--focus-ring"),
            backdrop: c("--backdrop"),
            placeholder: c("--search-placeholder"),
            inset: c("--inset-fill"),
        }
    }

    /// A distinct navigation surface, including palettes whose panel and page are identical.
    pub fn sidebar(&self) -> Color {
        if self.dark {
            self.sunken
        } else {
            self.bg
        }
    }

    /// The pane the selected entry is read in.
    pub fn content(&self) -> Color {
        if self.dark {
            self.bg
        } else {
            self.panel
        }
    }

    /// The rune-kit token set the kit's controls (text fields, switches, selects) read, seeded
    /// from this palette so they sit in it instead of in Rune's own default.
    pub fn kit(&self) -> rune_kit::Kit {
        let mut kit = rune_kit::Kit::from_palette_focus(
            rune_kit::PaletteSeed {
                bg: self.bg,
                fg: self.fg,
                accent: self.accent,
            },
            rune_kit::PaletteFocus::Custom(self.focus),
        );
        // A field is a well in the surface it sits on, with an edge that can be seen at rest.
        kit.c.bg_base = self.sunken;
        kit.c.border_base = self.line_strong;
        kit.radius.sm = 4.0;
        kit.radius.md = 6.0;
        kit.radius.lg = 10.0;
        kit
    }
}

/// A token's colour as Rune paints it: the stylesheet's sRGB value, decoded to linear light.
fn colour(theme: Theme, token: &str) -> Color {
    let rgba = theme
        .colour(token)
        .unwrap_or_else(|error| panic!("palette {} has no colour {token}: {error}", theme.name()));
    let channel = |value: f64| srgb_decode((value / 255.0) as f32);
    Color {
        r: channel(rgba.red),
        g: channel(rgba.green),
        b: channel(rgba.blue),
        a: rgba.alpha as f32,
    }
}

/// The palette names the Settings appearance picker offers, `system` first, each with the label
/// the web shows.
pub fn choices() -> Vec<(&'static str, String)> {
    let mut out = vec![(SYSTEM, "Follow system".to_string())];
    for name in dev_console_core::theme::palette_names() {
        out.push((name, label(name)));
    }
    out
}

/// A palette's display name, exactly as `OMARCHY_THEMES` in `src/lib/models.ts` pairs the slug with
/// its human name: no brand prefix, title-cased words (`omarchy-tokyo-night` → "Tokyo Night",
/// `omarchy-last-horizon` → "Last Horizon", `dark` → "Dark"), so both Settings pickers list the
/// same names for the same palettes.
pub fn label(name: &str) -> String {
    let title = |words: &str| {
        words
            .split('-')
            .map(|word| {
                let mut chars = word.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    title(name.strip_prefix("omarchy-").unwrap_or(name))
}

/// The type scale. Whole-point sizes only: the Library is mostly read on 1x displays, where a
/// half-point size lands glyph stems between pixels and the text looks soft.
pub mod text {
    use super::{Family, TextStyle};

    pub const XS: TextStyle = TextStyle::new(11.0, 400.0);
    pub const SM: TextStyle = TextStyle::new(12.0, 400.0);
    pub const BODY: TextStyle = TextStyle::new(13.0, 400.0);
    /// Button labels and the selected segment: one step up in weight, not in size.
    pub const MEDIUM: TextStyle = TextStyle::new(13.0, 500.0);
    pub const STRONG: TextStyle = TextStyle::new(13.0, 600.0);
    pub const LG: TextStyle = TextStyle::new(15.0, 600.0);
    /// The one large size: the name of what the detail pane is showing.
    pub const XL: TextStyle = TextStyle::new(20.0, 600.0).tracking(-0.2);
    pub const ROW: TextStyle = TextStyle::new(13.0, 500.0);
    pub const LIBRARY_ROW: TextStyle = TextStyle::new(14.0, 500.0);
    pub const DETAIL_TITLE: TextStyle = TextStyle::new(24.0, 600.0).tracking(-0.3);
    pub const SECTION: TextStyle = TextStyle::new(15.0, 600.0);
    pub const PROJECT: TextStyle = TextStyle::new(13.0, 600.0);
    pub const ROW_META: TextStyle = TextStyle::new(12.0, 400.0);
    pub const SEARCH: TextStyle = TextStyle::new(17.0, 400.0);
    /// Field and section labels: sentence case, quiet, never capitals.
    pub const LABEL: TextStyle = TextStyle::new(12.0, 500.0);
    pub const MONO: TextStyle = TextStyle::new(13.0, 400.0).family(Family::SystemMono);
    pub const MONO_SM: TextStyle = TextStyle::new(13.0, 400.0).family(Family::SystemMono);
}

/// Sizes, in points, on a 4-point grid.
pub mod size {
    pub const SP1: f32 = 4.0;
    pub const SP2: f32 = 8.0;
    pub const SP3: f32 = 12.0;
    pub const SP4: f32 = 16.0;
    pub const SP5: f32 = 24.0;
    pub const SP6: f32 = 32.0;
    pub const RADIUS_SM: f32 = 4.0;
    pub const RADIUS: f32 = 6.0;
    pub const RADIUS_LG: f32 = 10.0;
    pub const RADIUS_ROW: f32 = 6.0;
    /// `--radius-shell`: the arc every floating surface (dialog, Settings sheet, palette) wears.
    pub const RADIUS_SHELL: f32 = 14.0;
    pub const CONTROL_H: f32 = 28.0;
    pub const CONTROL_H_SM: f32 = 24.0;
    /// A Library list row: one line, dense enough to scan twenty without scrolling.
    pub const ROW_H: f32 = 34.0;
    /// A Finder result: two lines, name over context.
    pub const RESULT_H: f32 = 44.0;
    pub const SEARCH_H: f32 = 46.0;
    pub const FOOTER_H: f32 = 34.0;
    pub const TAB_H: f32 = 26.0;
    pub const TILE_SM: f32 = 22.0;
    pub const TILE_LG: f32 = 32.0;
    pub const PANE_LIST: f32 = 420.0;
    pub const PANE_LIST_MIN: f32 = 360.0;
    pub const PANE_LIST_MAX: f32 = 560.0;
    pub const LIBRARY_ROW_H: f32 = 52.0;
    /// The bar across the top of each pane.
    pub const BAR_H: f32 = 48.0;
    /// Reading measure of the detail pane: text does not run the width of a wide window.
    pub const MEASURE: f32 = 680.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_palette_resolves_every_token_the_views_paint() {
        for theme in Theme::all() {
            let look = Look::from_theme(theme);
            assert_eq!(look.dark, theme.appearance().is_dark(), "{}", theme.name());
            assert!(
                look.bg.a > 0.99,
                "{} has a translucent background",
                theme.name()
            );
        }
    }

    #[test]
    fn system_follows_the_desktop_and_unknown_names_fall_back_to_it() {
        assert!(Look::for_setting(SYSTEM, true).dark);
        assert!(!Look::for_setting(SYSTEM, false).dark);
        assert!(Look::for_setting("no-such-palette", true).dark);
        assert_eq!(
            Look::for_setting("omarchy-tokyo-night", false),
            Look::from_theme(Theme::named("omarchy-tokyo-night").unwrap())
        );
    }

    #[test]
    fn labels_read_as_the_settings_picker_shows_them() {
        assert_eq!(label("dark"), "Dark");
        assert_eq!(label("light"), "Light");
        assert_eq!(label("omarchy-tokyo-night"), "Tokyo Night");
        assert_eq!(label("omarchy-catppuccin-latte"), "Catppuccin Latte");
        assert_eq!(label("omarchy-last-horizon"), "Last Horizon");
        assert_eq!(label("omarchy-matte-black"), "Matte Black");
        assert_eq!(label("omarchy-rose-pine"), "Rose Pine");
        assert_eq!(label("omarchy-vantablack"), "Vantablack");
        for (_, text) in choices() {
            assert!(
                !text.contains("Omarchy"),
                "the picker shows palette names, not the brand prefix: {text}"
            );
        }
        let choices = choices();
        assert_eq!(choices[0].0, SYSTEM);
        assert_eq!(
            choices.len(),
            dev_console_core::theme::palette_names().len() + 1
        );
    }

    /// The revamp paints text on surfaces the shipped audit never measured: the two panes on a
    /// light palette swap places, and controls sit on translucent washes of `--fg` rather than
    /// on a token. Every pairing a view actually draws is measured here, in all 24 palettes,
    /// against the same 4.5:1 the audit holds text to.
    #[test]
    fn revamp_text_clears_contrast_on_every_surface_it_is_drawn_on() {
        use dev_console_core::theme::value::{contrast_ratio, Rgba};

        fn over(top: &Rgba, alpha: f64, under: &Rgba) -> Rgba {
            let mix = |a: f64, b: f64| a * alpha + b * (1.0 - alpha);
            Rgba {
                red: mix(top.red, under.red),
                green: mix(top.green, under.green),
                blue: mix(top.blue, under.blue),
                alpha: 1.0,
            }
        }

        let mut failures = Vec::new();
        for theme in Theme::all() {
            let colour = |token: &str| theme.colour(token).unwrap();
            let stack = |layers: &[&str]| theme.stack(layers).unwrap();
            let fg = colour("--fg");
            let panes = [("bg", colour("--bg")), ("panel", colour("--bg-panel"))];
            let mut surfaces: Vec<(String, Rgba)> = vec![
                ("sunken plate".into(), colour("--bg-sunken")),
                ("dialog".into(), colour("--shell-bg")),
            ];
            for (name, pane) in &panes {
                surfaces.push((format!("{name} pane"), *pane));
                surfaces.push((format!("hovered row on {name}"), over(&fg, 0.05, pane)));
                surfaces.push((format!("plain button on {name}"), over(&fg, 0.12, pane)));
                surfaces.push((
                    format!("selected row on {name}"),
                    stack(&[
                        "--selected",
                        &format!("--bg{}", if *name == "bg" { "" } else { "-panel" }),
                    ]),
                ));
            }
            surfaces.push((
                "plain button on dialog".into(),
                over(&fg, 0.12, &colour("--shell-bg")),
            ));
            for (surface, painted) in &surfaces {
                for token in ["--fg", "--fg-muted", "--fg-dim"] {
                    // The dimmest ink is for counts and footnotes on a pane or a plate; it is
                    // never put on a button or a selected row.
                    if token == "--fg-dim"
                        && (surface.contains("button") || surface.contains("selected"))
                    {
                        continue;
                    }
                    let ratio = contrast_ratio(&colour(token), painted);
                    if ratio < 4.5 {
                        failures.push(format!(
                            "{}: {token} on {surface} is {ratio:.2}:1",
                            theme.name()
                        ));
                    }
                }
            }
            let accent = colour("--accent");
            for (name, pane) in &panes {
                let ratio = contrast_ratio(&accent, pane);
                if ratio < 4.5 {
                    failures.push(format!("{}: link on {name} is {ratio:.2}:1", theme.name()));
                }
            }
            let ratio = contrast_ratio(&colour("--accent-fg"), &accent);
            if ratio < 4.5 {
                failures.push(format!(
                    "{}: filled button label is {ratio:.2}:1",
                    theme.name()
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
