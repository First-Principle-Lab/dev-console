//! The theme data both apps read, ported out of the stylesheet (plan §2, checkpoint C5).
//!
//! `src/styles/tokens.css` and `src/styles/omarchy.css` are the source of truth for the
//! shipped Svelte UI; the tables in [`palettes`] and [`shared`] are a generated copy of
//! them, and [`inventory`] re-parses the stylesheets at test time and fails if a row has
//! drifted. That pairing is the point: the native app cannot quietly disagree with the
//! stylesheet about a colour, and neither can be retuned without the other failing.
//!
//! What a palette holds, in the order the stylesheets declare them:
//!
//! - `dark` and `light`, from `tokens.css`, and 22 Omarchy palettes from `omarchy.css`.
//!   Each palette restates the same 49 colour-bearing tokens; the 52 shared tokens
//!   (type scale, spacing, radii, borders, control sizes, motion) are declared once.
//! - Lookups check the palette first and the shared block second, which is how the
//!   stylesheet cascades, so `--gap: var(--sp-3)` still follows a palette that retunes
//!   `--sp-3`.
//!
//! [`contrast`] is the port of `scripts/contrast-check.mjs`: same pairings, same
//! thresholds (4.5:1 text, 3.0:1 boundary), same exemptions with their reasons, and the
//! same completeness sweep that fails when a new colour token is neither measured nor
//! exempted on purpose.

pub mod contrast;
pub mod value;

mod checks;
mod palettes;
mod shared;

#[cfg(test)]
mod audit_tests;
#[cfg(test)]
mod inventory;
#[cfg(test)]
mod tests;

use palettes::PALETTES;
use shared::SHARED_ROWS;

pub use crate::theme::value::{
    classify, contrast_ratio, expand_vars, parse_colour, parse_length, parse_number, Length,
    LengthUnit, Rgba, ThemeError, TokenKind,
};

/// Whether a palette is painted on a dark or a light surface, taken from the
/// `color-scheme` the stylesheet block declares (the shared `:root` for `dark`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Dark,
    Light,
}

impl Appearance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }
}

/// One named palette: the 49 tokens its stylesheet block restates, plus the appearance
/// its `color-scheme` declares.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub name: &'static str,
    pub appearance: Appearance,
    pub rows: &'static [(&'static str, &'static str)],
}

/// Every palette, in the order the stylesheets declare them. The settings list renders
/// this order; `inventory` asserts it still matches the stylesheet.
pub fn palettes() -> &'static [&'static Palette] {
    PALETTES
}

/// The palette names, in registry order.
pub fn palette_names() -> Vec<&'static str> {
    PALETTES.iter().map(|palette| palette.name).collect()
}

/// A palette resolved against the shared tokens: the thing a view asks for a colour.
///
/// Equality is identity: two themes are the same when they resolve the same palette, so a
/// comparison never walks the token table.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    palette: &'static Palette,
}

impl PartialEq for Theme {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.palette, other.palette)
    }
}

impl Eq for Theme {}

impl Theme {
    /// The `dark` palette, which is what `tokens.css` applies with no `data-theme`.
    pub fn dark() -> Self {
        Self::named("dark").expect("the dark palette is in the registry")
    }

    pub fn light() -> Self {
        Self::named("light").expect("the light palette is in the registry")
    }

    pub fn named(name: &str) -> Result<Self, ThemeError> {
        PALETTES
            .iter()
            .copied()
            .find(|palette| palette.name == name)
            .map(|palette| Self { palette })
            .ok_or_else(|| ThemeError::UnknownPalette {
                name: name.to_string(),
            })
    }

    /// Every registered palette as a theme, in registry order.
    pub fn all() -> Vec<Theme> {
        PALETTES.iter().map(|palette| Self { palette }).collect()
    }

    pub fn palette(self) -> &'static Palette {
        self.palette
    }

    pub fn name(self) -> &'static str {
        self.palette.name
    }

    pub fn appearance(self) -> Appearance {
        self.palette.appearance
    }

    /// The declared text of a token with no `var()` expansion: the palette's own row
    /// first, then the shared block, which is the stylesheet's cascade order.
    pub fn raw(self, token: &str) -> Option<&'static str> {
        self.palette
            .rows
            .iter()
            .chain(SHARED_ROWS.iter())
            .find(|(name, _)| *name == token)
            .map(|(_, value)| *value)
    }

    /// Every token name this theme declares: the palette's 49 then the shared 52.
    pub fn tokens(self) -> impl Iterator<Item = &'static str> {
        let palette = self.palette;
        palette
            .rows
            .iter()
            .map(|(name, _)| *name)
            .chain(SHARED_ROWS.iter().map(|(name, _)| *name))
    }

    /// Expand a token's `var()` references until the value is plain CSS.
    pub fn resolve(self, token: &str) -> Result<String, ThemeError> {
        let raw = self.raw(token).ok_or_else(|| ThemeError::Undeclared {
            palette: self.name().to_string(),
            token: token.to_string(),
        })?;
        self.expand(raw).map_err(|error| match error {
            ThemeError::Undeclared { token, .. } => ThemeError::Undeclared {
                palette: self.name().to_string(),
                token,
            },
            other => other,
        })
    }

    /// Resolve a reference that is either a token name or a literal CSS value, which is
    /// what a contrast row needs: `--fg` resolves the token, `#ffffff` resolves itself.
    pub fn value(self, reference: &str) -> Result<String, ThemeError> {
        if reference.starts_with("--") {
            return self.resolve(reference);
        }
        self.expand(reference)
    }

    fn expand(self, text: &str) -> Result<String, ThemeError> {
        expand_vars(text, |token| self.raw(token))
    }

    /// The colour a reference paints. A shorthand that *contains* a colour resolves to
    /// it, so `--selected-marker` (`inset 2px 0 0 var(--accent)`) measures the marker
    /// colour rather than failing on the shorthand - the shipped audit's `resolveColour`.
    pub fn colour(self, reference: &str) -> Result<Rgba, ThemeError> {
        let value = self.value(reference)?;
        if let Some(colour) = parse_colour(&value) {
            return Ok(colour);
        }
        for layer in value::split_top(&value, ',') {
            for part in value::words(layer) {
                if let Some(colour) = parse_colour(part) {
                    return Ok(colour);
                }
            }
        }
        Err(ThemeError::NotAColour {
            reference: reference.to_string(),
            value,
        })
    }

    /// Composite a stack of layers, topmost first, onto black - how the browser paints
    /// a translucent `--selected` band over a `--shell-bg` surface.
    pub fn stack(self, layers: &[&str]) -> Result<Rgba, ThemeError> {
        let mut out = Rgba::opaque(0.0, 0.0, 0.0);
        for layer in layers.iter().rev() {
            out = self.colour(layer)?.over(&out);
        }
        Ok(out)
    }

    /// A sizing token as a number with its unit.
    pub fn length(self, token: &str) -> Result<Length, ThemeError> {
        let value = self.resolve(token)?;
        parse_length(token, &value)
    }

    /// A unitless token (`--leading`).
    pub fn number(self, token: &str) -> Result<f64, ThemeError> {
        let value = self.resolve(token)?;
        parse_number(token, &value)
    }

    /// What a token holds. Every declared token in every palette has to answer this,
    /// which is the completeness promise C5 is tested against.
    pub fn kind(self, token: &str) -> Result<TokenKind, ThemeError> {
        let value = self.resolve(token)?;
        classify(token, &value)
    }
}
