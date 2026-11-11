//! Checkpoint C5: the generated theme data *is* the stylesheet, and every palette clears
//! the shipped contrast audit.

use super::contrast;
use super::inventory;
use super::shared::SHARED_ROWS;
use super::value::{
    contrast_ratio, expand_vars, parse_colour, parse_length, parse_number, LengthUnit,
};
use super::{Appearance, Length, Rgba, Theme, ThemeError, TokenKind};

/// The shape of the tables, so a row that silently disappears fails here rather than
/// quietly lowering the audit's coverage below.
const SHARED_TOKEN_COUNT: usize = 52;
const PALETTE_TOKEN_COUNT: usize = 49;
const PALETTE_COUNT: usize = 24;
const OMARCHY_PALETTE_COUNT: usize = 22;
const CHECK_GROUP_COUNT: usize = 5;
const CHECK_ROW_COUNT: usize = 76;
const EXEMPT_COUNT: usize = 27;

fn stylesheet() -> (String, String) {
    (
        inventory::read(inventory::tokens_path()),
        inventory::read(inventory::omarchy_path()),
    )
}

fn colour(red: f64, green: f64, blue: f64) -> Rgba {
    Rgba {
        red,
        green,
        blue,
        alpha: 1.0,
    }
}

#[test]
fn the_shared_token_table_is_the_tokens_css_root_block() {
    let (tokens, _) = stylesheet();
    let declared = inventory::shared_rows(&tokens);
    assert_eq!(
        declared.len(),
        SHARED_TOKEN_COUNT,
        "the shared `:root` block no longer declares {SHARED_TOKEN_COUNT} tokens; move the \
         tokens into the Rust table and update this test in the same change"
    );
    let generated: Vec<(String, String)> = SHARED_ROWS
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect();
    assert_eq!(
        generated, declared,
        "theme/shared.rs drifted from tokens.css"
    );
}

#[test]
fn the_palette_registry_is_every_stylesheet_palette_in_order() {
    let (tokens, omarchy) = stylesheet();
    let declared = inventory::palettes(&tokens, &omarchy);
    assert_eq!(
        declared.len(),
        PALETTE_COUNT,
        "expected dark, light and {OMARCHY_PALETTE_COUNT} Omarchy palettes"
    );
    let registered = super::palette_names();
    assert_eq!(registered.len(), declared.len(), "palette count drifted");
    let declared_names: Vec<&str> = declared.iter().map(|sheet| sheet.name.as_str()).collect();
    assert_eq!(
        registered, declared_names,
        "the registry order is the settings list order; it no longer matches the stylesheet"
    );
    assert_eq!(
        declared
            .iter()
            .filter(|sheet| sheet.name.starts_with("omarchy-"))
            .count(),
        OMARCHY_PALETTE_COUNT
    );
}

#[test]
fn every_generated_palette_matches_its_stylesheet_block_row_for_row() {
    let (tokens, omarchy) = stylesheet();
    for sheet in inventory::palettes(&tokens, &omarchy) {
        let theme = Theme::named(&sheet.name)
            .unwrap_or_else(|error| panic!("{} is not registered: {error}", sheet.name));
        assert_eq!(
            sheet.rows.len(),
            PALETTE_TOKEN_COUNT,
            "{} restates a different number of tokens",
            sheet.name
        );
        let generated: Vec<(String, String)> = theme
            .palette()
            .rows
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect();
        assert_eq!(
            generated, sheet.rows,
            "{} drifted from its stylesheet block",
            sheet.name
        );
        let expected = match sheet.scheme.as_str() {
            "dark" => Appearance::Dark,
            "light" => Appearance::Light,
            other => panic!("{} declares color-scheme {other}", sheet.name),
        };
        assert_eq!(theme.appearance(), expected, "{} appearance", sheet.name);
    }
}

#[test]
fn the_two_light_palettes_in_tokens_css_are_identical() {
    let (tokens, _) = stylesheet();
    let system = inventory::system_light(&tokens);
    let explicit = inventory::palettes(&tokens, "")
        .into_iter()
        .find(|sheet| sheet.name == "light")
        .expect("tokens.css declares a light palette")
        .rows;
    assert_eq!(
        system, explicit,
        "the prefers-color-scheme and [data-theme='light'] blocks drifted apart, and tokens.css \
         says the two light lists must stay identical"
    );
}

#[test]
fn every_palette_restates_the_same_token_names() {
    let (tokens, omarchy) = stylesheet();
    let sheets = inventory::palettes(&tokens, &omarchy);
    let reference: Vec<&str> = sheets[0]
        .rows
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(reference.len(), PALETTE_TOKEN_COUNT);
    for sheet in &sheets[1..] {
        let names: Vec<&str> = sheet.rows.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names, reference,
            "{} renames or reorders palette tokens",
            sheet.name
        );
    }
}

#[test]
fn every_declared_token_in_every_palette_maps_to_a_kind() {
    let mut mapped = 0usize;
    for theme in Theme::all() {
        for token in theme.tokens() {
            let kind = theme
                .kind(token)
                .unwrap_or_else(|error| panic!("{}: {error}", theme.name()));
            assert!(!format!("{kind:?}").is_empty());
            mapped += 1;
        }
    }
    assert_eq!(
        mapped,
        PALETTE_COUNT * (PALETTE_TOKEN_COUNT + SHARED_TOKEN_COUNT),
        "the completeness sweep no longer covers every declared token"
    );
}

#[test]
fn the_shared_block_carries_the_shapes_the_views_ask_for() {
    let theme = Theme::dark();
    let cases: [(&str, TokenKind); 8] = [
        ("--font-ui", TokenKind::FontStack),
        ("--font-mono", TokenKind::FontStack),
        ("--text", TokenKind::Length),
        ("--leading", TokenKind::Ratio),
        ("--leading-tight", TokenKind::Ratio),
        ("--label-tracking", TokenKind::Length),
        ("--border", TokenKind::Border),
        ("--transition", TokenKind::Transition),
    ];
    for (token, expected) in cases {
        assert_eq!(theme.kind(token), Ok(expected), "{token}");
    }
    assert_eq!(
        theme.length("--radius-circle"),
        Ok(Length {
            value: 50.0,
            unit: LengthUnit::Percent
        }),
        "a percentage radius has to keep its unit"
    );
    assert_eq!(
        theme.length("--label-tracking").map(|l| l.unit),
        Ok(LengthUnit::Em)
    );
    assert_eq!(theme.number("--leading"), Ok(1.45));
    assert_eq!(theme.length("--control-h").map(|l| l.value), Ok(30.0));
    assert_eq!(theme.resolve("--gap"), Ok("12px".to_string()));
    assert_eq!(theme.resolve("--gap-sm"), Ok("8px".to_string()));
    assert_eq!(
        theme.resolve("--border"),
        Ok("1px solid #2e323b".to_string())
    );
    assert_eq!(theme.resolve("--selected-marker"), {
        let marker = Theme::dark().resolve("--selected-marker").unwrap();
        assert_eq!(marker, "inset 2px 0 0 #7c8cf8");
        Ok(marker)
    });
    assert!(theme.raw("--no-such-token").is_none());
    assert!(matches!(
        theme.resolve("--no-such-token"),
        Err(ThemeError::Undeclared { .. })
    ));
}

#[test]
fn a_palette_block_is_forty_four_colours_four_shadows_and_a_blur() {
    for theme in Theme::all() {
        let (mut colour, mut shadow, mut length) = (0, 0, 0);
        for token in theme.palette().rows.iter().map(|(name, _)| *name) {
            match theme.kind(token).unwrap() {
                TokenKind::Colour => colour += 1,
                TokenKind::Shadow => shadow += 1,
                TokenKind::Length => length += 1,
                kind => panic!(
                    "{}: {kind:?} has no business in a palette block",
                    theme.name()
                ),
            }
        }
        assert_eq!((colour, shadow, length), (44, 4, 1), "{}", theme.name());
    }
}

#[test]
fn the_contrast_audit_clears_every_threshold_in_all_twenty_four_palettes() {
    let mut reports = String::new();
    let mut unclean = String::new();
    for theme in Theme::all() {
        let audit = contrast::audit(&theme);
        reports.push_str(&audit.summary());
        if !audit.is_clean() {
            unclean.push_str(&audit.summary());
            unclean.push_str(&format!(
                "failures {:?}\nuncovered {:?}\n",
                audit.failures, audit.uncovered
            ));
        }
        assert_eq!(audit.groups.len(), CHECK_GROUP_COUNT, "{}", theme.name());
        assert_eq!(audit.row_count(), CHECK_ROW_COUNT, "{}", theme.name());
        assert_eq!(audit.exemptions.len(), EXEMPT_COUNT, "{}", theme.name());
    }
    assert!(
        unclean.is_empty(),
        "the audit is not clean:\n{unclean}\n--- measured ---\n{reports}"
    );
}

#[test]
fn the_audit_measures_the_pairings_the_shipped_script_measures() {
    let audit = contrast::audit(&Theme::dark());
    let names: Vec<&str> = audit.groups.iter().map(|group| group.name).collect();
    assert_eq!(
        names,
        [
            "text on surfaces",
            "focus ring on every surface it can be drawn on",
            "selection perceivable without colour",
            "caret and field affordance",
            "kind and status tints that carry meaning",
        ]
    );
    assert_eq!(contrast::Kind::Text.threshold(), 4.5);
    assert_eq!(contrast::Kind::Boundary.threshold(), 3.0);
    let first = &audit.groups[0].measurements[0];
    assert_eq!(first.token, "--fg");
    assert_eq!(first.layers, &["--bg"][..]);
    assert_eq!(first.threshold(), 4.5, "a text row is gated at 4.5:1");
    assert!(first.ratio > 14.0, "--fg on --bg measured {}", first.ratio);
    assert!(audit.exemptions.iter().any(|row| row.token == "--backdrop"));
    assert!(audit
        .exemptions
        .iter()
        .all(|row| !row.reason.is_empty() && row.reference_ratio.is_some()));
    let boundary = audit.groups[1]
        .measurements
        .iter()
        .find(|row| row.token == "--focus-ring")
        .expect("the focus ring is measured");
    assert_eq!(boundary.threshold(), 3.0);
    assert!(boundary.passes());
}

#[test]
fn the_completeness_sweep_gates_both_measured_and_exempt_tokens() {
    let theme = Theme::dark();
    let audit = contrast::audit(&theme);
    assert!(
        audit.uncovered.is_empty(),
        "unaccounted colours: {:?}",
        audit.uncovered
    );
    let flat: Vec<&str> = theme
        .tokens()
        .filter(|token| {
            theme
                .kind(token)
                .ok()
                .is_some_and(TokenKind::is_flat_colour)
        })
        .collect();
    assert_eq!(
        flat.len(),
        44,
        "a palette block declares 44 flat colours and the shared block none"
    );

    let measured: Vec<&str> = audit
        .groups
        .iter()
        .flat_map(|group| &group.measurements)
        .flat_map(|row| std::iter::once(row.token).chain(row.layers.iter().copied()))
        .collect();
    let exempt: Vec<&str> = audit.exemptions.iter().map(|row| row.token).collect();
    let unaccounted: Vec<&&str> = flat
        .iter()
        .filter(|token| !measured.contains(token) && !exempt.contains(token))
        .collect();
    assert!(
        unaccounted.is_empty(),
        "{unaccounted:?} are neither measured by a row nor exempted with a reason"
    );

    let exempted = flat.iter().filter(|token| exempt.contains(token)).count();
    assert!(
        exempted > 0 && exempted < flat.len(),
        "the sweep needs both kinds of row: {exempted} of {} flat colours are exempt",
        flat.len()
    );
    for token in &exempt {
        assert!(
            theme.raw(token).is_some(),
            "{token} is exempt but no longer declared, which the audit reports as a failure"
        );
    }
    assert!(!audit
        .failures
        .iter()
        .any(|row| row.contains("no longer exists")));
}

#[test]
fn the_value_layer_parses_what_the_shipped_script_parses() {
    assert_eq!(parse_colour("#fff"), Some(colour(255.0, 255.0, 255.0)));
    assert_eq!(parse_colour("#f08a86"), Some(colour(240.0, 138.0, 134.0)));
    let half = parse_colour("#00000080").expect("8-digit hex");
    assert_eq!((half.red, half.green, half.blue), (0.0, 0.0, 0.0));
    assert!((half.alpha - 128.0 / 255.0).abs() < 1e-9);
    let spaced = parse_colour("rgb(0 0 0 / 0.5)").expect("space and slash syntax");
    assert!((spaced.alpha - 0.5).abs() < 1e-9);
    let percent = parse_colour("rgba(100%, 0%, 0%, 100%)").expect("percentage channels");
    assert_eq!(
        (percent.red, percent.green, percent.blue),
        (255.0, 0.0, 0.0)
    );
    assert!((percent.alpha - 1.0).abs() < 1e-9);
    assert!(parse_colour("inset 2px 0 0 #fff").is_none());
    assert!(parse_colour("0 24px 60px rgba(0, 0, 0, 0.55)").is_none());
    assert!(parse_colour("#12345").is_none());
    assert!(parse_colour("rgb(0 0)").is_none());
    let clear = parse_colour("transparent").expect("transparent is a colour");
    assert_eq!(
        (clear.red, clear.green, clear.blue, clear.alpha),
        (0.0, 0.0, 0.0, 0.0)
    );

    let white = colour(255.0, 255.0, 255.0);
    let black = colour(0.0, 0.0, 0.0);
    assert!((contrast_ratio(&white, &black) - 21.0).abs() < 1e-9);
    assert!((contrast_ratio(&black, &white) - 21.0).abs() < 1e-9);
    let replaced = colour(255.0, 0.0, 0.0).over(&black);
    assert_eq!(
        (replaced.red, replaced.green, replaced.blue, replaced.alpha),
        (255.0, 0.0, 0.0, 1.0),
        "an opaque layer replaces what is under it"
    );
    let blended = Rgba {
        red: 255.0,
        green: 0.0,
        blue: 0.0,
        alpha: 0.5,
    }
    .over(&black);
    assert!((blended.red - 127.5).abs() < 1e-9 && blended.alpha == 1.0);
    let theme = Theme::dark();
    let band = theme.stack(&["--selected", "--shell-bg"]).unwrap();
    let alone = theme.colour("--shell-bg").unwrap();
    assert!(
        band.luminance() > alone.luminance(),
        "the band lightens the shell"
    );
    assert!(theme.stack(&["--nope", "--bg"]).is_err());
}

#[test]
fn var_expansion_walks_a_chain_and_refuses_a_cycle() {
    let theme = Theme::dark();
    assert_eq!(theme.resolve("--selected-fg"), Ok("#e9eaef".to_string()));
    assert_eq!(theme.resolve("--footer-fg"), Ok("#959bac".to_string()));
    assert_eq!(
        theme.resolve("--shadow-dialog"),
        theme.resolve("--shadow-shell")
    );
    let table: &[(&str, &str)] = &[("--a", "var(--b)"), ("--b", "var(--c)"), ("--c", "#fff")];
    let lookup = |name: &str| table.iter().find(|(key, _)| *key == name).map(|(_, v)| *v);
    assert_eq!(expand_vars("var(--a)", lookup), Ok("#fff".to_string()));
    let cyclic: &[(&str, &str)] = &[("--a", "var(--a)")];
    let cycle = |name: &str| cyclic.iter().find(|(key, _)| *key == name).map(|(_, v)| *v);
    assert!(matches!(
        expand_vars("var(--a)", cycle),
        Err(ThemeError::VarCycle { .. })
    ));
    assert!(matches!(
        expand_vars("var(--missing)", |_| None::<&'static str>),
        Err(ThemeError::Undeclared { .. })
    ));
    assert_eq!(
        expand_vars("inset var(--edge-w) 0 0 var(--accent)", |name| match name {
            "--edge-w" => Some("2px"),
            "--accent" => Some("#7c8cf8"),
            _ => None,
        }),
        Ok("inset 2px 0 0 #7c8cf8".to_string())
    );
    assert_eq!(
        expand_vars("--plain, no vars", |_| None::<&'static str>),
        Ok("--plain, no vars".to_string())
    );
}

#[test]
fn a_length_keeps_its_unit_and_an_unknown_unit_is_an_error() {
    assert_eq!(
        parse_length("--radius-circle", "50%"),
        Ok(Length {
            value: 50.0,
            unit: LengthUnit::Percent
        })
    );
    assert_eq!(
        parse_length("--label-tracking", "0.06em").map(|l| l.unit),
        Ok(LengthUnit::Em)
    );
    assert!(parse_length("--sp-1", "4px").is_ok());
    assert!(matches!(
        parse_length("--pane-list", "20rem"),
        Err(ThemeError::NotALength { .. })
    ));
    assert!(matches!(
        parse_length("--gap", "calc(100% - 10px)"),
        Err(ThemeError::NotALength { .. })
    ));
    assert!(matches!(
        parse_number("--leading", "1.45em"),
        Err(ThemeError::NotANumber { .. })
    ));
    assert_eq!(parse_number("--leading", "1.45"), Ok(1.45));
}
