//! The shared dialog shell: one panel, one Escape, one focus trap, for every confirmation
//! the native shell has to show.
//!
//! The shipped app has one `Dialog.svelte` and eleven call sites that pass a title, a
//! `label` (its accessible name), a paragraph and footer buttons into it - the delete
//! confirmation, the run confirmation, the command-output panel, the unsaved-changes prompt,
//! the project-import review, the restore review, the settings error. This module is the same
//! idea with the same division of labour: a screen owns *what* is being confirmed ([`Dialog`],
//! built from its own state) and this file owns *how* the shell behaves while it is open.
//!
//! ## Where each shipped behaviour lives
//!
//! `Dialog.svelte` implements its window-level behaviour by hand: `showModal()`, a capture-phase
//! `keydown` that wraps Tab between the first and last control, an Escape branch calling
//! `oncancel`, a click branch that cancels when the pointer lands outside the panel, an
//! initial-focus request for the first focusable control, and a restore to the element that had
//! focus on mount - `if (trigger?.isConnected) trigger.focus()`, i.e. only if it is still there.
//!
//! Rune's [`El::modal`] is all of that in one seam: it traps Tab inside the modal, makes
//! everything outside it inert, dismisses on Escape (and, with `click_outside`, on a press off the
//! panel), moves focus to the first stop in the modal when it mounts, and restores focus to
//! whatever held it before - dropping the restore when that element has left the tree, which is
//! the same `isConnected` guard. So there is no hand-written trap here to get wrong, and the
//! focus rules are asserted against Rune's real traversal in
//! `dev-console-ui-tests/tests/dialog.rs` rather than assumed.
//!
//! ## What is deliberately not here
//!
//! The shipped component also takes arbitrary `children`, a `wide` (780px) variant and a
//! `labelledBy` id reference. The shell has no dialog that needs them yet - every one of its
//! confirmations is a title, one paragraph and two actions - so they are absent rather than
//! speculative. Adding a `wide` panel or a form body is a change to this file plus a test, not a
//! per-screen copy of the trap.

use rune::tok::{radius, space, type_};
use rune::{col, el, row, text, El};

/// The panel's width, the shipped `.dialog { width: min(520px, 100%) }`.
pub const WIDTH: f32 = 520.0;

/// The height of the footer's buttons, matching the shell's other controls.
const CONTROL_HEIGHT: f32 = 28.0;

/// What a screen is asking the user to confirm.
///
/// The fields mirror `Dialog.svelte`'s props: `title` is the visible heading, `label` the
/// accessible name shown when the two differ (and the one an accessibility query resolves the
/// dialog by), `body` the single paragraph, and `cancel`/`confirm` the two footer actions.
///
/// Every field is a heading, a category word and two button labels. Nothing here can hold a
/// record's contents, which is the point: a confirmation is built from the metadata a row
/// already shows, so the dialog cannot be the place a secret value reaches an element tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    /// The visible heading, e.g. `Delete “git status”?`.
    pub title: String,
    /// The accessible name: the shipped `label` prop, e.g. `Confirm delete`.
    pub label: &'static str,
    /// The one paragraph under the heading.
    pub body: String,
    /// The dismissive action's label. The shipped footer always offers `Cancel` first.
    pub cancel: &'static str,
    /// The action that goes ahead with it.
    pub confirm: &'static str,
}

impl Dialog {
    /// The delete confirmation `src/windows/Library.svelte` holds as `pendingDelete`: the record
    /// named in the heading, its kind in the sentence, `Cancel` and `Delete` as the actions.
    ///
    /// `name` is the row's metadata name, never a secret value, and the wording is the shipped
    /// paragraph verbatim - including the promise about encrypted backups, which is true of the
    /// ported archive service even while no view reads it.
    pub fn delete(name: &str, kind: &'static str) -> Dialog {
        Dialog {
            title: format!("Delete \u{201c}{name}\u{201d}?"),
            label: "Confirm delete",
            body: format!(
                "This removes the saved {kind} permanently. Encrypted backups you made earlier \
                 still hold it."
            ),
            cancel: "Cancel",
            confirm: "Delete",
        }
    }
}

/// Draws `spec` over its host's content and takes the window's keyboard and pointer focus.
///
/// Generic over the host's state because the shell has several screens that will confirm things
/// and one trap to share. `on_cancel` is the shipped `oncancel` and fires from three places - the
/// Cancel button, Escape, and a press outside the panel - which is why the two handlers are
/// function pointers rather than captures: the same one has to be given to [`El::modal`] and to a
/// button.
///
/// The panel is a child of a full-window scrim (`theme.shadow`, the token the shell already uses
/// for the dark of an elevation), centred in it, so a press on the scrim is a press off the panel
/// and the modal treats it as a cancel.
pub fn confirm<S: 'static>(
    spec: &Dialog,
    theme: rune::tok::Theme,
    on_cancel: fn(&mut S),
    on_confirm: fn(&mut S),
) -> El {
    col()
        .key("dialog-scrim")
        // Pinned to all four edges of the host's padding box, so the panel inside it can be centred
        // and the dim covers the screen. It is deliberately *not* the modal: `El::modal` marks
        // everything before the modal in tree order inert and dismisses on a press outside it, so the
        // element that carries it has to be the panel. Put it on the scrim instead and a press on the
        // dim is a press *inside* the modal, which is the opposite of the shipped component's
        // `event.target !== panel` branch - a scripted press off the panel then does nothing at all.
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .fill(theme.shadow)
        .center()
        .child(
            col()
                .key("dialog")
                .role("dialog")
                // The modal: focus arrives here when this subtree appears, Tab stays inside it, the
                // rest of the window is inert, Escape and any press off it call `on_cancel`, and the
                // element that had focus before gets it back when it goes away.
                .modal(true, on_cancel)
                .label(spec.label)
                .w(WIDTH)
                .max_w(WIDTH)
                .px(space::S6)
                .py(space::S6)
                .gap(space::S3)
                .radius(radius::LG)
                .fill(theme.card)
                .border(1.0, theme.card_border)
                .child(
                    text(spec.title.clone())
                        .key("dialog-title")
                        .font(type_::TITLE)
                        .color(theme.text),
                )
                .child(
                    text(spec.body.clone())
                        .key("dialog-body")
                        .font(type_::BODY)
                        .color(theme.text_secondary),
                )
                .child(footer(spec, theme, on_cancel, on_confirm)),
        )
}

/// The footer: the shipped `.foot` is `justify-content: flex-end` behind a spacer, so the actions
/// sit at the trailing edge in the order Cancel, then the action that commits.
fn footer<S: 'static>(
    spec: &Dialog,
    theme: rune::tok::Theme,
    on_cancel: fn(&mut S),
    on_confirm: fn(&mut S),
) -> El {
    row()
        .key("dialog-footer")
        .w_full()
        .gap(space::S2)
        .center_y()
        // The shipped spacer span: `grow` is Rune's way of pushing the actions apart edge.
        .child(el().grow(1.0))
        .child(action(
            "dialog-cancel",
            spec.cancel,
            theme.control_off,
            theme.text,
            on_cancel,
        ))
        // The destructive variant wants the palette's danger colour, which is the `theme-switch`
        // hop this shell has not made yet, so the commit takes the one accent token available and
        // the row in the matrix that covers the palette's colours stays `NOT COVERED`.
        .child(action(
            "dialog-confirm",
            spec.confirm,
            theme.indicator,
            theme.knob,
            on_confirm,
        ))
}

/// One footer action: a focusable button, labelled with its own text so the accessible name and
/// the visible name are the same word.
fn action<S: 'static>(
    key: &'static str,
    label: &'static str,
    fill: rune::Color,
    colour: rune::Color,
    on_press: fn(&mut S),
) -> El {
    row()
        .key(key)
        .role("button")
        .label(label)
        .focusable(true)
        .h(CONTROL_HEIGHT)
        .px(space::S3)
        .center_y()
        .radius(radius::MD)
        .fill(fill)
        .on_press(on_press)
        .child(text(label).font(type_::LABEL).color(colour))
}
