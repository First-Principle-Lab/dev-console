//! Compact relative timestamps and the absolute fallback.
//!
//! Ported from `src/lib/time.ts`, the last of the four TypeScript-only modules plan §3 names
//! ("drafts, validation, argument parsing, formatting, and Finder selection logic"). Rows and
//! headers show `2h ago` rather than a locale date: an absolute date is both wider (it clipped
//! whole rows in the review screenshots) and useless at a glance. The absolute value stays
//! available for tooltips and for the details pane, so nothing is lost.
//!
//! Two things are deliberate about the shape of these functions:
//!
//! * `now` is a parameter. The webview read `Date.now()` inside the formatter, which makes the
//!   unit boundaries impossible to test without sleeping. Passing the clock in keeps the
//!   arithmetic checkable and matches how the rest of the crate treats time
//!   ([`crate::repository::now_rfc3339`] is the one place the clock is read).
//! * rounding is half-up, like `Math.round`. The unit steps chain (`minutes -> hours -> days ->
//!   months -> years`, each rounded from the previous), which is what the shipped labels have
//!   always meant: 23h30m is already "1d ago" on every screen in the current app.
//!
//! A value that is not RFC 3339 - the format [`crate::repository::now_rfc3339`] writes - renders
//! as the empty string, exactly as the webview's `Date.parse` -> `NaN` guard did. Showing a guess
//! for a timestamp the app cannot read is worse than showing nothing next to it.

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Reads a stored timestamp. `None` for anything that is not RFC 3339, which the callers render
/// as an empty label rather than as a wrong one.
pub fn parse_timestamp(iso: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(iso, &Rfc3339).ok()
}

/// `Math.round` for the non-negative, integer ratios this module works in: `floor(n/d + 1/2)`,
/// computed without a float so the boundaries are exact.
fn round_div(numerator: i128, denominator: i128) -> i64 {
    ((2 * numerator + denominator) / (2 * denominator)) as i64
}

/// The row label: `just now`, `12m ago`, `3h ago`, `5d ago`, `2mo ago`, `1y ago`.
///
/// A timestamp in the future (a clock skew of a few seconds between this machine and a file it
/// just read) clamps to `just now` instead of showing a negative age.
pub fn relative_time(iso: &str, now: OffsetDateTime) -> String {
    let Some(then) = parse_timestamp(iso) else {
        return String::new();
    };
    let micros = (now - then).whole_microseconds().max(0);
    let minutes = round_div(micros, 60 * 1_000_000);
    if minutes < 1 {
        return "just now".to_string();
    }
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = round_div(minutes as i128, 60);
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = round_div(hours as i128, 24);
    if days < 30 {
        return format!("{days}d ago");
    }
    let months = round_div(days as i128, 30);
    if months < 12 {
        return format!("{months}mo ago");
    }
    format!("{}y ago", round_div(months as i128, 12))
}

/// [`relative_time`] against the wall clock, which is what a view calls.
pub fn relative_time_now(iso: &str) -> String {
    let now = OffsetDateTime::now_utc();
    relative_time(iso, now)
}

/// The absolute value behind a relative label: `2026-11-08 09:10`, always UTC.
///
/// The same value the webview built by slicing an ISO string to the minute
/// (`toISOString().slice(0, 16).replace('T', ' ')`), so a stored `+05:30` offset is shown in UTC
/// and the label is stable regardless of the machine's zone.
pub fn absolute_time(iso: &str) -> String {
    let Some(then) = parse_timestamp(iso) else {
        return String::new();
    };
    let stamp = then.to_utc().format(&Rfc3339).unwrap_or_default();
    stamp.chars().take(16).collect::<String>().replace('T', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed clock: every boundary below is asserted against it, which is only possible because
    /// the formatter does not read the clock itself.
    fn at(iso: &str) -> OffsetDateTime {
        parse_timestamp(iso).unwrap_or_else(|| panic!("fixture timestamp is not RFC 3339: {iso}"))
    }

    const NOW: &str = "2026-11-08T12:00:00.000Z";

    /// `then` is `seconds` before [`NOW`], built by the same arithmetic the labels use.
    fn ago(seconds: i64) -> String {
        let then = at(NOW) - time::Duration::seconds(seconds);
        then.format(&Rfc3339).unwrap()
    }

    #[test]
    fn says_just_now_inside_the_first_minute() {
        let now = at(NOW);
        assert_eq!(relative_time(&ago(0), now), "just now");
        assert_eq!(relative_time(&ago(29), now), "just now");
        // Half-up rounding, like Math.round: 30s already reads as a minute.
        assert_eq!(relative_time(&ago(30), now), "1m ago");
        assert_eq!(relative_time(&ago(59), now), "1m ago");
    }

    #[test]
    fn counts_minutes_until_an_hour() {
        let now = at(NOW);
        assert_eq!(relative_time(&ago(90), now), "2m ago");
        assert_eq!(relative_time(&ago(59 * 60), now), "59m ago");
        // 59m30s rounds to 60 minutes, which is no longer "minutes".
        assert_eq!(relative_time(&ago(59 * 60 + 30), now), "1h ago");
    }

    #[test]
    fn counts_hours_until_a_day() {
        let now = at(NOW);
        assert_eq!(relative_time(&ago(5 * 3600), now), "5h ago");
        assert_eq!(relative_time(&ago(23 * 3600), now), "23h ago");
        // 23h30m is 24 rounded hours, so the label steps to days, as it always has.
        assert_eq!(relative_time(&ago(23 * 3600 + 30 * 60), now), "1d ago");
    }

    #[test]
    fn counts_days_until_a_month() {
        let now = at(NOW);
        assert_eq!(relative_time(&ago(3 * 86400), now), "3d ago");
        assert_eq!(relative_time(&ago(29 * 86400), now), "29d ago");
        assert_eq!(relative_time(&ago(29 * 86400 + 12 * 3600), now), "1mo ago");
    }

    #[test]
    fn counts_months_until_a_year() {
        let now = at(NOW);
        assert_eq!(relative_time(&ago(330 * 86400), now), "11mo ago");
        assert_eq!(relative_time(&ago(360 * 86400), now), "1y ago");
    }

    #[test]
    fn clamps_a_timestamp_from_the_future() {
        let now = at(NOW);
        let skewed = (at(NOW) + time::Duration::seconds(45))
            .format(&Rfc3339)
            .unwrap();
        assert_eq!(relative_time(&skewed, now), "just now");
    }

    #[test]
    fn says_nothing_about_a_value_it_cannot_read() {
        let now = at(NOW);
        for garbage in ["", "not a date", "2026-11-08", "08/11/2026 12:00"] {
            assert_eq!(relative_time(garbage, now), "");
            assert_eq!(absolute_time(garbage), "");
        }
    }

    #[test]
    fn absolute_time_shows_the_utc_minute() {
        assert_eq!(
            absolute_time("2026-11-08T09:10:11.123Z"),
            "2026-11-08 09:10"
        );
        assert_eq!(
            absolute_time("2026-11-08T09:10:11+00:00"),
            "2026-11-08 09:10"
        );
        // The label is UTC whatever offset the value carried, because the minute shown is the
        // instant's UTC minute (the webview sliced `toISOString`).
        assert_eq!(
            absolute_time("2026-11-08T09:10:00+05:30"),
            "2026-11-08 03:40"
        );
    }

    #[test]
    fn the_clock_overload_matches_the_stored_format() {
        let now = crate::repository::now_rfc3339();
        assert!(parse_timestamp(&now).is_some(), "{now} is RFC 3339");
        assert_eq!(relative_time_now(&now), "just now");
        assert!(!absolute_time(&now).is_empty());
    }
}
