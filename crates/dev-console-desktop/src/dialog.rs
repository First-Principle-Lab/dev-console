//! The shared dialog shell: one panel, one Escape, one focus trap, for every confirmation
//! the native shell has to show.
//!
//! The shipped app has one `Dialog.svelte` and eleven call sites that pass a title, a `label` (its
//! accessible name), a paragraph and footer buttons into it - the delete confirmation, the run
//! confirmation, the command-output panel, the unsaved-changes prompt, the project-import review,
//! the restore review, the settings error. This module is the same idea with the same division of
//! labour: a screen owns *what* is being confirmed ([`Dialog`], built from its own state) and this
//! file owns *how* the shell behaves while it is open.
//!
//! ## What "shared" has to mean before the word can be used
//!
//! One call site does not make a shell - it makes a delete dialog with a helper function beside it.
//! The claim is only real when a second screen of *different shape* goes through the same code
//! without copying any of it, and the shell's second user is the editor's unsaved-changes prompt
//! ([`Dialog::unsaved_changes`]), which differs from the first in every way a second call site can
//! differ: the dismissive route returns to the screen underneath instead of refusing an action, the
//! panel carries a **third** footer action (`Discard`) so the Tab order and the inert background are
//! not the delete dialog's two buttons relabelled, and the heading, the sentence and all three labels
//! differ. Everything about the *behaviour* - where focus goes when a panel appears, which controls
//! Tab may visit, what Escape and a press off the panel mean, who gets focus back - is written once,
//! here, and reached from two screens. The scripted suites assert the same four rules against each of
//! them: `dev-console-ui-tests/tests/dialog.rs` for the delete confirmation, `tests/editor_draft.rs`
//! for the prompt. That is what makes "shared" a checked claim rather than a word - a second copy of
//! the panel would have to be written by hand, and would then fail, one by one, the assertions the
//! first copy already passes.
//!
//! ## Where each shipped behaviour lives
//!
//! `Dialog.svelte` implements its window-level behaviour by hand: `showModal()`, a capture-phase
//! `keydown` that wraps Tab between the first and last control, an Escape branch calling `oncancel`,
//! a click branch that cancels when the pointer lands outside the panel, an initial-focus request for
//! the first focusable control, and a restore to the element that had focus on mount -
//! `if (trigger?.isConnected) trigger.focus()`, i.e. only if it is still there.
//!
//! Rune's [`El::modal`] is all of that in one seam: it traps Tab inside the modal, makes everything
//! outside it inert, dismisses on Escape (and, with `click_outside`, on a press off the panel), moves
//! focus to the first stop in the modal when it mounts, and restores focus to whatever held it
//! before, dropping the restore when that element has left the tree - the same `isConnected` guard.
//! So there is no hand-written trap here to get wrong, and the focus rules are asserted against
//! Rune's real traversal in `dev-console-ui-tests/tests/dialog.rs` rather than assumed.
//!
//! ## What is deliberately not here
//!
//! The shipped component also takes arbitrary `children`, a `wide` (780px) variant and a `labelledBy`
//! id reference. No dialog the shell has built needs them yet - each is a heading, one paragraph and
//! two or three actions - so they are absent rather than speculative. Adding a `wide` panel or a form
//! body is a change to this file plus a test, not a per-screen copy of the trap.

use rune::tok::{radius, space, type_};
use rune::{col, el, row, text, El};

/// The panel's width, the shipped `.dialog { width: min(520px, 100%) }`.
pub const WIDTH: f32 = 520.0;

/// The height of the footer's buttons, matching the shell's other controls.
const CONTROL_HEIGHT: f32 = 28.0;

/// What a screen is asking the user to confirm.
///
/// The fields mirror `Dialog.svelte`'s props: `title` is the visible heading, `label` the accessible
/// name shown when the two differ (and the one an accessibility query resolves the dialog by), `body`
/// the single paragraph, and `cancel`/`discard`/`confirm` the footer actions, the middle one only for
/// a screen that has a route for it.
///
/// Every field is a heading, a category word and two or three button labels. Nothing here can hold a
/// record's contents, which is the point: a confirmation is built from the metadata a row already
/// shows, so the dialog cannot be the place a value reaches an element tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    /// The visible heading, e.g. `Delete “git status”?`.
    pub title: String,
    /// The accessible name: the shipped `label` prop, e.g. `Confirm delete`.
    pub label: &'static str,
    /// The one paragraph under the heading.
    pub body: String,
    /// The dismissive action's label. The shipped footer always offers it first, and `oncancel` -
    /// Escape, a press off the panel, and this button - is one route with three owners.
    pub cancel: &'static str,
    /// The middle action, when the screen has one: the unsaved-changes prompt's `Discard`. It is data
    /// rather than a second shell because it changes what the shell has to get right - the panel has
    /// three Tab stops instead of two, and the middle one is neither the dismissive nor the committing
    /// route. `None` is the two-button footer every other confirmation uses.
    pub discard: Option<&'static str>,
    /// The action that goes ahead with it.
    pub confirm: &'static str,
}

impl Dialog {
    /// The delete confirmation `src/windows/Library.svelte` holds as `pendingDelete`: the record named
    /// in the heading, its kind in the sentence, `Cancel` and `Delete` as the actions.
    ///
    /// `name` is the row's metadata name, never a secret value, and the wording is the shipped
    /// paragraph verbatim - including the promise about encrypted backups, which is true of the ported
    /// archive service even while no view reads it.
    pub fn delete(name: &str, kind: &'static str) -> Dialog {
        Dialog {
            title: format!("Delete \u{201c}{name}\u{201d}?"),
            label: "Confirm delete",
            body: format!(
                "This removes the saved {kind} permanently. Encrypted backups you made earlier still \
                 hold it."
            ),
            cancel: "Cancel",
            discard: None,
            confirm: "Delete",
        }
    }

    /// The unsaved-changes prompt `src/windows/Library.svelte` holds as `pendingClose`: a fixed
    /// heading, a fixed sentence, and `Keep editing` / `Discard` / `Save`.
    ///
    /// Nothing about it depends on which record is being edited - the shipped prompt names no entry
    /// either - so there is no `name` parameter, and for the same reason there is no surface for a
    /// draft's *content* to reach. It is also why the panel takes a third Tab stop: the shipped
    /// footer's middle button, `Discard`, is neither the dismissive route nor the committing one, and
    /// a shell that could draw only two buttons would have had to grow a second panel to host this
    /// screen.
    pub fn unsaved_changes() -> Dialog {
        Dialog {
            title: "Unsaved changes".into(),
            label: "Unsaved changes",
            body: "This entry has edits that are not saved.".into(),
            cancel: "Keep editing",
            discard: Some("Discard"),
            confirm: "Save",
        }
    }
}

/// Whether the panel is the window's modal - the screen the keyboard belongs to.
///
/// The shell has one trap, and a window can honour only one: [`El::modal`] marks everything *before*
/// the modal in tree order inert, so when two of this shell's panels are on screen at once the host
/// has to say which one owns the keyboard. It is the one drawn last; the other is [`Modal::Under`] -
/// still drawn, still clickable, but not where Tab or Escape go. See `library_view` in `ui.rs` for
/// the one combination of screens that reaches this and what it costs there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    /// The panel owns the window's keyboard: focus arrives on its first action, Tab stays inside it,
    /// Escape and a press off the panel both take the dismissive route, and everything else in the
    /// window is inert until it goes away.
    Trap,
    /// The panel is on screen and its actions still fire, but the keyboard belongs to another panel.
    Under,
}

/// What the footer's actions mean for the screen that drew the panel.
///
/// `Dialog` is data - a heading, a sentence, three labels - and a screen is what knows whether
/// `Discard` means "drop the draft" or has no meaning at all. The three routes are therefore passed in
/// rather than derived from the wording, and `discard` is an `Option` because that is the honest type
/// for a route the delete confirmation does not have: nothing here can send `Discard` to a screen that
/// would have to invent a meaning for it.
///
/// Function pointers rather than closures, because `cancel` is reached from two places - its button,
/// and the modal's Escape and click-off-panel handling - and two owners of one route are exactly what a
/// pair of captured closures would let drift apart.
pub struct Actions<S> {
    /// The dismissive route: the Cancel / Keep editing button, Escape, and a press off the panel.
    pub cancel: fn(&mut S),
    /// The middle route, for the screens whose footer has a third button.
    pub discard: Option<fn(&mut S)>,
    /// The route that goes ahead with it.
    pub confirm: fn(&mut S),
}

/// Draws `spec` as the shell's panel: one scrim, one heading, one sentence, one footer, and - when
/// `modal` says so - one focus trap.
///
/// Generic over the host's state because the shell has several screens that will confirm things and one
/// trap to share. This is the whole of the panel: there is no second copy for the next screen to use,
/// and a screen that needs a fourth action would have to change *this* function rather than fork it,
/// which is the bargain the module doc makes about the word "shared".
///
/// `id` is the panel's element key and the prefix of its parts (`id`, `id-title`, `id-cancel`, ...).
/// It is a parameter for the same reason the modal flag is: Rune addresses elements by key, so two
/// panels drawn in one tree under the same keys would leave one of them unreachable to a pointer or a
/// script. The delete confirmation is `"dialog"` - the keys the shipped spec resolves - and the
/// unsaved-changes prompt is `"prompt"`. Nothing else about the two differs.
///
/// The panel is a child of a full-window scrim (`theme.shadow`, the token the shell already uses for
/// the dark of an elevation), centred in it, so a press on the scrim is a press off the panel. That
/// scrim is deliberately *not* the modal: [`El::modal`] marks everything before the modal in tree order
/// inert and dismisses on a press outside it, so the element carrying it has to be the panel. Put it on
/// the scrim instead and a press on the dim is a press *inside* the modal, which is the opposite of the
/// shipped component's `event.target !== panel` branch - a scripted press off the panel then does
/// nothing at all. That is not a style note; it is why the trap sits where it sits, and
/// `pressing_off_the_unsaved_changes_panel_keeps_the_edit` is the assertion that keeps it true.
pub fn panel<S: 'static>(
    spec: &Dialog,
    theme: rune::tok::Theme,
    modal: Modal,
    id: &'static str,
    actions: Actions<S>,
) -> El {
    let Actions {
        cancel,
        discard: _,
        confirm: _,
    } = &actions;
    let cancel = *cancel;
    let part = |suffix: &str| format!("{id}-{suffix}");
    let footer = footer(spec, theme, id, &actions);
    let panel = col()
        .key(id)
        .role("dialog")
        .label(spec.label)
        .w(WIDTH)
        .max_w(WIDTH)
        .px(space::S6)
        .py(space::S6)
        .gap(space::S3)
        .radius(radius::LG)
        .fill(theme.card)
        .border(1.0, theme.card_border);
    // The modal, for the panel the host draws last: focus arrives here when this subtree appears, Tab
    // stays inside it, the rest of the window is inert, Escape and any press off it take the dismissive
    // route, and the element that held focus before gets it back when this one goes away. `cancel` goes
    // to the modal *and* to the dismissive button, which is the second reason the routes are function
    // pointers: two owners of one route must not be able to disagree.
    let panel = match modal {
        Modal::Trap => panel.modal(true, cancel),
        Modal::Under => panel,
    };
    col()
        .key(part("scrim"))
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .fill(theme.shadow)
        .center()
        .child(
            panel
                .child(
                    text(spec.title.clone())
                        .key(part("title"))
                        .font(type_::TITLE)
                        .color(theme.text),
                )
                .child(
                    text(spec.body.clone())
                        .key(part("body"))
                        .font(type_::BODY)
                        .color(theme.text_secondary),
                )
                .child(footer),
        )
}

/// The footer, and the one place that knows how many Tab stops a panel has.
///
/// The shipped `.foot` is `justify-content: flex-end` behind a spacer, so the actions sit at the
/// trailing edge in the order Cancel, then Discard when the screen has one, then the action that
/// commits. That order *is* the Tab order the trap enforces, because the three are siblings and each is
/// focusable - so a screen gets its trap for free and cannot draw an order the keyboard would disagree
/// with.
fn footer<S: 'static>(
    spec: &Dialog,
    theme: rune::tok::Theme,
    id: &'static str,
    actions: &Actions<S>,
) -> El {
    let key = |suffix: &str| format!("{id}-{suffix}");
    let footer = row()
        .key(key("footer"))
        .w_full()
        .gap(space::S2)
        .center_y()
        // The shipped spacer span: `grow` is Rune's way of pushing the actions apart edge.
        .child(el().grow(1.0))
        .child(action(
            key("cancel"),
            spec.cancel,
            theme.control_off,
            theme.text,
            actions.cancel,
        ));
    let footer = match (spec.discard, actions.discard) {
        // The middle action exists exactly when the screen has a route for it, and one `match` over
        // both is what stops the label and the route from being given different answers: a `Discard`
        // button that led nowhere, or a route with no button, are both unrepresentable here.
        (Some(label), Some(on_discard)) => footer.child(action(
            key("discard"),
            label,
            theme.control_off,
            theme.text,
            on_discard,
        )),
        (Some(_), None) | (None, _) => footer,
    };
    // The destructive variant wants the palette's danger colour, which is the `theme-switch` hop this
    // shell has not made yet, so the commit takes the one accent token available and the row in the
    // matrix that covers the palette's colours stays `NOT COVERED`.
    footer.child(action(
        key("confirm"),
        spec.confirm,
        theme.indicator,
        theme.knob,
        actions.confirm,
    ))
}

/// One footer action: a focusable button, labelled with its own text so the accessible name and the
/// visible name are the same word.
fn action<S: 'static>(
    key: String,
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
