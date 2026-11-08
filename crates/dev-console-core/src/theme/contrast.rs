//! The contrast audit, ported from `scripts/contrast-check.mjs` (checkpoint C5).
//!
//! The promise being kept is the one in `docs/design/raycast-ui-redesign.md`: normal text
//! clears 4.5:1 and a meaningful control boundary clears 3.0:1, in every palette, and the
//! shipped `bun run test` gate runs the Node script that measures it. This module runs the
//! same table against the same palette data from Rust, so the native app can be held to
//! the promise without a Node runtime - and so a token retune fails in both languages.
//!
//! The three things that make the port trustworthy rather than merely plausible:
//!
//! - The table is generated from the script itself ([`crate::theme::checks`]), so the
//!   pairings, the `where` notes and the exemptions cannot drift from it by hand.
//! - [`Audit`] reports the measured ratio of every row, not just a pass bit, so the worst
//!   pairing per group is visible in the test output the way it is in the script's.
//! - The completeness sweep is the script's: every token whose whole value is one flat
//!   colour must be measured by a row or exempted with a reason, per palette.

use std::fmt::Write as _;

use crate::theme::checks::{CHECK_GROUPS, EXEMPT, THRESHOLD_BOUNDARY, THRESHOLD_TEXT};
use crate::theme::value::{ThemeError, TokenKind};
use crate::theme::Theme;

/// Which promise a row makes. Text is the stricter one; a boundary is WCAG 1.4.11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Boundary,
}

impl Kind {
    /// The thresholds the shipped audit measures against.
    pub fn threshold(self) -> f64 {
        match self {
            Self::Text => THRESHOLD_TEXT,
            Self::Boundary => THRESHOLD_BOUNDARY,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Boundary => "boundary",
        }
    }
}

/// One measured pairing: `--fg` painted over a stack of layers, topmost first.
#[derive(Debug, Clone, Copy)]
pub struct CheckRow {
    pub token: &'static str,
    pub layers: &'static [&'static str],
    pub kind: Kind,
    /// The rule in `src/` this pairing comes from, kept so a failure names a real widget.
    pub note: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct CheckGroup {
    pub name: &'static str,
    pub rows: &'static [CheckRow],
}

/// A row after measurement.
#[derive(Debug, Clone)]
pub struct Measurement {
    pub token: &'static str,
    pub layers: &'static [&'static str],
    pub note: &'static str,
    pub kind: Kind,
    pub ratio: f64,
}

impl Measurement {
    pub fn threshold(&self) -> f64 {
        self.kind.threshold()
    }

    pub fn passes(&self) -> bool {
        self.ratio >= self.threshold()
    }
}

/// A colour token the audit deliberately does not gate, still measured against
/// `--bg-panel` so the exemption stays visible instead of silent.
#[derive(Debug, Clone)]
pub struct Exemption {
    pub token: &'static str,
    pub reason: &'static str,
    pub reference_ratio: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct GroupReport {
    pub name: &'static str,
    pub kind: Kind,
    pub measurements: Vec<Measurement>,
}

impl GroupReport {
    pub fn worst(&self) -> Option<&Measurement> {
        self.measurements
            .iter()
            .min_by(|a, b| a.ratio.total_cmp(&b.ratio))
    }

    pub fn passes(&self) -> bool {
        self.measurements.iter().all(Measurement::passes)
    }
}

/// The result of auditing one palette.
#[derive(Debug, Clone)]
pub struct Audit {
    pub palette: &'static str,
    pub groups: Vec<GroupReport>,
    pub exemptions: Vec<Exemption>,
    /// A row that could not be measured, or measured below its threshold.
    pub failures: Vec<String>,
    /// A colour token that is neither measured by a row nor exempted with a reason.
    pub uncovered: Vec<String>,
}

impl Audit {
    pub fn measurements(&self) -> impl Iterator<Item = &Measurement> {
        self.groups.iter().flat_map(|group| &group.measurements)
    }

    pub fn row_count(&self) -> usize {
        self.groups
            .iter()
            .map(|group| group.measurements.len())
            .sum()
    }

    pub fn is_clean(&self) -> bool {
        self.failures.is_empty() && self.uncovered.is_empty()
    }

    /// One line per group, the script's `--summary` shape, for test output.
    pub fn summary(&self) -> String {
        let mut out = format!("{} palette\n\n", self.palette);
        for group in &self.groups {
            let worst = group.worst();
            let measured =
                worst.map_or_else(|| "n/a".to_string(), |row| format!("{:.2}:1", row.ratio));
            let pairing = worst.map_or_else(String::new, |row| {
                format!("  {} on {}", row.token, row.layers.join(" + "))
            });
            let verdict = if group.passes() { "ok  " } else { "FAIL" };
            let _ = writeln!(
                out,
                "  {verdict} {:<48} {:>2} pairs  min {measured}{pairing}  (>= {:.1}:1)",
                group.name,
                group.measurements.len(),
                group.kind.threshold(),
            );
        }
        for failure in self.failures.iter().chain(&self.uncovered) {
            let _ = writeln!(out, "  FAIL {failure}");
        }
        out
    }
}

/// Measure every pairing in the table against one palette, then sweep for colour tokens
/// the table does not account for. Nothing here panics: an unresolvable token becomes a
/// failure naming the token, which is what a retune needs to be actionable.
pub fn audit(theme: &Theme) -> Audit {
    let mut failures = Vec::new();
    let mut groups = Vec::new();
    let mut accounted: Vec<&'static str> = EXEMPT.iter().map(|(token, _)| *token).collect();

    for group in CHECK_GROUPS {
        let mut measurements = Vec::with_capacity(group.rows.len());
        for row in group.rows {
            accounted.push(row.token);
            accounted.extend(row.layers);
            let foreground = match theme.colour(row.token) {
                Ok(colour) => colour,
                Err(error) => {
                    failures.push(describe(theme, row, &error));
                    continue;
                }
            };
            let background = match theme.stack(row.layers) {
                Ok(colour) => colour,
                Err(error) => {
                    failures.push(describe(theme, row, &error));
                    continue;
                }
            };
            measurements.push(Measurement {
                token: row.token,
                layers: row.layers,
                note: row.note,
                kind: row.kind,
                ratio: crate::theme::contrast_ratio(&foreground, &background),
            });
        }
        let kind = group.rows.first().map(|row| row.kind).unwrap_or(Kind::Text);
        groups.push(GroupReport {
            name: group.name,
            kind,
            measurements,
        });
    }

    for failure in groups
        .iter()
        .flat_map(|group| &group.measurements)
        .filter(|measurement| !measurement.passes())
    {
        failures.push(format!(
            "{}: {} on {} = {:.2}:1 (< {}:1) - {}",
            theme.name(),
            failure.token,
            failure.layers.join(" + "),
            failure.ratio,
            failure.kind.as_str(),
            failure.note,
        ));
    }

    let reference = theme.stack(&["--bg-panel"]).ok();
    let exemptions = EXEMPT
        .iter()
        .map(|(token, reason)| Exemption {
            token,
            reason,
            reference_ratio: reference.and_then(|background| {
                theme
                    .colour(token)
                    .ok()
                    .map(|foreground| crate::theme::contrast_ratio(&foreground, &background))
            }),
        })
        .collect();

    for (token, _) in EXEMPT {
        if theme.raw(token).is_none() {
            failures.push(format!(
                "{}: exempt token {token} no longer exists in the stylesheet",
                theme.name()
            ));
        }
    }

    let uncovered = sweep(theme, &accounted);
    Audit {
        palette: theme.name(),
        groups,
        exemptions,
        failures,
        uncovered,
    }
}

/// Every token whose whole value is one flat colour has to be measured by a row or
/// exempted with a reason; this is the script's completeness gate, per palette.
fn sweep(theme: &Theme, accounted: &[&'static str]) -> Vec<String> {
    theme
        .tokens()
        .filter(|token| {
            theme
                .kind(token)
                .ok()
                .is_some_and(TokenKind::is_flat_colour)
        })
        .filter(|token| !accounted.contains(token))
        .map(|token| {
            format!(
                "{}: colour token {token} is neither contrast-checked nor exempt",
                theme.name()
            )
        })
        .collect()
}

/// A row that could not be measured at all, named the way the script names it.
fn describe(theme: &Theme, row: &CheckRow, error: &ThemeError) -> String {
    format!(
        "{}: {} on {} - {error}",
        theme.name(),
        row.token,
        row.layers.join(" + ")
    )
}
