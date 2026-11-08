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
//! It is not enough for hover, focus rings, text rasterisation, shadows, or whether the mapped
//! window really is 680×420 on Hyprland: those are what the frame *looks* like, and plan §5 keeps
//! them for a real session. What it does settle, because Rune keeps it as runtime state rather than
//! as paint, is *which* element holds keyboard focus and what the keyboard does to it - so initial
//! focus, the Tab order, a modal's focus trap and its restore on close are all asserted here (see
//! `tests/dialog.rs`). A focus *ring* is still paint: these tests can say which element is focused,
//! never that the user can see it.
//!
//! ## Fixtures are synthetic
//!
//! Every string in this crate is made-up test material. AGENTS.md forbids capturing real secret
//! values, and nothing here can reach a profile: `dev-console-desktop` has no storage code at all
//! until plan milestone 2 moves it in.
//!
//! ## The clipboard is real, and in-memory
//!
//! Every [`Stage`] hands its runtime a [`rune::clipboard::MemoryClipboard`] instead of the OS board
//! (`Runtime::use_clipboard`), which is what makes the copy seam checkable on a host with no
//! Wayland or X11 session at all: the write, the auto-clear deadline and the "something else was
//! copied since" guard are Rune's own code paths, driven for real, with no compositor in the loop.
//! What that cannot decide is the OS board itself - whether `wl-clipboard`/`xclip` really received the
//! text, and whether a clipboard *manager* archived a secret anyway - which stays the
//! UNVERIFIED (owner desktop) row it is in the feasibility ledger.
//!
//! Read the board with [`Stage::clipboard_holds`], not [`Stage::clipboard_text`], wherever the
//! assertion only needs to know *whether* a value landed: `holds` never puts the text into a failure
//! message, which is the difference between asserting a copy happened and printing what it copied.

use dev_console_core::models::CopyRequest;
use dev_console_desktop::app::library_spec;
use dev_console_desktop::copy::CopySource;
use dev_console_desktop::ui::{finder_view, Candidate, Finder, Library};
use rune::clipboard::MemoryClipboard;
use rune::{AppSpec, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Tx};

/// The Finder viewport, in logical points. The same constant the window asks the compositor for
/// (`dev_console_desktop::identity::FINDER_SIZE`); duplicated deliberately so a scripted test still
/// measures the tree at the size the shell intended even if the identity constant drifts.
pub const FINDER_VIEWPORT: (f32, f32) = (680.0, 420.0);

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
    "finder-status",
    "finder-error",
    "finder-footer",
    "footer-move",
    "footer-tab",
    "footer-copy",
    "footer-dismiss",
    "footer-count",
];

/// The one text a [`SyntheticVault`] hands over for a `secret-*` id, and the string every hygiene
/// assertion in `tests/finder_copy.rs` searches the tree, the state and the `Debug` line for. Made-up
/// on purpose (AGENTS.md): a real value must never be the thing a leak test looks for, because the
/// test file would then be the leak.
pub const SYNTHETIC_SECRET_VALUE: &str = "synthetic-vault-value-never-displayed";

/// The draft a `command-*` id resolves to: what the palette copies for a command row.
pub const SYNTHETIC_COMMAND_DRAFT: &str = "git status --short";

/// Every key the Library's tree is expected to carry, checked against the live tree rather than
/// against a hand-picked few, so a new element cannot opt out of the surface checks by not being
/// thought of.
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

/// Keys the shared dialog's tree carries while it is open, in focus order. The scrim is first
/// because it is the modal host - the element `El::modal` puts on the tree - and the panel is its
/// child; the last two are the only Tab stops, in the order the shipped footer renders them.
pub const DIALOG_KEYS: &[&str] = &[
    "dialog-scrim",
    "dialog",
    "dialog-title",
    "dialog-body",
    "dialog-footer",
    "dialog-cancel",
    "dialog-confirm",
];

/// A runtime driven without a display, with the assertions the suites share.
pub struct Stage<S: 'static> {
    runtime: Runtime<S>,
    /// The clipboard every stage hands its runtime, in place of the OS one. See
    /// [`Stage::clipboard_holds`]: a scripted test can prove a copy landed without ever printing what
    /// it copied.
    clipboard: MemoryClipboard,
}

impl<S: 'static> Stage<S> {
    /// A stage running `view` over `state` at `viewport`, with no window-level key handler. Use
    /// [`Stage::from_spec`] for a window the shell actually opens; this is for a stage that is not a
    /// shipped window at all (the harness's own editor probe in `mod tests`).
    pub fn new(state: S, view: fn(&S, &mut Cx<S>) -> El, viewport: (f32, f32)) -> Self {
        Self::build(state, view, None, viewport)
    }

    /// A stage wired the way the window is: its view, its size and its window-level key router all
    /// come from the same [`AppSpec`] the desktop entry point hands Rune
    /// ([`dev_console_desktop::app::library_spec`] and friends).
    ///
    /// This exists so a keyboard script cannot test a lookalike. A stage that assembled its own view
    /// and handler would still pass if the shipped window asked for no router at all - and the
    /// wiring is precisely the half a scripted test otherwise assumes. Reading `spec.size` rather
    /// than a viewport constant here is the same refusal applied to geometry.
    pub fn from_spec(state: S, spec: &AppSpec<S>) -> Self {
        Self::build(state, spec.view, spec.on_key, spec.size)
    }

    /// Lays out one frame at `t = 0` over `state` and leaves it settled.
    ///
    /// The runtime is put in the keyboard state the real window has on this target, rather than in
    /// Rune's headless default: `window.rs` calls `set_full_keyboard_access` with the platform
    /// preference at startup, and `platform/preferences.rs` reports `true` on every OS but macOS.
    /// With the flag off, Rune's Tab traversal visits only text fields and list groups (it is
    /// modelling macOS's "Full Keyboard Access" setting), so a script of a button's Tab order would
    /// be measuring a macOS accommodation that the Arch target this shell ships to never has.
    fn build(
        state: S,
        view: fn(&S, &mut Cx<S>) -> El,
        on_key: Option<fn(&mut S, &KeyEvent, &mut Tx) -> bool>,
        viewport: (f32, f32),
    ) -> Self {
        let mut runtime = Runtime::new(state, view, viewport, Env::new(true, 1.0));
        runtime.on_key = on_key;
        runtime.set_full_keyboard_access(true);
        // Before the first frame, so a copy issued by a handler laid out in frame zero already has
        // somewhere to go: `Runtime::use_clipboard` swaps the OS board for an in-memory one, which on
        // this host (no `WAYLAND_DISPLAY`, no `DISPLAY`, no `wl-copy`) is the difference between
        // exercising the copy path and silently writing into the void.
        let clipboard = rune::clipboard::memory();
        runtime.use_clipboard(&clipboard);
        runtime.frame(0.0);
        Self { runtime, clipboard }
    }

    /// Whether the clipboard holds exactly `value`, without producing a copy of it.
    ///
    /// The hygiene rule in AGENTS.md ("never log, print or screenshot real secret values") applies to
    /// test failures too: `assert!(stage.clipboard_holds(VALUE))` says only that the write missed,
    /// while `assert_eq!(stage.clipboard_text(), Some(VALUE))` would print the expected value into the
    /// CI log the moment it failed. Use this wherever the assertion is about *whether*, and say so in
    /// the message.
    pub fn clipboard_holds(&self, value: &str) -> bool {
        self.clipboard
            .text()
            .is_some_and(|text| text.as_str() == value)
    }

    /// True when nothing is on the clipboard - the state a cleared or never-written board is in.
    pub fn clipboard_is_empty(&self) -> bool {
        self.clipboard.text().is_none()
    }

    /// The clipboard's text, for an assertion that has to *compare* it (a replacement, a prefix).
    /// Prefer [`Stage::clipboard_holds`] for the leak-sensitive direction: a failure message built
    /// from this value can print what a secret-copy test was holding.
    pub fn clipboard_text(&self) -> Option<String> {
        self.clipboard.text().map(|text| text.to_string())
    }

    /// Puts `value` on the clipboard the way *another application* would: a fresh write, which is how
    /// a script reaches the shipped `expire_due` guard - "clear the clipboard only when it still holds
    /// the value this app copied" - without waiting for a deadline to pass.
    pub fn set_clipboard_text(&self, value: &str) {
        self.clipboard.set_text(value);
    }

    /// The state a script drove.
    pub fn state(&self) -> &S {
        &self.runtime.state
    }

    /// The state, for setting up a case no interaction can produce - a result list that got
    /// shorter under the highlight, say, which the search port will cause and no key here can.
    /// Never a substitute for driving the UI: a script that mutates its way to an assertion has
    /// tested the fixture.
    pub fn state_mut(&mut self) -> &mut S {
        &mut self.runtime.state
    }

    /// Lays out another frame, e.g. after a press, so animations and layout settle again.
    pub fn step(&mut self, dt: f64) {
        self.runtime.frame(dt);
    }

    /// True when `key` is in the live tree right now.
    ///
    /// Deliberately not `presented`: when an element leaves the tree Rune plays its exit transition
    /// and keeps it in the frame, and `presented` reports it throughout ("exiting or not"). A script
    /// that pressed a row and wants to assert the row is *gone* must ask the live-tree question, which
    /// is what `Runtime::find` answers - it treats an exiting subtree as gone. `box_of`/`press` still
    /// read `presented`, because they operate on the frame currently laid out under the pointer.
    pub fn has(&self, key: &str) -> bool {
        self.runtime.find(key).is_some()
    }

    /// The keys from `candidates` that are present, in the order given.
    pub fn present_from(&self, candidates: &[&'static str]) -> Vec<&'static str> {
        candidates
            .iter()
            .copied()
            .filter(|key| self.has(key))
            .collect()
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

    /// Which element holds keyboard focus, by its key. `None` when nothing does - and that is the
    /// state a stage starts in, because Rune gives nothing focus until a pointer or a Tab asks.
    pub fn focused_key(&self) -> Option<String> {
        self.runtime
            .elements()
            .into_iter()
            .find(|element| element.focused)
            .and_then(|element| element.key)
    }

    /// The accessible name `key` carries, for scripts that have to read a label rather than guess
    /// one from the source - the only way to check a footer promises a binding that exists.
    pub fn label_of(&self, key: &str) -> Option<String> {
        self.runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key))
            .and_then(|element| element.label)
    }

    /// The visible text an element shows, as opposed to [`Stage::label_of`]'s accessible name. For a
    /// dialog that is the difference between the heading a user reads and the name a screen reader
    /// resolves, and a shell that promises both has to be checked on both.
    pub fn text_of(&self, key: &str) -> Option<String> {
        self.runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key))
            .and_then(|element| element.text)
    }

    /// Every accessible name under `prefix`, sorted, so a "nothing else advertises itself" check
    /// names what it looked at instead of trusting a key list.
    pub fn labels_starting_with(&self, prefix: &str) -> Vec<String> {
        let mut labels: Vec<String> = self
            .runtime
            .elements()
            .into_iter()
            .filter(|element| {
                element
                    .key
                    .as_deref()
                    .is_some_and(|key| key.starts_with(prefix))
            })
            .filter_map(|element| element.label)
            .collect();
        labels.sort();
        labels
    }

    /// Every string the *tree* carries: each element's accessible name and its visible text, in tree
    /// order. This is the surface a secret would leak through if a copy ever put one on screen - an
    /// accessible value, a rendered label, a screen reader's announcement - so the hygiene scripts
    /// scan this rather than a hand-picked key list, and a new element cannot opt out of the check by
    /// not being thought of.
    ///
    /// Deliberately not `Debug` for the whole of `ElementInfo`: these are the two fields a user can
    /// be shown. The state's own `Debug` is checked separately, and the clipboard is not in either.
    pub fn element_strings(&self) -> Vec<String> {
        self.runtime
            .elements()
            .into_iter()
            .flat_map(|element| [element.label, element.text].into_iter().flatten())
            .collect()
    }

    /// True when `needle` reaches a surface of the *window* it must never reach: an element's
    /// accessible name, its visible text, or its element key. Returns the surfaces it did reach, so a
    /// failure names the hole rather than only the needle.
    ///
    /// The clipboard is deliberately not one of these surfaces - that is where a copied value belongs,
    /// and [`Stage::clipboard_holds`] is how a script asserts it got there. A view state's `Debug` is
    /// the third surface a leak needs, and it is checked where the type is known
    /// (`tests/finder_copy.rs`), because this harness is generic over the state and cannot print a
    /// type it does not know is printable.
    pub fn leak_surfaces(&self, needle: &str) -> Vec<&'static str> {
        let mut surfaces = Vec::new();
        if self
            .element_strings()
            .iter()
            .any(|shown| shown.contains(needle))
        {
            surfaces.push("an element's accessible name or visible text");
        }
        if self.runtime.elements().iter().any(|element| {
            element
                .key
                .as_deref()
                .is_some_and(|key| key.contains(needle))
        }) {
            surfaces.push("an element key");
        }
        surfaces
    }

    /// Presses `key` where it lies and returns whether the press left it holding keyboard focus.
    /// Every key-routing script has to start here: in Rune a text field is the only thing that sees
    /// its own keys before its editor does, so a key sent to an unfocused field proves nothing.
    pub fn press_until_focused(&mut self, key: &str) -> bool {
        self.press(key) && self.focused_key().as_deref() == Some(key)
    }

    /// Injects one key event and lays out the frame that follows it, so the state the handler left
    /// and the tree rebuilt from it are both observable when this returns. `mods` are empty, which
    /// is all the bindings ported so far read; a chord test passes its own [`KeyEvent`].
    pub fn press_key(&mut self, key: Key) {
        self.send_key(KeyEvent {
            key,
            mods: Mods::default(),
        });
    }

    /// Injects a full key event, modifiers included.
    pub fn send_key(&mut self, ev: KeyEvent) {
        self.runtime.key(ev);
        self.runtime.frame(1.0 / 60.0);
    }

    /// Sends a Ctrl+`c` chord in the shape this platform's window actually reports it in.
    ///
    /// Off macOS the toolkit maps the control key onto `Mods::cmd` - the *primary* modifier - as
    /// well as `Mods::ctrl`, and the chorded character arrives as `Key::Command(c)` rather than
    /// `Key::Text` (`window/keys.rs::map_mods`/`map_key`). A script that invented its own shape
    /// would be handing the router a key no window ever sends, and a chord matcher could be wrong
    /// and still pass.
    pub fn press_ctrl(&mut self, c: char) {
        self.send_key(KeyEvent {
            key: Key::Command(c),
            mods: Mods {
                cmd: true,
                ctrl: true,
                ..Mods::default()
            },
        });
    }

    /// Injects Tab, optionally Shift+Tab - the pair a focus trap is defined by, spelled out here so
    /// a script cannot accidentally test the forward direction twice.
    pub fn press_tab(&mut self, shift: bool) {
        self.send_key(KeyEvent {
            key: Key::Tab,
            mods: Mods {
                shift,
                ..Mods::default()
            },
        });
    }

    /// Types literal text the way a confirmed keystroke arrives: one `Key::Text` event, which is
    /// what a field inserts and a routing handler has to fall through to.
    pub fn type_text(&mut self, text: &str) {
        self.press_key(Key::Text(text.to_owned()));
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
    /// The Library shell with `state`, wired from the same [`AppSpec`] the resident process builds
    /// its primary window from - view, size and window-level key router included.
    pub fn library(state: Library) -> Stage<Library> {
        Stage::from_spec(state, &library_spec())
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

/// A copy source over two made-up entries, so a scripted test can press Enter on a row whose value it
/// knows and can then look for that value in the places a value must never appear.
///
/// This is the seam the real shell will fill with `Arc<AppState>` (plan milestone 2): the one
/// component allowed to turn an entry id into plaintext, and the reason no *state* field has to be
/// trusted with one. Like every fixture here the contents are synthetic - AGENTS.md's rule is about
/// real values, and a leak test written against a real secret would be the leak.
#[derive(Debug, Default)]
pub struct SyntheticVault;

impl SyntheticVault {
    /// A vault that resolves the two ids [`secret_candidate`] and [`command_candidate`] hand out.
    pub fn new() -> SyntheticVault {
        SyntheticVault
    }

    /// Wraps a Finder built from [`command_candidate`] and [`secret_candidate`] in one step, which is
    /// every copy script's starting state.
    pub fn finder() -> Finder {
        Finder::with_candidates(&[command_candidate(), secret_candidate()])
            .with_copy_source(std::sync::Arc::new(SyntheticVault::new()))
    }
}

impl CopySource for SyntheticVault {
    fn value_for(&self, request: &CopyRequest) -> Option<String> {
        match request.id.as_str() {
            "synthetic-command-1" => Some(SYNTHETIC_COMMAND_DRAFT.to_owned()),
            "synthetic-secret-1" => Some(SYNTHETIC_SECRET_VALUE.to_owned()),
            // Anything else - a deleted row, a locked vault, an id this fixture never handed out -
            // resolves to nothing, which is the failure the shipped palette reports as
            // "Copy failed. Nothing was copied."
            _ => None,
        }
    }
}

/// The command row: `name` is what the palette prints, `id` is what the copy asks for.
pub fn command_candidate() -> Candidate {
    Candidate::command("synthetic-command-1", "synthetic git status")
}

/// The secret row. Its name is metadata; the value behind `synthetic-secret-1` is what every hygiene
/// assertion below searches the tree, the state and the `Debug` output for.
pub fn secret_candidate() -> Candidate {
    Candidate::secret("synthetic-secret-1", "synthetic deploy token")
}

/// A Library with rows, so the list and the empty state can both be exercised.
pub fn library_with_entries() -> Library {
    Library {
        entries: vec![
            "synthetic command one".into(),
            "synthetic command two".into(),
        ],
        vault_unlocked: true,
        ..Library::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the scripts below build a view of their own; the harness's job is to drive the shell's.
    use rune::{col, text_input, TextEdit, TextStyle};

    #[test]
    fn a_stage_lays_out_the_finder_before_any_script_runs() {
        let stage = Stage::finder(finder_with_rows());
        assert!(
            stage.has("finder-window"),
            "the first frame must present the tree"
        );
        assert_eq!(
            // The whole surface a Finder that has *not* just copied something presents, in tree order.
            // `finder-status`/`finder-error` are absent by construction - the copy line is an unkeyed
            // spacer until there is an outcome to report - and `footer-copy` is present because `Enter`
            // is bound. A new footer chip or a status line that draws itself when idle fails here,
            // which is the point of listing the surface rather than checking a handful of keys.
            stage.present_from(FINDER_KEYS),
            vec![
                "finder-window",
                "finder-query-row",
                "finder-query",
                "finder-context",
                "finder-rows",
                "finder-footer",
                "footer-move",
                "footer-tab",
                "footer-copy",
                "footer-dismiss",
                "footer-count"
            ]
        );
    }

    #[test]
    fn pressing_a_key_that_is_not_there_presses_nothing() {
        let mut stage = Stage::finder(Finder::default());
        assert!(
            !stage.press("finder-row-0"),
            "an empty Finder has no rows to press"
        );
        assert_eq!(stage.state().selected, 0);
    }

    #[test]
    fn fixtures_hold_synthetic_text_only() {
        // The hygiene rule in AGENTS.md is about real values; the fixtures must be obviously synthetic
        // so a captured tree can never be mistaken for user data.
        let finder = finder_with_rows();
        assert_eq!(finder.context, "synthetic-project");
        let library = library_with_entries();
        assert!(library
            .entries
            .iter()
            .all(|entry| entry.starts_with("synthetic")));
        assert!(SYNTHETIC_QUERY.starts_with("synthetic"));
    }

    /// A bare field with no routing at all, for the two claims every keyboard script leans on and
    /// which no Finder-level assertion can establish by itself: that this harness can tell which
    /// element holds focus, and that an injected key really drives Rune's editor. Without the
    /// second, "the routing handler consumed the arrow" would pass just as happily when the field
    /// never had the key in the first place.
    #[derive(Debug)]
    struct Bare {
        field: TextEdit,
    }

    fn bare_view(state: &Bare, _cx: &mut Cx<Bare>) -> El {
        col().key("bare-window").size(240.0, 120.0).child(
            text_input(&state.field, TextStyle::new(14.0, 400.0))
                .key("bare-field")
                .size(200.0, 30.0)
                .on_edit(|state: &mut Bare| &mut state.field),
        )
    }

    fn bare() -> Bare {
        Bare {
            field: TextEdit::new(String::new()),
        }
    }

    #[test]
    fn focus_starts_empty_and_a_press_puts_it_in_the_field() {
        let mut stage = Stage::new(bare(), bare_view, (240.0, 120.0));
        assert_eq!(
            stage.focused_key(),
            None,
            "nothing holds focus until a pointer or Tab asks, so a script that skips the press is \
             testing the window rather than the field"
        );
        assert!(
            stage.press_until_focused("bare-field"),
            "a press inside a text field is what focuses it"
        );
    }

    #[test]
    fn an_injected_key_drives_the_real_editor() {
        let mut stage = Stage::new(bare(), bare_view, (240.0, 120.0));
        assert!(stage.press_until_focused("bare-field"));
        stage.type_text("synthetic-token");
        assert_eq!(
            stage.state().field.text,
            "synthetic-token",
            "a Key::Text event has to reach the field's own state, or every routing script below \
             is asserting on a key that never arrived"
        );
        stage.press_key(Key::Backspace);
        assert_eq!(
            stage.state().field.text,
            "synthetic-toke",
            "a key nothing routes has to fall through to the editor: this is what makes \
             \"the handler consumed it\" a claim rather than a tautology"
        );
    }
}
