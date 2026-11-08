//! The CSS-shaped values a theme token can hold, and the arithmetic the contrast
//! audit runs on them.
//!
//! Everything here is a port of one function in `scripts/contrast-check.mjs`, kept
//! at the same precision and the same acceptance rules so the two audits cannot
//! disagree about a palette: `colour()` parses the same three notations and nothing
//! else, `over()` composites the same way, and `luminance()` / `contrast_ratio()`
//! are the WCAG 2.1 relative-luminance formula the shipped script uses.
//!
//! Two rules here exist because a naive port gets them wrong:
//!
//! - `expand_vars` resolves iteratively with an equality check per pass, so a legal
//!   three-deep chain resolves and a real cycle is reported instead of recursing.
//! - [`Length`] keeps its unit. Dropping it turns `--radius-circle: 50%` into 50 px
//!   and `--label-tracking: 0.06em` into 0.06 px, and an unsupported unit is an error
//!   rather than a silent zero.

use std::fmt;

/// An sRGB colour: channels in 0.0..=255.0, alpha in 0.0..=1.0.
///
/// `transparent` is black at alpha 0, exactly as the shipped audit parses it, which is
/// what makes a `--material-tint: transparent` layer composite to the surface under it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

impl Rgba {
    pub const fn opaque(red: f64, green: f64, blue: f64) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }

    /// Composite this layer over `background`, the same `over()` the audit uses: an
    /// opaque layer replaces what is under it, a translucent one blends channel-wise.
    pub fn over(&self, background: &Self) -> Self {
        if self.alpha >= 1.0 {
            return Self {
                red: self.red,
                green: self.green,
                blue: self.blue,
                alpha: 1.0,
            };
        }
        let blend = |front: f64, back: f64| front * self.alpha + back * (1.0 - self.alpha);
        Self {
            red: blend(self.red, background.red),
            green: blend(self.green, background.green),
            blue: blend(self.blue, background.blue),
            alpha: 1.0,
        }
    }

    /// WCAG 2.1 relative luminance.
    pub fn luminance(&self) -> f64 {
        let channel = |raw: f64| {
            let value = raw / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(self.red) + 0.7152 * channel(self.green) + 0.0722 * channel(self.blue)
    }
}

/// The contrast ratio between two painted colours: `(L1 + 0.05) / (L2 + 0.05)`.
pub fn contrast_ratio(a: &Rgba, b: &Rgba) -> f64 {
    let first = a.luminance();
    let second = b.luminance();
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

/// Everything that can go wrong while reading a theme.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    /// A palette name the registry does not hold.
    #[error("there is no palette named `{name}`")]
    UnknownPalette { name: String },
    /// A `var(--x)` or a `--x` reference with no declaration in the palette or the
    /// shared block. The shipped script throws here; so does this.
    #[error("`{token}` is not declared in the `{palette}` palette or the shared tokens")]
    Undeclared { palette: String, token: String },
    /// A `var()` chain that never settles.
    #[error("`var()` does not settle while resolving `{value}` (a token refers to itself)")]
    VarCycle { value: String },
    /// The audit needs a colour and found something else.
    #[error("`{reference}` holds no colour (resolves to `{value}`)")]
    NotAColour { reference: String, value: String },
    /// A sizing token whose value this port cannot represent.
    #[error("`{token}` = `{value}` is not a length in a unit this port understands (px, em, %)")]
    NotALength { token: String, value: String },
    /// A token expected to be a bare number.
    #[error("`{token}` = `{value}` is not a number")]
    NotANumber { token: String, value: String },
    /// A declared token that maps to none of the categories in [`TokenKind`], which is
    /// the failure mode C5's completeness test is there to catch.
    #[error("`{token}` = `{value}` maps to no theme token kind")]
    Unmapped { token: String, value: String },
}

/// A length with its unit attached, because dropping the unit is how a port silently
/// turns `50%` into `50px`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Length {
    pub value: f64,
    pub unit: LengthUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnit {
    Px,
    Em,
    Percent,
}

impl fmt::Display for LengthUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = match self {
            Self::Px => "px",
            Self::Em => "em",
            Self::Percent => "%",
        };
        f.write_str(unit)
    }
}

impl fmt::Display for Length {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.value, self.unit)
    }
}

/// Parse a length. An unsupported unit (`rem`, `ch`, `vw`, a `calc()`) is an error
/// rather than a zero, so a new unit shows up as a failing test.
pub fn parse_length(token: &str, text: &str) -> Result<Length, ThemeError> {
    let text = text.trim();
    let unmapped = || ThemeError::NotALength {
        token: token.to_string(),
        value: text.to_string(),
    };
    let (number, unit) = if let Some(rest) = text.strip_suffix('%') {
        (rest, LengthUnit::Percent)
    } else if let Some(rest) = text.strip_suffix("px") {
        (rest, LengthUnit::Px)
    } else if let Some(rest) = text.strip_suffix("em") {
        (rest, LengthUnit::Em)
    } else {
        return Err(unmapped());
    };
    number
        .trim()
        .parse::<f64>()
        .map_err(|_| unmapped())
        .map(|value| Length { value, unit })
}

/// Parse a unitless number (`--leading: 1.45`).
pub fn parse_number(token: &str, text: &str) -> Result<f64, ThemeError> {
    text.trim()
        .parse::<f64>()
        .map_err(|_| ThemeError::NotANumber {
            token: token.to_string(),
            value: text.trim().to_string(),
        })
}

/// Parse one flat colour, or `None` when the value is not a single colour. This is the
/// shipped `colour()` function: `transparent`, `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`,
/// and `rgb()`/`rgba()` with comma, space or slash separators and optional percentages.
/// Anything else - a shadow, a border, a font stack - is `None`, which is what makes
/// `--shadow-shell` exempt from the contrast sweep while `--danger` is not.
pub fn parse_colour(text: &str) -> Option<Rgba> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text.eq_ignore_ascii_case("transparent") {
        return Some(Rgba {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 0.0,
        });
    }
    parse_hex(text).or_else(|| parse_rgb_function(text))
}

fn parse_hex(text: &str) -> Option<Rgba> {
    let digits = text.strip_prefix('#')?;
    if !matches!(digits.len(), 3 | 4 | 6 | 8)
        || !digits.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    let full: String = if digits.len() == 3 || digits.len() == 4 {
        digits.chars().flat_map(|digit| [digit, digit]).collect()
    } else {
        digits.to_string()
    };
    let byte = |start: usize| u8::from_str_radix(&full[start..start + 2], 16).map(f64::from);
    let red = byte(0).ok()?;
    let green = byte(2).ok()?;
    let blue = byte(4).ok()?;
    let alpha = if full.len() == 8 {
        byte(6).ok()? / 255.0
    } else {
        1.0
    };
    Some(Rgba {
        red,
        green,
        blue,
        alpha,
    })
}

fn parse_rgb_function(text: &str) -> Option<Rgba> {
    let lowered = text.to_ascii_lowercase();
    let inner = lowered
        .strip_prefix("rgb(")
        .or_else(|| lowered.strip_prefix("rgba("))?;
    let inner = inner.strip_suffix(')')?;
    let parts: Vec<&str> = inner
        .split(|c: char| c == ',' || c.is_ascii_whitespace() || c == '/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 3 || parts.len() > 4 {
        return None;
    }
    let mut channel = [1.0_f64; 4];
    for (index, part) in parts.iter().enumerate() {
        let value = match part.strip_suffix('%') {
            Some(percent) => {
                percent.parse::<f64>().ok()? / 100.0 * if index == 3 { 1.0 } else { 255.0 }
            }
            None => part.parse::<f64>().ok()?,
        };
        if !value.is_finite() {
            return None;
        }
        channel[index] = value;
    }
    Some(Rgba {
        red: channel[0],
        green: channel[1],
        blue: channel[2],
        alpha: channel[3],
    })
}

/// Expand `var(--token)` and `var(--token, fallback)` until the value settles.
///
/// Iterative with an equality check per pass: a three-deep chain resolves, a token that
/// refers to itself is reported as [`ThemeError::VarCycle`] instead of recursing.
pub fn expand_vars<'a, F>(value: &str, lookup: F) -> Result<String, ThemeError>
where
    F: Fn(&str) -> Option<&'a str>,
{
    let mut text = value.to_string();
    for _ in 0..32 {
        let next = expand_once(&text, &lookup)?;
        if next == text {
            // Settled. Anything still holding a `var(` after a pass that changed
            // nothing refers to itself, which is a cycle rather than a finished value.
            return if text.contains("var(") {
                Err(ThemeError::VarCycle {
                    value: value.to_string(),
                })
            } else {
                Ok(text)
            };
        }
        text = next;
    }
    Err(ThemeError::VarCycle {
        value: value.to_string(),
    })
}

fn expand_once<'a, F>(text: &str, lookup: &F) -> Result<String, ThemeError>
where
    F: Fn(&str) -> Option<&'a str>,
{
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("var(") {
        out.push_str(&rest[..at]);
        let closing = match find_close(rest, at + 4) {
            Some(index) => index,
            None => return Ok(out + &rest[at..]),
        };
        let inner = &rest[at + 4..closing];
        let (name, fallback) = match inner.find(',') {
            Some(at) => (inner[..at].trim(), Some(inner[at + 1..].trim())),
            None => (inner.trim(), None),
        };
        if !name.starts_with("--") {
            // Not a token reference; leave the text alone so the equality check ends
            // the expansion instead of looping on it.
            out.push_str(&rest[at..closing + 1]);
            rest = &rest[closing + 1..];
            continue;
        }
        match lookup(name).or(fallback) {
            Some(found) => out.push_str(found),
            None => {
                return Err(ThemeError::Undeclared {
                    palette: String::new(),
                    token: name.to_string(),
                })
            }
        }
        rest = &rest[closing + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Index of the `)` that closes the group starting at `from`, counting nested parens so
/// `rgba(0, 0, 0, 0.5)` inside a fallback stays intact.
fn find_close(text: &str, from: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, char) in text.char_indices().skip_while(|(index, _)| *index < from) {
        match char {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some(offset);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// What a token holds. C5's completeness test asserts every declared token in every
/// palette maps to exactly one of these, so a new token with a shape this port has never
/// seen fails the suite instead of rendering as a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// A single flat colour, including `transparent`.
    Colour,
    /// A length in px, em or %.
    Length,
    /// A unitless number (`--leading`).
    Ratio,
    /// A comma-separated list of family names (`--font-ui`).
    FontStack,
    /// `<length> <style> <colour>` (`--border`).
    Border,
    /// One or more `<offset> <blur> <spread> <colour>` layers, optionally `inset`
    /// (`--shadow-shell`, `--selected-marker`).
    Shadow,
    /// `<time> <easing>` (`--transition`).
    Transition,
}

impl TokenKind {
    /// True when the whole value is one flat colour. This is the predicate the shipped
    /// audit uses for its completeness sweep (`colour(resolve(value)) !== null`), so the
    /// Rust sweep gates exactly the same tokens: a shadow or a border shorthand holds
    /// colours but is measured through the token it paints, not by the sweep.
    pub fn is_flat_colour(self) -> bool {
        matches!(self, Self::Colour)
    }
}

/// Classify a resolved token value.
pub fn classify(token: &str, value: &str) -> Result<TokenKind, ThemeError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ThemeError::Unmapped {
            token: token.to_string(),
            value: value.to_string(),
        });
    }
    if parse_colour(value).is_some() {
        return Ok(TokenKind::Colour);
    }
    if is_border(value) {
        return Ok(TokenKind::Border);
    }
    if is_shadow(value) {
        return Ok(TokenKind::Shadow);
    }
    if is_transition(value) {
        return Ok(TokenKind::Transition);
    }
    if parse_length(token, value).is_ok() {
        return Ok(TokenKind::Length);
    }
    if parse_number(token, value).is_ok() {
        return Ok(TokenKind::Ratio);
    }
    if is_font_stack(value) {
        return Ok(TokenKind::FontStack);
    }
    Err(ThemeError::Unmapped {
        token: token.to_string(),
        value: value.to_string(),
    })
}

/// Split on a separator that is outside any parenthesis.
pub fn split_top(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (index, char) in text.char_indices() {
        match char {
            '(' => depth += 1,
            ')' => depth -= 1,
            other if other == separator && depth == 0 => {
                parts.push(text[start..index].trim());
                start = index + other.len_utf8();
            }
            _ => {}
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() || parts.is_empty() {
        parts.push(tail);
    }
    parts.retain(|part| !part.is_empty());
    parts
}

/// Whitespace-separated tokens, keeping `rgba(0, 0, 0, 0.5)` in one piece.
pub fn words(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start: Option<usize> = None;
    for (index, char) in text.char_indices() {
        match char {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if char.is_whitespace() && depth == 0 => {
                if let Some(from) = start.take() {
                    out.push(&text[from..index]);
                }
            }
            _ => {
                if start.is_none() {
                    start = Some(index);
                }
            }
        }
    }
    if let Some(from) = start {
        out.push(&text[from..]);
    }
    out
}

const BORDER_STYLES: [&str; 4] = ["solid", "dashed", "dotted", "double"];
const EASING: [&str; 8] = [
    "ease",
    "linear",
    "ease-in",
    "ease-out",
    "ease-in-out",
    "step-start",
    "step-end",
    "initial",
];

fn is_border(value: &str) -> bool {
    let parts = words(value);
    parts.len() == 3
        && parse_length("", parts[0]).is_ok()
        && BORDER_STYLES.contains(&parts[1])
        && parse_colour(parts[2]).is_some()
}

/// `0 24px 60px rgba(0, 0, 0, 0.55), 0 2px 8px rgba(0, 0, 0, 0.35)` and
/// `inset 2px 0 0 #7c8cf8`: every comma layer ends in a colour.
fn is_shadow(value: &str) -> bool {
    let layers = split_top(value, ',');
    !layers.is_empty()
        && layers.iter().all(|layer| {
            let mut parts = words(layer);
            if parts.first().is_some_and(|part| *part == "inset") {
                parts.remove(0);
            }
            parts.len() >= 4
                && parse_colour(parts[parts.len() - 1]).is_some()
                && parts[..parts.len() - 1]
                    .iter()
                    .all(|part| *part == "0" || parse_length("", part).is_ok())
        })
}

fn is_transition(value: &str) -> bool {
    let parts = words(value);
    parts.len() == 2 && parse_duration(parts[0]).is_some() && EASING.contains(&parts[1])
}

fn parse_duration(text: &str) -> Option<f64> {
    if let Some(ms) = text.strip_suffix("ms") {
        ms.parse::<f64>().ok()
    } else {
        text.strip_suffix('s')?.parse::<f64>().ok()
    }
}

fn is_font_stack(value: &str) -> bool {
    let parts = split_top(value, ',');
    parts.len() > 1
        && parts.iter().all(|part| {
            let part = part.trim();
            let bare = part.trim_matches('\'').trim_matches('"');
            !bare.is_empty()
                && bare
                    .chars()
                    .all(|c| c.is_ascii_alphabetic() || matches!(c, '-' | ' '))
        })
}
