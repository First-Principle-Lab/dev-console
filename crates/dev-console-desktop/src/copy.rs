//! The copy seam: how a highlighted candidate reaches the clipboard, and what the shell may keep
//! afterwards.
//!
//! The shipped palette's `activate()` does three things and this module mirrors all three: it asks
//! the backend for the copy (`api.copy(entry.kind, entry.id, 0)`), it reports *what happened* and
//! never what was copied (`status = 'Copied the secret to the clipboard.'`, with the source comment
//! "The value itself never appears here"), and it closes only on success (`api.hideSelf()` after the
//! `await`, with the failure path staying open to show `Copy failed. Nothing was copied.`).
//!
//! ## Two clipboard owners, and which one this shell uses
//!
//! [`dev_console_core::clipboard::Clipboard`] writes through `wl-copy`/`xclip` and keeps its own
//! pending value for the expiry worker, because the shipped Tauri app has no clipboard of its own.
//! Rune does: [`Tx::copy_secret`] writes the text, zeroes the buffer it was given, holds a keyed
//! fingerprint of what it wrote, and clears the board when the deadline passes *only if nothing else
//! has been copied since*. That is the shipped `expire_due`/`clear_owned` rule
//! ("clear only when the clipboard still holds the value this app copied") implemented in the toolkit
//! rather than beside it, and it is why the native shell hands the write to Rune: two owners of the
//! same expiry is a bug. What stays in `dev-console-core` is the part only the backend can do -
//! turning an entry id into a value, behind the revision guard - which is exactly what [`CopySource`]
//! is.
//!
//! [`Tx::copy_secret`]: rune::Tx::copy_secret

use std::time::Duration;

use dev_console_core::models::{CopyRequest, Kind, Settings};

/// How long a copied value stays on the clipboard by default.
///
/// Read from the same place the shipped app reads it (the `clipboardExpirySeconds` setting's default)
/// rather than restated here, so the two apps cannot drift apart. The service layer clamps a stored
/// value to 5..=3600 when settings load ([`Settings`] holds the default).
pub fn default_expiry() -> Duration {
    Duration::from_secs(Settings::default().clipboard_expiry_seconds)
}

/// Where a highlighted row's value comes from.
///
/// This is the one seam through which a value can reach a window, and it is deliberately a `dyn`:
/// a view-state struct holds a *pointer to a resolver*, never a field a value could sit in, so
/// `Debug`, an element dump and a log line have nothing to print. The real implementation is the
/// service call the shipped palette makes - `copy(kind, id, expected_revision)`, which reads the
/// entry, checks the revision and hands the plaintext straight to the clipboard.
///
/// Returning `None` is the failure the shipped app reports as `Copy failed. Nothing was copied.`:
/// a locked vault, a deleted row, a refused revision or an unreadable archive all arrive here, and
/// none of them may be reported *with* the reason the backend refused to print.
pub trait CopySource {
    /// The exact text the entry named by `request` copies as, or `None` when it cannot be resolved.
    fn value_for(&self, request: &CopyRequest) -> Option<String>;
}

/// The copy source the shell constructs today: there is no repository or vault wired into
/// `dev-console-desktop` yet (plan milestone 2 moves the services in), so nothing resolves and every
/// copy takes the shipped failure path.
///
/// This is not a placeholder that fakes success - it is the honest shape of "the native palette
/// cannot copy an entry yet", stated where the seam that will change it lives. The same `None` is
/// what a locked vault produces on a real desktop, so the failure path this returns is the one the
/// shipped app already shows users.
pub struct Unwired;

impl CopySource for Unwired {
    fn value_for(&self, _request: &CopyRequest) -> Option<String> {
        None
    }
}

/// What a copy attempt left behind that the window is allowed to show.
///
/// `CopyResult` is what the backend hands the frontend in the shipped app: metadata only, by design
/// ("the value itself never appears here"). This is the view's mirror of it, and deliberately an enum
/// of fixed strings rather than a `String` a caller could mistake for a place to put a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyOutcome {
    /// The command the highlight named went onto the clipboard.
    CommandCopied,
    /// The secret the highlight named went onto the clipboard. Only the fact crosses into the view.
    SecretCopied,
    /// Nothing was written, and the palette stays open to say so.
    Failed,
}

impl CopyOutcome {
    /// The line the shipped palette shows. Verbatim, including the full stop, because the e2e specs
    /// in `tests/e2e/` assert on copied-state text and a paraphrase would make the two apps disagree
    /// about what they promise.
    pub const fn message(self) -> &'static str {
        match self {
            CopyOutcome::CommandCopied => "Copied the command to the clipboard.",
            CopyOutcome::SecretCopied => "Copied the secret to the clipboard.",
            CopyOutcome::Failed => "Copy failed. Nothing was copied.",
        }
    }

    /// True when the shipped `activate()` would have reached its `api.hideSelf()` line: the palette
    /// closes on a copy that happened and stays open on one that did not.
    pub const fn succeeded(self) -> bool {
        !matches!(self, CopyOutcome::Failed)
    }

    /// The outcome the backend reports for an entry of `kind` whose copy went through.
    pub const fn for_kind(kind: Kind) -> CopyOutcome {
        match kind {
            Kind::Command => CopyOutcome::CommandCopied,
            Kind::Secret => CopyOutcome::SecretCopied,
        }
    }
}
