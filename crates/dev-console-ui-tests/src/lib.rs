//! Synthetic fixtures and scripted-interaction helpers for the desktop shell's non-render tests.
//!
//! Plan §1 gives `dev-console-ui-tests` four jobs: synthetic fixtures, scripted interactions,
//! rendering checks and visual acceptance scenes. This crate does the first two today. Rendering
//! checks and visual acceptance scenes need a GPU and a compositor, which this host does not have, so
//! they are *not* approximated here: the element-tree assertions in `tests/` prove structure and
//! state transitions, and nothing in this crate claims to have checked a pixel.
//!
//! ## What a scripted test can and cannot settle
//!
//! `rune::Runtime` is the same runtime the window uses, driven without a display: it lays out a real
//! element tree, routes real pointer presses into real state handlers, and reports the geometry it
//! computed. That is enough to prove which elements exist for a given state, that a press reaches the
//! handler that changes state, and that an out-of-range or empty state cannot produce elements it
//! should not have.
//!
//! It is not enough for hover, focus rings, initial focus, the focus trap, Escape handling, text
//! rasterisation, shadows, or whether the mapped window really is 680×420 on Hyprland. Plan §5 keeps
//! those for a real session; they stay UNVERIFIED in this container.
//!
//! ## Fixtures are synthetic
//!
//! Every string in this crate is made-up test material. AGENTS.md forbids capturing real secret
//! values, and nothing here can reach a profile: `dev-console-desktop` has no storage code at all
//! until plan milestone 2 moves it in.

use dev_console_desktop::ui::{Finder, Library, finder_view, library_view};
use rune::{Cx, El, Env, Runtime};

/// The Finder viewport, in logical points. The same constant the window asks the compositor for
/// (`dev_console_desktop::identity::FINDER_SIZE`); duplicated deliberately so a scripted test still
/// measures the tree at the size the shell intended even if the identity constant drifts.
pub const FINDER_VIEWPORT: (f32, f32) = (680.0, 420.0);

/// The Library viewport, in logical points.
pub const LIBRARY_VIEWPORT: (f32, f32) = (1040.0, 700.0);

/// Synthetic query text. Never a real secret, a real path or real command output (AGENTS.md).
pub const SYNTHETIC_QUERY: &str = "synthetic-query-token";

/// Every key the shell is expected to put in the Finder's tree, so a test can ask "which of these are
/// present" without an enumeration API on `Runtime`.
pub const FINDER_KEYS: &[&str] = &[
    "finder-window",
    "finder-query-row",
    "finder-query",
    "finder-context",
    "finder-rows",
    "finder-empty",
    "finder-footer",
    "footer-up",
    "footer-open",
    "footer-copy",
    "footer-dismiss",
];

/// Keys the Library's tree is expected to carry.
pub const LIBRARY_KEYS: &[&str] = &[
    "library-window",
    "library-header",
    "library-title",
    "vault-status",
    "library-tabs",
    "tab-Commands",
    "tab-Secrets",
    "entry-list",
    "empty-state",
    "library-footer",
    "footer-hint",
];

/// A runtime driven without a display, with the assertions the suites share.
pub struct Stage<S: 'static> {
    runtime: Runtime<S>,
}

impl<S: 'static> Stage<S> {
    /// Builds a stage, lays out one frame at `t = 0`, and leaves the state settled.
    pub fn new(state: S, view: fn(&S, &mut Cx<S>) -> El, viewport: (f32, f32)) -> Self {
        let mut runtime = Runtime::new(state, view, viewport, Env::new(true, 1.0));
        runtime.frame(0.0);
        Self { runtime }
    }

    /// The state a script drove.
    pub fn state(&self) -> &S {
        &self.runtime.state
    }

    /// Lays out another frame, e.g. after a press, so animations and layout settle again.
    pub fn step(&mut self, dt: f64) {
        self.runtime.frame(dt);
    }

    /// True when `key` is in the presented tree.
    pub fn has(&self, key: &str) -> bool {
        self.runtime.presented(key).is_some()
    }

    /// The keys from `candidates` that are present, in the order given.
    pub fn present_from(&self, candidates: &[&'static str]) -> Vec<&'static str> {
        candidates.iter().copied().filter(|key| self.has(key)).collect()
    }

    /// `key`'s laid-out box: `(x, y, width, height)` and the scale it was measured at.
    pub fn box_of(&self, key: &str) -> Option<([f32; 4], f32)> {
        self.runtime.presented(key)
    }

    /// The centre of `key`'s box, for pressing it.
    pub fn centre_of(&self, key: &str) -> Option<(f32, f32)> {
        let ([x, y, width, height], _) = self.box_of(key)?;
        Some((x + width / 2.0, y + height / 2.0))
    }

    /// Presses `key` where it lies. Returns false when the key is not in the tree, so a script that
    /// expects an element can assert rather than press into empty space.
    pub fn press(&mut self, key: &str) -> bool {
        let Some((x, y)) = self.centre_of(key) else {
            return false;
        };
        self.runtime.pointer_down(x, y);
        self.runtime.pointer_up(x, y);
        self.runtime.frame(1.0 / 60.0);
        true
    }

}

/// A stage whose rows are addressable by index, so a script can press the Nth result.
impl<S: RowTarget + 'static> Stage<S> {
    /// Presses row `index`. False when that row is not in the tree.
    pub fn press_row(&mut self, index: usize) -> bool {
        let key = S::row_key(index);
        self.press(&key)
    }
}

impl Stage<Finder> {
    /// The Finder shell with `state`.
    pub fn finder(state: Finder) -> Stage<Finder> {
        Stage::new(state, finder_view, FINDER_VIEWPORT)
    }
}

impl Stage<Library> {
    /// The Library shell with `state`.
    pub fn library(state: Library) -> Stage<Library> {
        Stage::new(state, library_view, LIBRARY_VIEWPORT)
    }
}

/// A stage whose rows are addressable by index, so a script can press the Nth result.
pub trait RowTarget {
    /// The element key of row `index`.
    fn row_key(index: usize) -> String;
}

impl RowTarget for Finder {
    fn row_key(index: usize) -> String {
        format!("finder-row-{index}")
    }
}

impl RowTarget for Library {
    fn row_key(index: usize) -> String {
        format!("entry-{index}")
    }
}

/// A Finder showing three synthetic candidates and a detected project folder.
pub fn finder_with_rows() -> Finder {
    Finder {
        context: "synthetic-project".into(),
        ..Finder::with_rows(&["git status", "cargo test", "just lint"])
    }
}

/// A Library with rows, so the list and the empty state can both be exercised.
pub fn library_with_entries() -> Library {
    Library {
        entries: vec!["synthetic command one".into(), "synthetic command two".into()],
        vault_unlocked: true,
        ..Library::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stage_lays_out_the_finder_before_any_script_runs() {
        let stage = Stage::finder(finder_with_rows());
        assert!(stage.has("finder-window"), "the first frame must present the tree");
        assert_eq!(
            stage.present_from(FINDER_KEYS),
            vec![
                "finder-window",
                "finder-query-row",
                "finder-query",
                "finder-context",
                "finder-rows",
                "finder-footer",
                "footer-up",
                "footer-open",
                "footer-copy",
                "footer-dismiss"
            ]
        );
    }

    #[test]
    fn pressing_a_key_that_is_not_there_presses_nothing() {
        let mut stage = Stage::finder(Finder::default());
        assert!(!stage.press("finder-row-0"), "an empty Finder has no rows to press");
        assert_eq!(stage.state().selected, 0);
    }

    #[test]
    fn fixtures_hold_synthetic_text_only() {
        // The hygiene rule in AGENTS.md is about real values; the fixtures must be obviously synthetic
        // so a captured tree can never be mistaken for user data.
        let finder = finder_with_rows();
        assert_eq!(finder.context, "synthetic-project");
        let library = library_with_entries();
        assert!(library.entries.iter().all(|entry| entry.starts_with("synthetic")));
        assert!(SYNTHETIC_QUERY.starts_with("synthetic"));
    }
}
