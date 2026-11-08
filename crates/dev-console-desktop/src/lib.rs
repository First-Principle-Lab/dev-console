//! The native Rune shell: two windows, a tray, one instance, and a control socket that wakes it.
//!
//! This crate is the desktop half of the plan in `docs/plans/rune-migration.md`. It owns windows,
//! tray, CLI, single-instance and wakeups. It deliberately owns **no** domain logic: models, the
//! SQLite repository, the age vault, the keyring adapter, polkit, search, imports, project scanners,
//! history, environment snapshots, recovery and backups all live in `dev-console-core` (plan §1), and
//! this shell reaches them only through that crate's types. The one place it has to hold one is
//! [`off_thread`], the stream that runs a blocking call on a Rune worker and has the answer judged by
//! `dev_console_core::freshness` before a window may paint it. The views in [`ui`] are still
//! placeholder trees with no profile behind them, so nothing here reads or writes a real user profile,
//! a database or a keyring, and it cannot disagree with the running Tauri app about user data.
//!
//! ## What this crate can prove on a headless host, and what it cannot
//!
//! Unit tests here cover the *specifications* the shell asks Rune for and the parts that need no
//! display: window titles and sizes, the opaque/borderless option set, the single-instance lock, the
//! control-socket round trip, the tray menu's shape and the icon buffer's layout. Rune's headless
//! tooling can also drive the views over their element trees (see `dev-console-ui-tests`).
//!
//! None of that establishes what plan §5 reserves for a real session: activation, the title/class
//! `hyprctl -j clients` reports for `dev-console-finder`, the mapped 680×420 size, hover, dismissal,
//! whether neighbouring tiled windows keep their layout, tray rendering, GPU/X11 error logs, and
//! "verified on Hyprland/NVIDIA". Those stay UNVERIFIED here - by the plan's own rule ("Treat exit 77
//! as UNVERIFIED"), which is what [`app::EXIT_NO_DISPLAY`] encodes for this binary - and are carried
//! to the owner's desktop check.

pub mod app;
pub mod copy;
pub mod dialog;
pub mod identity;
pub mod instance;
pub mod off_thread;
pub mod tray;
pub mod ui;

/// The one source walk shared by the gates that read this repository's own files, so a walk that
/// silently found nothing cannot make two of them pass at once.
#[cfg(test)]
mod source_scan;

/// The gate over `docs/project/native-feasibility-ledger.md`, which is what checkpoint C1's evidence
/// clause asks for: a named list of the plan §1 checks this container decides and the ones only the
/// owner's Hyprland/NVIDIA session can decide.
#[cfg(test)]
mod ledger_tests;

/// The gate over `docs/project/native-capability-matrix.md`, the artefact checkpoint C6's evidence
/// clause asks for: every shipped behaviour mapped to the native test that decides it or to an
/// explicit statement of the seam that is missing. The ledger above says what this host cannot
/// *decide*; this matrix says what the native shell has not *built*, one shipped behaviour at a time.
#[cfg(test)]
mod matrix_tests;

/// Application identity: the Wayland `app_id`, the StatusNotifierItem id, and the name the owner's
/// Hyprland rules and `.desktop` file use. The window *title* is separate, and for the Finder it is
/// `dev-console-finder` (see [`identity`]).
pub const APP_ID: &str = "dev-console";

/// Runs the shell. Returns the process exit code; `main` only forwards it.
///
/// Kept out of `main` so the integration tests in `dev-console-ui-tests` can call it without
/// replacing the real binary's process.
pub fn run(args: &[String]) -> i32 {
    app::run(args)
}

/// The command-line help text, exposed so a test can assert the accepted flags stay two.
pub const USAGE: &str = "usage: dev-console [--finder | --library]";

#[cfg(test)]
mod tests {
    use super::source_scan::rust_sources;

    /// AGENTS.md forbids hard-coded colours in view code, and checkpoint C5 asks for that rule
    /// to be proven by grep rather than asserted. This is that grep, kept as a test so it cannot
    /// rot: every colour a view paints has to come from a token set - `dev_console_core::theme`
    /// once milestone 4 maps the ported palette into `rune::tok`, and Rune's own token set until
    /// then - because a literal written into a view is a colour the 24 palettes cannot retune and
    /// the contrast audit in [`dev_console_core::theme`] cannot measure.
    ///
    /// The shape of the rule matches the one the CSS follows: a `#`-prefixed hex triplet, or an
    /// `rgb`/`rgba`/`hsl` call whose first argument is a number. Names ending in those words
    /// (`icon_rgba`) and Rune's `TrayIcon::rgba(width, height, bytes)` constructor, which takes a
    /// pixel buffer and not a colour, are deliberately outside it.
    #[test]
    fn no_view_source_hard_codes_a_colour() {
        let roots = [
            concat!(env!("CARGO_MANIFEST_DIR"), "/src"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../dev-console-ui-tests/src"),
        ];
        let mut scanned: Vec<String> = Vec::new();
        let mut offenders: Vec<String> = Vec::new();
        for root in roots {
            for path in rust_sources(root) {
                scanned.push(path.clone());
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
                for (number, line) in text.lines().enumerate() {
                    if let Some(column) = find_colour_literal(line) {
                        offenders.push(format!(
                            "{}:{}:{column}: {}",
                            path.rsplit_once("/src/")
                                .map_or(path.as_str(), |(_, tail)| tail),
                            number + 1,
                            line.trim()
                        ));
                    }
                }
            }
        }
        assert!(
            scanned.iter().any(|path| path.ends_with("/ui.rs")),
            "the scan stopped walking the view file: {scanned:?}"
        );
        assert!(
            scanned
                .iter()
                .any(|path| path.contains("dev-console-ui-tests")),
            "the scan stopped walking the scripted tests, which build the same element trees: \
             {scanned:?}"
        );
        assert!(
            scanned.len() >= 9,
            "the colour scan walked only {} sources; a view file moved or the roots are wrong",
            scanned.len()
        );
        assert!(
            offenders.is_empty(),
            "view code must take colours from a token set, not write one out:\n{}",
            offenders.join("\n")
        );
    }

    /// The first colour literal on a line, if it holds one.
    fn find_colour_literal(line: &str) -> Option<usize> {
        let bytes = line.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'#' {
                if hex_triplet(&bytes[index + 1..]).is_some() {
                    return Some(index);
                }
                continue;
            }
            if *byte != b'r' && *byte != b'h' {
                continue;
            }
            if index > 0 && is_ident_byte(bytes[index - 1]) {
                continue;
            }
            for name in ["rgb(", "rgba(", "hsl(", "hsla("] {
                if !line[index..].starts_with(name) {
                    continue;
                }
                let open = index + name.len() - 1;
                let first = line[open + 1..].chars().next();
                if first.is_some_and(|c| c.is_ascii_digit() || c == '#') {
                    return Some(index);
                }
            }
        }
        None
    }

    /// A `#`-prefixed hex triplet: 3, 4, 6 or 8 hex digits that run out before the next
    /// identifier byte. A markdown heading (`#` then a space) and an attribute (`#` then a
    /// bracket) do not match, which is what keeps this scan over the source honest.
    fn hex_triplet(bytes: &[u8]) -> Option<usize> {
        for length in [6usize, 3, 8, 4] {
            if bytes.len() >= length
                && bytes[..length].iter().all(u8::is_ascii_hexdigit)
                && !bytes.get(length).is_some_and(|byte| {
                    byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-'
                })
            {
                return Some(length);
            }
        }
        None
    }

    fn is_ident_byte(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
    }

    /// The colour literals the detector must catch, spelled out of parts: this file is one of
    /// the scanned sources, so spelling a complete colour out here in a comment would fail the
    /// gate on its own test fixture. The two builders below reassemble them at runtime instead,
    /// which is also what the gate's own message would point at if a colour did land in a view.
    fn hex_colour(digits: &str) -> String {
        format!(r##".fill("#{}")"##, digits)
    }

    fn function_colour(name: &str, channels: &str) -> String {
        format!("{}({})", name, channels)
    }

    /// The grep above is only a gate if it can actually see what it forbids, so the detector is
    /// tested against the shapes on both sides of the line: the colour literals a view must not
    /// write, and the near-misses that must not make it red. Without this the scan could pass by
    /// matching nothing at all, which is the failure mode a grep-as-test is most likely to have.
    #[test]
    fn the_colour_detector_sees_a_literal_and_ignores_a_token_name() {
        let forbidden = [
            hex_colour("1a2b3c"),
            hex_colour("ABCDEF"),
            hex_colour("fff"),
            hex_colour("0e1020ff"),
            function_colour("rgba", "255, 255, 255, 0.07"),
            function_colour("rgb", "0 0 0 / 0.5"),
            function_colour("hsl", "220, 50%, 60%"),
            function_colour("hsla", "220 50% 60% / 0.4"),
        ];
        for literal in &forbidden {
            assert!(
                find_colour_literal(literal).is_some(),
                "the detector missed a colour literal: {literal}"
            );
        }
        for allowed in [
            "use rune::tok::{radius, space, type_};",
            "pub fn icon_rgba(size: u32) -> Vec<u8> {",
            "TrayIcon::rgba(ICON_SIZE, ICON_SIZE, icon_rgba(ICON_SIZE))",
            "let pixels = icon_rgba(8);",
            "let hash = map.get(&key);",
            "#[derive(Debug, Clone, Copy)]",
            "if index > 0 && is_ident_byte(bytes[index - 1]) {",
            r#"assert!(path.extension().is_some_and(|ext| ext == "rs"));"#,
            "/// Every colour comes from `rune::tok::theme`.",
            ".color(theme.text_secondary)",
        ] {
            assert!(
                find_colour_literal(allowed).is_none(),
                "the detector flagged something that is not a colour literal: {allowed}"
            );
        }
    }

    #[test]
    fn the_app_id_is_the_tray_id() {
        assert_eq!(crate::APP_ID, crate::tray::TRAY_ID);
    }
}
