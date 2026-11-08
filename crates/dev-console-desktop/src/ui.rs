//! Explicit UI state and the first view functions for the two windows.
//!
//! Plan §2 asks for each screen to be "a Rust view function that produces Rune elements from
//! explicit UI state". These are the first cut of that shape for the Library and the Finder, and
//! they are deliberately thin: they establish the state/element split, the element keys the
//! scripted tests in `dev-console-ui-tests` address, the token-only colour rule, and where real
//! data will come from.
//!
//! ## What is not here yet, and where it is specified
//!
//! Plan milestone 4 ("Port all feature screens") fills these in, with data from the services that
//! milestone 2 moves into `dev-console-core`: the repository and search ranking behind
//! [`Library::entries`], the vault state behind the header, the project scanners behind
//! [`Finder::context`], and the ports of the draft validation, argument parsing, formatting and
//! Finder selection logic now living in `src/lib/*.ts`. Keyboard routing (Escape, arrow selection,
//! Ctrl+1/Ctrl+2), the focus trap, completion behaviour, output panels and the SVG icon set are all
//! milestone 4 work against a real renderer; none of them can be *accepted* headlessly, so they are
//! not invented here.
//!
//! ## Colour rule
//!
//! Every colour comes from `rune::tok::theme` (and spacing/radius/type from `space`/`radius`/
//! `type_`). AGENTS.md forbids hard-coded colours in view code. The real palette is the CSS token
//! set in `src/styles/tokens.css` plus the 22 Omarchy palettes, and plan §2/§5 ("Port the contrast
//! checks to the new theme data") makes that mapping a checkpoint of its own; `rune::tok::theme` is
//! Rune's built-in default, not Dev Console's theme, and is used here only so these views draw
//! something without inventing a parallel colour source.

use rune::tok::{radius, space, type_};
use rune::{Cx, El, TextEdit, col, el, row, text, text_input};

/// Which list the Library is showing. Mirrors the Ctrl+1 / Ctrl+2 tabs of the current app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// The Commands tab, shown by default.
    #[default]
    Commands,
    /// The Secrets tab. Its rows are metadata only: values are revealed through a separate action.
    Secrets,
}

impl Tab {
    /// The tab's label as the footer and the tabs row show it.
    pub const fn label(self) -> &'static str {
        match self {
            Tab::Commands => "Commands",
            Tab::Secrets => "Secrets",
        }
    }
}

/// The Library window's state.
#[derive(Debug, Clone, Default)]
pub struct Library {
    /// Commands or Secrets.
    pub tab: Tab,
    /// Placeholder for the ranked, grouped rows `src/lib` builds from the repository; empty means
    /// the empty state is shown. Synthetic strings only - never entry content from a real profile.
    pub entries: Vec<String>,
    /// True when the vault is unlocked. Only the *status* crosses into the UI; the key and any
    /// secret value never reach this struct.
    pub vault_unlocked: bool,
}

/// The Finder window's state.
#[derive(Debug)]
pub struct Finder {
    /// The query field's editable text.
    pub query: TextEdit,
    /// Completion candidates, best first. Placeholder for the ported Finder selection logic.
    pub rows: Vec<String>,
    /// Index of the highlighted row.
    pub selected: usize,
    /// The project folder the Finder detected from the focused window, shown as a chip. Empty
    /// means no context was detected. A path only - never file contents.
    pub context: String,
}

impl Default for Finder {
    fn default() -> Self {
        Finder { query: TextEdit::new(String::new()), rows: Vec::new(), selected: 0, context: String::new() }
    }
}

impl Finder {
    /// A Finder showing `rows`, for tests and for the headless snapshot scenes.
    pub fn with_rows(rows: &[&str]) -> Self {
        Finder { rows: rows.iter().map(|row| (*row).to_owned()).collect(), ..Finder::default() }
    }

    /// Highlights `index` when it names a row, and says whether anything moved. The scripted tests
    /// drive selection through this rather than through a private field, so the rule "a press on a
    /// row that is not in the list cannot select it" is one tested place.
    pub fn select(&mut self, index: usize) -> bool {
        if index < self.rows.len() && self.selected != index {
            self.selected = index;
            return true;
        }
        false
    }
}

/// Draws the Library window.
pub fn library_view(state: &Library, cx: &mut Cx<Library>) -> El {
    let theme = rune::tok::theme(cx.system_dark());
    col()
        .key("library-window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .px(space::S6)
        .py(space::S6)
        .gap(space::S3)
        .child(header(state, theme))
        .child(tabs(state))
        .child(entry_list(state, theme))
        .child(footer(state, theme))
}

fn header(state: &Library, theme: rune::tok::Theme) -> El {
    row()
        .key("library-header")
        .w_full()
        .gap(space::S3)
        .center_y()
        .child(text("Dev Console").key("library-title").font(type_::TITLE).color(theme.text))
        .child(
            row()
                .key("vault-status")
                .h(22.0)
                .px(space::S2)
                .radius(radius::SM)
                .center_y()
                .fill(theme.card)
                .border(1.0, theme.card_border)
                .child(
                    text(if state.vault_unlocked { "Vault unlocked" } else { "Vault locked" })
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
}

fn tabs(state: &Library) -> El {
    row().key("library-tabs").gap(space::S2).child(tab(Tab::Commands, state.tab)).child(tab(Tab::Secrets, state.tab))
}

fn tab(tab: Tab, current: Tab) -> El {
    let selected = tab == current;
    row()
        .key(format!("tab-{}", tab.label()))
        .role("tab")
        .label(tab.label())
        // `rune` at this revision exposes no `selected` state on an element (`el/a11y.rs` offers
        // `checked`, `position_in_set`, `a11y_value`, `active_descendant`). The visible difference
        // is the fill; whether that is enough for the tabs' accessible selection is a milestone-4
        // decision to take against a screen reader, not a call to make headlessly.
        .position_in_set(if selected { 1 } else { 2 }, 2)
        .focusable(true)
        .h(28.0)
        .px(space::S3)
        .center_y()
        .radius(radius::MD)
        .on_press_tx(move |state: &mut Library, _| state.tab = tab)
        .child(text(tab.label()).font(type_::LABEL))
}

fn entry_list(state: &Library, theme: rune::tok::Theme) -> El {
    if state.entries.is_empty() {
        return col()
            .key("empty-state")
            .w_full()
            .h_full()
            .center()
            .gap(space::S2)
            .child(text(if state.tab == Tab::Secrets { "No secrets yet" } else { "No commands yet" }).font(type_::TITLE).color(theme.text))
            .child(text("Add one with the editor, or import tasks from a project folder.").font(type_::BODY).color(theme.text_secondary));
    }

    col()
        .key("entry-list")
        .role("list")
        .label(state.tab.label())
        .w_full()
        .h_full()
        .gap(space::S1)
        .children(state.entries.iter().enumerate().map(|(index, entry)| {
            row()
                .key(format!("entry-{index}"))
                .role("listitem")
                .label(entry.clone())
                .focusable(true)
                .h(28.0)
                .px(space::S2)
                .center_y()
                .radius(radius::MD)
                .on_press_tx(move |state: &mut Library, _| {
                    // Placeholder for opening the entry in the inspector; the milestone-4
                    // screen owns that. Removing the row keeps the press wired to a handler the
                    // scripted test can observe, instead of pretending the inspector exists.
                    state.entries.remove(index);
                })
                .child(text(entry.clone()).font(type_::BODY))
        }))
}

fn footer(state: &Library, theme: rune::tok::Theme) -> El {
    row()
        .key("library-footer")
        .w_full()
        .gap(space::S3)
        .center_y()
        .child(
            text(format!("{} {}", state.entries.len(), if state.tab == Tab::Secrets { "secrets" } else { "commands" }))
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(keycap("footer-hint", "Ctrl+1 / Ctrl+2 to switch tabs", theme))
}

/// Draws the Finder window: query field, context chip, candidates and the keyboard footer.
pub fn finder_view(state: &Finder, cx: &mut Cx<Finder>) -> El {
    let theme = rune::tok::theme(cx.system_dark());
    col()
        .key("finder-window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .px(space::S3)
        .py(space::S3)
        .gap(space::S2)
        .child(query_field(state, theme))
        .child(context_chip(state, theme))
        .child(result_rows(state, theme))
        .child(finder_footer(state, theme))
}

fn query_field(state: &Finder, theme: rune::tok::Theme) -> El {
    row()
        .key("finder-query-row")
        .w_full()
        .h(34.0)
        .px(space::S2)
        .center_y()
        .radius(radius::MD)
        .fill(theme.card)
        .border(1.0, theme.card_border)
        .child(
            text_input(&state.query, type_::BODY)
                .key("finder-query")
                .role("textbox")
                .label("Search commands and secrets")
                .color(theme.text)
                .on_edit(|state: &mut Finder| &mut state.query),
        )
}

fn context_chip(state: &Finder, theme: rune::tok::Theme) -> El {
    if state.context.is_empty() {
        return el().key("finder-context").size(0.0, 0.0);
    }
    row()
        .key("finder-context")
        .h(22.0)
        .px(space::S2)
        .center_y()
        .radius(radius::SM)
        .fill(theme.card)
        .border(1.0, theme.card_border)
        .label(format!("Project {}", state.context))
        .child(text(state.context.clone()).font(type_::MONO_CAPTION).color(theme.text_secondary))
}

fn result_rows(state: &Finder, theme: rune::tok::Theme) -> El {
    if state.rows.is_empty() {
        return col()
            .key("finder-empty")
            .w_full()
            .h_full()
            .center()
            .child(text("Type to search").font(type_::BODY).color(theme.text_secondary));
    }
    col()
        .key("finder-rows")
        .role("list")
        .label("Results")
        .w_full()
        .h_full()
        .gap(space::S1)
        .children(state.rows.iter().enumerate().map(|(index, row_text)| {
            let selected = index == state.selected;
            row()
                .key(format!("finder-row-{index}"))
                .role("listitem")
                .label(row_text.clone())
                // No `selected` state exists on `El` at this revision (see `tab`); the row's place
                // in the list is announced and the highlight is drawn.
                .position_in_set(index + 1, state.rows.len())
                .focusable(true)
                .h(28.0)
                .px(space::S2)
                .center_y()
                .radius(radius::MD)
                .fill(if selected { theme.hover } else { theme.bg })
                .on_press_tx(move |state: &mut Finder, _| {
                    state.select(index);
                })
                .child(text(row_text.clone()).font(type_::BODY).color(if selected { theme.text } else { theme.text_secondary }))
        }))
}

fn finder_footer(state: &Finder, theme: rune::tok::Theme) -> El {
    row()
        .key("finder-footer")
        .w_full()
        .gap(space::S2)
        .center_y()
        .child(keycap("footer-up", "↑↓", theme))
        .child(keycap("footer-open", "↵ open", theme))
        .child(keycap("footer-copy", "⌥⌘C copy", theme))
        .child(keycap("footer-dismiss", "esc", theme))
        .child(text(format!("{} of {}", state.selected + 1, state.rows.len())).font(type_::LABEL).color(theme.text_secondary))
}

/// A keyboard hint chip. `Keycap` in the current UI; the milestone-4 pass adds the real glyph set.
fn keycap(key: &'static str, label: &str, theme: rune::tok::Theme) -> El {
    row()
        .key(key)
        .h(18.0)
        .px(space::S1)
        .center_y()
        .radius(radius::SM)
        .fill(theme.control_off)
        .label(label.to_owned())
        .child(text(label.to_owned()).font(type_::MONO_CAPTION).color(theme.text_secondary))
}
