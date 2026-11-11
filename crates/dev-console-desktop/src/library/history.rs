//! Project history suggestions, the "Suggested" group above the command list:
//! `HistorySuggestions.svelte`.
//!
//! Commands the shell ran often in a project, offered as starting points for new entries. Saving
//! one fills the ordinary editor; dismissing one tells the backend its hash only.

use dev_console_core::history::Suggestion;
use rune::{col, icon, row, text, Cursor, El, Key, KeyEvent, Tx};

use super::Library;
use crate::icons;
use crate::theme::{size, text as ts, Look};
use crate::widgets::{self, Variant};

impl Library {
    pub fn toggle_history(&mut self) {
        self.history_open = !self.history_open;
    }

    pub fn install_history_tracking(&mut self) {
        let actions = self.hub().library.clone();
        self.status = Some("Installing project tracking…".into());
        self.exec().clone().run(
            self,
            move || actions.install_history_tracking(),
            |state, outcome| {
                state.status = Some(match outcome {
                    Ok(message) => message,
                    Err(error) => crate::hub::message(&error),
                })
            },
        );
    }
}

/// A suggestion is selected like a saved entry; only Save opens an editable draft.
impl Library {
    pub fn select_suggestion(&mut self, key: &str) {
        if self.draft_dirty() {
            self.status = Some(super::BUSY_EDIT.into());
            return;
        }
        if !self.suggestions.iter().any(|s| s.key == key) {
            return;
        }
        self.cancel_edit();
        self.details_serial += 1;
        self.selected_id = None;
        self.details = None;
        self.selected_suggestion = Some(key.into());
        self.entry_options_open = false;
    }
}

pub fn view(state: &Library, look: &Look) -> El {
    let mut section = col()
        .key("history-suggestions")
        .label("Suggested commands")
        .w_full()
        .px(size::SP3)
        .pt(size::SP2)
        .pb(size::SP3)
        .gap(size::SP1);
    let head = row()
        .w_full()
        .center_y()
        .gap(size::SP2)
        .child(
            row()
                .key("history-suggestions-toggle")
                .role("button")
                .label("Suggested commands")
                .expanded(state.history_open)
                .focusable(true)
                .focus_ring(2.0, 1.0, look.focus)
                .cursor(Cursor::Pointer)
                .grow(1.0)
                .h(32.0)
                .gap(6.0)
                .center_y()
                .on_press_tx(|state: &mut Library, tx| {
                    tx.instant();
                    state.toggle_history();
                })
                .child(icon(&icons::CHEVRON, 11.0).color(look.muted).icon_rotation(
                    if state.history_open {
                        0.0
                    } else {
                        -std::f32::consts::FRAC_PI_2
                    },
                ))
                .child(text("Suggested").font(ts::PROJECT).color(look.muted))
                .child(
                    text(state.suggestions.len().to_string())
                        .font(ts::SM)
                        .color(look.dim),
                ),
        )
        .child(
            widgets::icon_button(
                look,
                "history-suggestions-rescan",
                "Refresh suggestions",
                &icons::UPDATE,
                look.muted,
            )
            .w(size::CONTROL_H_SM)
            .h(size::CONTROL_H_SM)
            .help("Read project command counts again")
            .on_press(|state: &mut Library| state.load_suggestions()),
        );
    section = section.child(head);
    if !state.history_open {
        return section;
    }
    if state.suggestions.is_empty() {
        return section
            .child(
                text(if state.suggestions_busy {
                    "Reading project counts…"
                } else {
                    "Suggestions appear here as you use projects."
                })
                .font(ts::SM)
                .color(look.muted)
                .wrap()
                .w_full()
                .px(size::SP2),
            )
            .child(
                widgets::button_sm(
                    look,
                    "history-tracking-setup",
                    "Set up project tracking",
                    None,
                    Variant::Ghost,
                )
                .self_start()
                .on_press(|state: &mut Library| state.install_history_tracking()),
            );
    }
    let limit = if state.history_expanded {
        state.suggestions.len()
    } else {
        3
    };
    let mut list = col().key("history-suggestion-list").w_full().gap(2.0);
    for suggestion in state.suggestions.iter().take(limit) {
        list = list.child(suggestion_row(state, look, suggestion));
    }
    section = section.child(list);
    if state.suggestions.len() > 3 {
        section = section.child(
            widgets::button_sm(
                look,
                "history-suggestions-show-all",
                &if state.history_expanded {
                    "Show less".to_string()
                } else {
                    format!("Show all {} suggestions", state.suggestions.len())
                },
                None,
                Variant::Ghost,
            )
            .self_start()
            .ml(size::SP2)
            .on_press_tx(|state: &mut Library, tx| {
                tx.instant();
                state.history_expanded = !state.history_expanded;
            }),
        );
    }
    section
}

fn suggestion_row(state: &Library, look: &Look, suggestion: &Suggestion) -> El {
    let key = suggestion.key.clone();
    let keys = key.clone();
    let selected = state.selected_suggestion.as_deref() == Some(&suggestion.key);
    row()
        .key(format!("history-suggestion-{}", suggestion.key))
        .w_full()
        .radius(size::RADIUS_ROW)
        .fill(if selected {
            look.selected
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(look.hover)
        .border_sides(
            [0.0, 0.0, 0.0, 2.0],
            if selected {
                look.accent
            } else {
                rune::Color::TRANSPARENT
            },
        )
        .child(
            col()
                .key(format!("history-suggestion-command-{}", suggestion.key))
                .role("button")
                .label(format!("Preview suggestion: {}", suggestion.command))
                .help(format!(
                    "{}\nSelect to preview this suggestion",
                    suggestion.command
                ))
                .focusable(true)
                .focus_ring(2.0, 1.0, look.focus)
                .cursor(Cursor::Pointer)
                .w_full()
                .h(48.0)
                .px(size::SP2)
                .center()
                .gap(3.0)
                .on_press_tx(move |state: &mut Library, tx| {
                    tx.instant();
                    state.select_suggestion(&key);
                })
                .on_key(
                    move |state: &mut Library, ev: &KeyEvent, tx: &mut Tx| match ev.key {
                        Key::Enter => {
                            tx.instant();
                            state.select_suggestion(&keys);
                            true
                        }
                        Key::Delete => {
                            state.dismiss_suggestion(&keys);
                            true
                        }
                        _ => false,
                    },
                )
                .child(
                    text(suggestion.command.clone())
                        .key(format!("history-suggestion-title-{}", suggestion.key))
                        .font(ts::MONO_SM)
                        .color(if selected { look.selected_fg } else { look.fg })
                        .truncate()
                        .w_full(),
                )
                .child(
                    text(format!(
                        "{} · {}×",
                        suggestion.project.as_deref().unwrap_or("Project history"),
                        suggestion.count
                    ))
                    .key(format!("history-suggestion-caption-{}", suggestion.key))
                    .font(ts::SM)
                    .color(look.muted)
                    .truncate()
                    .w_full(),
                ),
        )
}

pub fn preview(state: &Library, suggestion: &Suggestion, look: &Look) -> El {
    let save_key = suggestion.key.clone();
    let dismiss_key = suggestion.key.clone();
    let mut head = row()
        .w_full()
        .h(56.0)
        .min_h(56.0)
        .px(size::SP5)
        .gap(size::SP2)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], look.line_hair);
    if state.narrow {
        head = head.child(
            widgets::button_sm(
                look,
                "back-to-list",
                "Library",
                Some(&icons::BACK),
                Variant::Ghost,
            )
            .on_press_tx(|state: &mut Library, tx| {
                tx.instant();
                state.back_to_list();
            }),
        );
    }
    head = head
        .child(icon(&icons::HISTORY, 14.0).color(look.muted))
        .child(text("From project history").font(ts::SM).color(look.muted));
    let body = col().w_full().max_w(size::MEASURE).px(size::SP6).py(size::SP5).gap(size::SP5)
        .child(col().w_full().gap(size::SP2)
            .child(text("Suggested command").font(ts::DETAIL_TITLE).color(look.fg))
            .child(text(format!("{} · used {} times · {} project runs", suggestion.project.as_deref().unwrap_or("Project"), suggestion.count, suggestion.project_count))
                .font(ts::SM).color(look.muted).wrap().w_full()))
        .child(col().w_full().gap(size::SP3)
            .child(super::view::plate(look).child(text(suggestion.command.clone()).key("suggestion-command-text")
                .font(ts::MONO).color(look.fg).selectable(true).wrap_anywhere().grow(1.0)))
            .children(suggestion.working_directory.as_ref().map(|path| text(widgets::short_path(path)).font(ts::MONO_SM).color(look.muted).wrap_anywhere().help(path.clone()))))
        .child(text("Save this command to your library. You can change its name and command before saving.")
            .font(ts::BODY).color(look.muted).wrap().w_full())
        .child(row().gap(size::SP2).center_y()
            .child(widgets::button(look, format!("history-suggestion-save-{}", suggestion.key), "Save command…", Some(&icons::PLUS), Variant::Primary)
                .h(32.0).on_press(move |state: &mut Library| state.save_suggestion(&save_key)))
            .child(widgets::button(look, format!("history-suggestion-dismiss-{}", suggestion.key), "Dismiss", None, Variant::Ghost)
                .h(32.0).on_press(move |state: &mut Library| state.dismiss_suggestion(&dismiss_key))));
    col()
        .key("suggestion-preview")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .instant_changes()
        .child(head)
        .child(
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(rune::ScrollSpec {
                    stick_bottom: false,
                })
                .child(body),
        )
}
