//! The tests that pin the ported contrast audit to the shipped Node script: every pairing in
//! its table, the measured minima it reports, and the dump a reviewer can diff against
//! `node scripts/contrast-check.mjs`. See `docs/project/development-guide.md` ("Visual review")
//! for why the stylesheets stay the source of truth and these tables are the leash.

use super::contrast;
use crate::theme::Theme;

/// The audit table itself, transcribed from `scripts/contrast-check.mjs` by
/// `scripts/generate-theme-data.py`: group, token, layer stack (topmost first) and the promise
/// kind for all 76 pairs the shipped script measures, then the 27 colour tokens it exempts.
///
/// This is the half of the fidelity promise the pinned numbers below cannot make. Those prove the
/// Rust *arithmetic* matches the Node script's; this proves the Rust table has the same rows,
/// painted on the same surfaces, gated at the same level - so a pairing cannot quietly disappear
/// from the ported audit and leave 75 green rows behind. A retune of the tokens must not change
/// anything here; only editing the script's table may.
#[test]
fn the_rust_audit_measures_the_shipped_scripts_pairings() {
    /// `(group, token, layer stack topmost first, promise kind)`, in the script's order.
    const SHIPPED_PAIRINGS: [(&str, &str, &str, &str); 76] = [
        ("text on surfaces", "--fg", "--bg", "text"),
        ("text on surfaces", "--fg", "--shell-bg", "text"),
        ("text on surfaces", "--fg", "--bg-panel", "text"),
        ("text on surfaces", "--fg", "--bg-raised", "text"),
        ("text on surfaces", "--fg", "--bg-sunken", "text"),
        (
            "text on surfaces",
            "--fg",
            "--inset-fill + --bg-panel",
            "text",
        ),
        ("text on surfaces", "--fg", "--bg-hover", "text"),
        ("text on surfaces", "--fg", "--bg-active", "text"),
        (
            "text on surfaces",
            "--fg",
            "--selected + --shell-bg",
            "text",
        ),
        (
            "text on surfaces",
            "--fg",
            "--selected + --bg-panel",
            "text",
        ),
        (
            "text on surfaces",
            "--selected-fg",
            "--selected + --shell-bg",
            "text",
        ),
        (
            "text on surfaces",
            "--selected-fg",
            "--selected + --bg-panel",
            "text",
        ),
        ("text on surfaces", "--fg-muted", "--bg", "text"),
        ("text on surfaces", "--fg-muted", "--shell-bg", "text"),
        ("text on surfaces", "--fg-muted", "--bg-panel", "text"),
        ("text on surfaces", "--fg-muted", "--bg-raised", "text"),
        (
            "text on surfaces",
            "--fg-muted",
            "--hover-fill + --shell-bg",
            "text",
        ),
        (
            "text on surfaces",
            "--fg-muted",
            "--selected + --shell-bg",
            "text",
        ),
        (
            "text on surfaces",
            "--fg-muted",
            "--selected + --bg-panel",
            "text",
        ),
        (
            "text on surfaces",
            "--fg-muted",
            "--warn-soft + --bg-panel",
            "text",
        ),
        ("text on surfaces", "--fg-dim", "--shell-bg", "text"),
        ("text on surfaces", "--fg-dim", "--bg-raised", "text"),
        ("text on surfaces", "--fg-dim", "--bg-sunken", "text"),
        (
            "text on surfaces",
            "--fg-dim",
            "--selected + --shell-bg",
            "text",
        ),
        (
            "text on surfaces",
            "--fg-dim",
            "--selected + --bg-panel",
            "text",
        ),
        ("text on surfaces", "--footer-fg", "--shell-bg", "text"),
        ("text on surfaces", "--search-fg", "--shell-bg", "text"),
        (
            "text on surfaces",
            "--search-fg",
            "--search-focus-bg + --shell-bg",
            "text",
        ),
        (
            "text on surfaces",
            "--search-placeholder",
            "--shell-bg",
            "text",
        ),
        (
            "text on surfaces",
            "--search-placeholder",
            "--search-focus-bg + --shell-bg",
            "text",
        ),
        ("text on surfaces", "--accent", "--bg", "text"),
        ("text on surfaces", "--accent", "--bg-panel", "text"),
        (
            "text on surfaces",
            "--fg",
            "--accent-soft + --selected + --bg-panel",
            "text",
        ),
        ("text on surfaces", "--accent-fg", "--accent", "text"),
        ("text on surfaces", "--accent-fg", "--accent-strong", "text"),
        ("text on surfaces", "--danger", "--bg-panel", "text"),
        (
            "text on surfaces",
            "--danger",
            "--danger-soft + --bg-panel",
            "text",
        ),
        (
            "text on surfaces",
            "--danger",
            "--danger-soft + --shell-bg",
            "text",
        ),
        ("text on surfaces", "--ok", "--bg-panel", "text"),
        ("text on surfaces", "--ok", "--ok-soft + --bg-panel", "text"),
        ("text on surfaces", "--ok", "--ok-soft + --shell-bg", "text"),
        ("text on surfaces", "--ok", "--shell-bg", "text"),
        ("text on surfaces", "--warn", "--bg-panel", "text"),
        (
            "text on surfaces",
            "--warn",
            "--warn-soft + --shell-bg",
            "text",
        ),
        (
            "text on surfaces",
            "--warn",
            "--selected + --shell-bg",
            "text",
        ),
        ("text on surfaces", "--warn", "--shell-bg", "text"),
        (
            "focus ring on every surface it can be drawn on",
            "--focus-ring",
            "--bg",
            "boundary",
        ),
        (
            "focus ring on every surface it can be drawn on",
            "--focus-ring",
            "--shell-bg",
            "boundary",
        ),
        (
            "focus ring on every surface it can be drawn on",
            "--focus-ring",
            "--bg-panel",
            "boundary",
        ),
        (
            "focus ring on every surface it can be drawn on",
            "--focus-ring",
            "--bg-raised",
            "boundary",
        ),
        (
            "focus ring on every surface it can be drawn on",
            "--focus-ring",
            "--bg-sunken",
            "boundary",
        ),
        (
            "focus ring on every surface it can be drawn on",
            "--focus-ring",
            "--selected + --shell-bg",
            "boundary",
        ),
        (
            "focus ring on every surface it can be drawn on",
            "--focus-ring",
            "--hover-fill + --shell-bg",
            "boundary",
        ),
        (
            "focus ring on every surface it can be drawn on",
            "--focus-ring",
            "--inset-fill + --bg-panel",
            "boundary",
        ),
        (
            "selection perceivable without colour",
            "--selected-marker",
            "--selected + --shell-bg",
            "boundary",
        ),
        (
            "selection perceivable without colour",
            "--selected-marker",
            "--shell-bg",
            "boundary",
        ),
        (
            "selection perceivable without colour",
            "--selected-marker",
            "--selected + --bg-panel",
            "boundary",
        ),
        (
            "selection perceivable without colour",
            "--selected-marker",
            "--bg-panel",
            "boundary",
        ),
        (
            "selection perceivable without colour",
            "--selected-marker",
            "--bg-raised",
            "boundary",
        ),
        (
            "caret and field affordance",
            "--caret",
            "--shell-bg",
            "boundary",
        ),
        (
            "caret and field affordance",
            "--caret",
            "--search-focus-bg + --shell-bg",
            "boundary",
        ),
        (
            "caret and field affordance",
            "--caret",
            "--bg-sunken",
            "boundary",
        ),
        (
            "caret and field affordance",
            "--fg-muted",
            "--bg-sunken",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--kind-command",
            "--accent-soft + --shell-bg",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--kind-command",
            "--accent-soft + --selected + --shell-bg",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--kind-command",
            "--accent-soft + --bg-panel",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--accent",
            "--accent-soft + --bg-panel",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--accent",
            "--selected + --bg-panel",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--accent",
            "--bg-panel",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--kind-secret",
            "--secret-soft + --shell-bg",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--kind-secret",
            "--secret-soft + --selected + --shell-bg",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--kind-secret",
            "--secret-soft + --bg-panel",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--kind-secret",
            "--secret-soft + --selected + --bg-panel",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--danger",
            "--danger-soft + --bg-panel",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--warn",
            "--bg-panel",
            "boundary",
        ),
        (
            "kind and status tints that carry meaning",
            "--accent",
            "--bg-sunken",
            "boundary",
        ),
    ];
    /// Colour tokens the script exempts from measurement, each with a reason in its table.
    const SHIPPED_EXEMPTIONS: [&str; 27] = [
        "--bg",
        "--bg-panel",
        "--bg-raised",
        "--bg-sunken",
        "--bg-hover",
        "--bg-active",
        "--shell-bg",
        "--line",
        "--line-strong",
        "--line-shell",
        "--line-hair",
        "--shell-edge",
        "--selected-edge",
        "--hover-fill",
        "--inset-fill",
        "--search-focus-bg",
        "--accent-soft",
        "--command-accent",
        "--secret-accent",
        "--accent-line",
        "--selected",
        "--danger-soft",
        "--ok-soft",
        "--warn-soft",
        "--secret-soft",
        "--backdrop",
        "--material-tint",
    ];

    let groups: Vec<&str> = super::checks::CHECK_GROUPS
        .iter()
        .map(|group| group.name)
        .collect();
    assert_eq!(
        groups,
        [
            "text on surfaces",
            "focus ring on every surface it can be drawn on",
            "selection perceivable without colour",
            "caret and field affordance",
            "kind and status tints that carry meaning",
        ],
        "the ported audit no longer measures the shipped script's groups, in its order"
    );

    let mut index = 0usize;
    let mut notes: Vec<&str> = Vec::new();
    for group in super::checks::CHECK_GROUPS {
        for row in group.rows {
            let (want_group, want_token, want_layers, want_kind) = SHIPPED_PAIRINGS[index];
            index += 1;
            assert_eq!(
                want_group, group.name,
                "row {index} belongs to another group"
            );
            assert_eq!(
                want_token, row.token,
                "row {index}: the shipped script measures {want_token}, the Rust table measures {}",
                row.token
            );
            assert_eq!(
                want_layers,
                row.layers.join(" + "),
                "row {index} ({}) is painted on a different stack",
                row.token
            );
            assert_eq!(
                want_kind,
                match row.kind {
                    super::contrast::Kind::Text => "text",
                    super::contrast::Kind::Boundary => "boundary",
                },
                "row {index} ({}) is gated as a different promise",
                row.token
            );
            assert!(!row.note.trim().is_empty(), "row {index} names no rule");
            notes.push(row.note);
        }
    }
    assert_eq!(index, 76, "the ported audit no longer has 76 pairs");
    notes.sort_unstable();
    let duplicates = notes.len() - notes.iter().collect::<std::collections::HashSet<_>>().len();
    assert_eq!(duplicates, 0, "a pairing lost the rule it came from");

    assert_eq!(super::checks::THRESHOLD_TEXT, 4.5);
    assert_eq!(super::checks::THRESHOLD_BOUNDARY, 3.0);
    let exempt: Vec<&str> = super::checks::EXEMPT
        .iter()
        .map(|(token, _)| *token)
        .collect();
    assert_eq!(
        exempt, SHIPPED_EXEMPTIONS,
        "the exempt list is no longer the shipped script's, in its order"
    );
    for (_, reason) in super::checks::EXEMPT {
        assert!(
            reason.split_whitespace().count() > 1,
            "an exemption is exempted without a reason: {reason}"
        );
    }
}

#[test]
#[ignore]
fn dump_for_the_node_cross_check() {
    let mut out = String::new();
    for theme in Theme::all() {
        let audit = contrast::audit(&theme);
        for group in &audit.groups {
            for row in &group.measurements {
                out.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{:.2}\n",
                    theme.name(),
                    group.name,
                    row.token,
                    row.layers.join(" + "),
                    row.ratio
                ));
            }
        }
    }
    std::fs::write("/tmp/rust-audit.tsv", out).expect("write");
}

/// The cross-check that makes this a *port* of `scripts/contrast-check.mjs` and not a
/// similar-looking audit: the worst measured pairing in every group, in every palette, at
/// two decimals, copied from the Node script's own output. `cargo test theme -- --ignored
/// dump_for_the_node_cross_check` rewrites the Rust side of the table to `/tmp` for
/// diffing; `node scripts/contrast-check.mjs` prints the shipped one. If a deliberate token
/// retune moves these numbers, it must move both audits by the same amount - that is the
/// promise being locked here, not these particular ratios.
#[test]
fn the_measured_minima_are_the_ones_the_node_audit_reports() {
    const GROUPS: [&str; 5] = [
        "text on surfaces",
        "focus ring on every surface it can be drawn on",
        "selection perceivable without colour",
        "caret and field affordance",
        "kind and status tints that carry meaning",
    ];
    /// Per palette, in registry order: the worst pairing in each group, two decimals.
    const MINIMA: [(&str, [&str; 5]); 24] = [
        ("dark", ["4.71", "6.83", "4.33", "5.02", "3.45"]),
        ("light", ["4.57", "8.00", "4.80", "4.99", "4.01"]),
        (
            "omarchy-catppuccin",
            ["6.02", "5.95", "5.95", "7.59", "4.98"],
        ),
        (
            "omarchy-catppuccin-latte",
            ["4.95", "4.95", "4.95", "5.63", "4.38"],
        ),
        ("omarchy-ethereal", ["5.56", "5.60", "5.60", "6.55", "4.86"]),
        (
            "omarchy-everforest",
            ["4.53", "4.53", "4.53", "5.74", "3.87"],
        ),
        (
            "omarchy-flexoki-light",
            ["5.58", "5.63", "5.63", "6.45", "4.96"],
        ),
        ("omarchy-gruvbox", ["5.15", "5.47", "5.47", "7.08", "4.59"]),
        (
            "omarchy-hackerman",
            ["4.62", "10.46", "10.46", "11.33", "7.79"],
        ),
        ("omarchy-kanagawa", ["4.61", "7.51", "7.51", "9.69", "3.99"]),
        (
            "omarchy-last-horizon",
            ["7.77", "8.41", "8.41", "10.55", "6.88"],
        ),
        ("omarchy-lumon", ["4.62", "6.15", "6.15", "7.98", "5.10"]),
        ("omarchy-lupine", ["5.29", "5.38", "5.38", "6.09", "4.66"]),
        (
            "omarchy-matte-black",
            ["4.72", "5.77", "5.77", "6.26", "4.17"],
        ),
        ("omarchy-miasma", ["4.76", "4.71", "4.71", "5.75", "4.06"]),
        ("omarchy-nord", ["4.76", "4.75", "4.75", "6.03", "4.04"]),
        (
            "omarchy-osaka-jade",
            ["5.53", "5.95", "5.95", "7.60", "5.00"],
        ),
        ("omarchy-retro-82", ["6.68", "6.95", "6.95", "8.60", "5.81"]),
        (
            "omarchy-ristretto",
            ["4.91", "4.86", "4.86", "5.95", "4.17"],
        ),
        (
            "omarchy-rose-pine",
            ["4.68", "4.72", "4.72", "5.21", "4.22"],
        ),
        ("omarchy-solitude", ["6.01", "5.96", "5.96", "7.26", "5.07"]),
        (
            "omarchy-tokyo-night",
            ["4.67", "5.15", "5.15", "6.34", "4.42"],
        ),
        (
            "omarchy-vantablack",
            ["4.65", "5.24", "5.24", "5.96", "4.84"],
        ),
        ("omarchy-white", ["4.65", "4.65", "4.65", "7.74", "5.76"]),
    ];

    let registered = super::palette_names();
    let rows: Vec<(&str, [&str; 5])> = MINIMA
        .iter()
        .map(|(name, values)| (*name, *values))
        .collect();
    assert_eq!(
        registered,
        rows.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        "the pinned table is keyed by palette in registry order"
    );
    for (name, expected) in &rows {
        let theme = Theme::named(name).expect("a pinned palette");
        let audit = contrast::audit(&theme);
        let groups: Vec<&str> = audit.groups.iter().map(|group| group.name).collect();
        assert_eq!(groups, GROUPS, "{} lost a check group", name);
        for (group, expected) in audit.groups.iter().zip(expected.iter().copied()) {
            let worst = group.worst().expect("a measured pair in every group");
            assert_eq!(
                format!("{:.2}", worst.ratio),
                expected,
                "{} / {}: the Rust audit now measures a different worst pairing than the \
                 shipped Node script measures; re-run `node scripts/contrast-check.mjs` and \
                 fix whichever port drifted",
                name,
                group.name
            );
            assert!(
                worst.passes(),
                "{} / {} fell below {}: {}",
                name,
                group.name,
                group.kind.threshold(),
                worst.note
            );
        }
    }
}
