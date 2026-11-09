//! Project history suggestions above the command list: `HistorySuggestions.svelte`.
//!
//! Commands the shell ran often in a project, offered as starting points for new entries. Saving
//! one fills the ordinary editor; dismissing one tells the backend its hash only.

use dev_console_core::history::Suggestion;
use rune::{col, el, icon, row, text, Cursor, El, Key, KeyEvent, Tx};

use super::Library;
use crate::icons;
use crate::theme::{size, text as ts, Look};
use crate::widgets::{self, Variant};

struct Project<'a> {
    name: String,
    root: String,
    count: usize,
    suggestions: Vec<&'a Suggestion>,
}

fn projects(suggestions: &[Suggestion]) -> Vec<Project<'_>> {
    let mut out: Vec<Project> = Vec::new();
    for suggestion in suggestions {
        let root = suggestion.project_root.clone().unwrap_or_default();
        match out.iter_mut().find(|project| project.root == root) {
            Some(project) => project.suggestions.push(suggestion),
            None => out.push(Project {
                name: suggestion
                    .project
                    .clone()
                    .unwrap_or_else(|| "Project".into()),
                root,
                count: suggestion.project_count,
                suggestions: vec![suggestion],
            }),
        }
    }
    out
}

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

pub fn view(state: &Library, look: &Look) -> El {
    let groups = projects(&state.suggestions);
    let mut head = row()
        .key("history-suggestions-toggle")
        .role("button")
        .label("Project history")
        .expanded(state.history_open)
        .focusable(true)
        .focus_ring(2.0, 1.0, look.focus)
        .w_full()
        .h(34.0)
        .px(size::SP3)
        .gap(size::SP2)
        .center_y()
        .cursor(Cursor::Pointer)
        .hover(look.fg.alpha(0.04))
        .on_press(|state: &mut Library| state.toggle_history())
        .child(icon(&icons::HISTORY, 13.0).color(look.muted))
        .child(text("Project history").font(ts::STRONG).color(look.fg));
    if !groups.is_empty() {
        head = head.child(widgets::badge(
            look,
            &format!("{} projects", groups.len()),
            look.muted,
            look.raised,
        ));
    }
    head = head.child(el().grow(1.0)).child(
        icon(&icons::CHEVRON, 13.0)
            .color(look.muted)
            .icon_rotation(if state.history_open {
                0.0
            } else {
                -std::f32::consts::FRAC_PI_2
            }),
    );

    let mut section = col()
        .key("history-suggestions")
        .label("Project history")
        .w_full()
        .border_sides([0.0, 0.0, 1.0, 0.0], look.line)
        .child(head);
    if !state.history_open {
        return section;
    }
    let mut list = col()
        .key("history-suggestion-list")
        .w_full()
        .px(size::SP2)
        .pb(size::SP2)
        .gap(size::SP1);
    if state.suggestions.is_empty() {
        list = list
            .child(
                text("Suggestions appear here as you use projects. Set up tracking, then open a new terminal.")
                    .font(ts::SM)
                    .color(look.muted)
                    .wrap()
                    .w_full()
                    .px(size::SP1),
            )
            .child(
                widgets::button_sm(look, "history-tracking-setup", "Enable project tracking for zsh", None, Variant::Default)
                    .on_press(|state: &mut Library| state.install_history_tracking())
                    .self_start(),
            );
    }
    let single = groups.len() == 1;
    for project in groups {
        let mut block = col()
            .key(format!("history-project-{}", project.root))
            .w_full()
            .gap(2.0)
            .child(
                row()
                    .h(26.0)
                    .px(size::SP1)
                    .gap(6.0)
                    .center_y()
                    .help(project.root.clone())
                    .child(icon(&icons::PROJECT, 12.0).color(look.muted))
                    .child(
                        text(project.name.clone())
                            .font(ts::STRONG)
                            .color(look.fg)
                            .truncate()
                            .grow(1.0),
                    )
                    .child(widgets::badge(
                        look,
                        &format!("{} runs", project.count),
                        look.muted,
                        look.raised,
                    )),
            );
        let _ = single;
        for suggestion in project.suggestions {
            block = block.child(suggestion_row(look, suggestion));
        }
        list = list.child(block);
    }
    list = list.child(
        row()
            .w_full()
            .gap(size::SP2)
            .center_y()
            .child(
                text(if state.suggestions_busy {
                    "Reading project counts…"
                } else if state.suggestions.is_empty() {
                    ""
                } else {
                    "Top five projects by recorded runs. Counts stay separate per project and folder."
                })
                .font(ts::XS)
                .color(look.muted)
                .wrap()
                .grow(1.0),
            )
            .child(
                widgets::button_sm(look, "history-suggestions-rescan", "Rescan", Some(&icons::UPDATE), Variant::Ghost)
                    .on_press(|state: &mut Library| state.load_suggestions()),
            ),
    );
    section = section.child(
        col()
            .w_full()
            .max_h(260.0)
            .scroll(rune::ScrollSpec {
                stick_bottom: false,
            })
            .child(list),
    );
    section
}

fn suggestion_row(look: &Look, suggestion: &Suggestion) -> El {
    let save_key = suggestion.key.clone();
    let press_key = suggestion.key.clone();
    let keys_key = suggestion.key.clone();
    let dismiss_key = suggestion.key.clone();
    col()
        .key(format!("history-suggestion-{}", suggestion.key))
        .w_full()
        .gap(4.0)
        .padding(size::SP2)
        .radius(size::RADIUS)
        .fill(look.raised)
        .border(1.0, look.line)
        .child(
            row()
                .key(format!("history-suggestion-command-{}", suggestion.key))
                .role("button")
                .label(format!(
                    "{} - used {} times. Enter saves it as “{}”, Delete dismisses it.",
                    suggestion.command, suggestion.count, suggestion.suggested_name
                ))
                .help("Enter saves it, Delete dismisses it")
                .focusable(true)
                .focus_ring(2.0, 1.0, look.focus)
                .w_full()
                .cursor(Cursor::Pointer)
                .on_press(move |state: &mut Library| state.save_suggestion(&press_key))
                .on_key(
                    move |state: &mut Library, ev: &KeyEvent, _: &mut Tx| match ev.key {
                        Key::Enter => {
                            state.save_suggestion(&keys_key);
                            true
                        }
                        Key::Delete => {
                            state.dismiss_suggestion(&keys_key);
                            true
                        }
                        _ => false,
                    },
                )
                .child(
                    text(suggestion.command.clone())
                        .font(ts::MONO_SM)
                        .color(look.fg)
                        .truncate()
                        .grow(1.0),
                ),
        )
        .child(
            row()
                .w_full()
                .gap(size::SP1)
                .center_y()
                .child(
                    text(suggestion.suggested_name.clone())
                        .font(ts::XS)
                        .color(look.muted)
                        .truncate()
                        .min_w(0.0)
                        .shrink(),
                )
                .child(widgets::badge(
                    look,
                    &format!("used {}×", suggestion.count),
                    look.muted,
                    look.panel,
                ))
                .child(el().grow(1.0))
                .child(
                    widgets::button_sm(
                        look,
                        format!("history-suggestion-save-{}", suggestion.key),
                        "Save",
                        Some(&icons::PLUS),
                        Variant::Primary,
                    )
                    .on_press(move |state: &mut Library| state.save_suggestion(&save_key)),
                )
                .child(
                    widgets::icon_button(
                        look,
                        format!("history-suggestion-dismiss-{}", suggestion.key),
                        "Dismiss this suggestion",
                        &icons::CLOSE,
                        look.muted,
                    )
                    .on_press(move |state: &mut Library| state.dismiss_suggestion(&dismiss_key)),
                ),
        )
}
