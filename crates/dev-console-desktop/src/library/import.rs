//! Import tasks from a project folder: `ProjectImport.svelte`.
//!
//! Scanning reads a folder's published task definitions (npm scripts, Makefile targets, just
//! recipes, Compose services); importing saves the chosen ones as commands. Neither executes
//! anything inside the folder.

use std::collections::BTreeSet;

use dev_console_core::models::{ImportProjectRequest, ScanProjectRequest};
use dev_console_core::projects::{Candidate, ProjectScan, Source};
use dev_console_core::services::Effects;
use rune::{col, el, icon, row, text, Cursor, El, FileDialogOptions, TextEdit, Tx};
use rune_kit::{Kit, TextInput};

use super::{dialogs, Library};
use crate::hub::message;
use crate::icons;
use crate::theme::{size, text as ts, Look};
use crate::widgets::{self, Tone, Variant};

const SOURCES: [Source; 4] = [Source::Npm, Source::Make, Source::Just, Source::Compose];

pub struct ProjectImport {
    pub path: TextEdit,
    pub scan: Option<ProjectScan>,
    pub selected: BTreeSet<String>,
    pub busy: bool,
    pub picking: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
}

impl ProjectImport {
    pub fn new() -> ProjectImport {
        let mut path = TextEdit::new(String::new());
        path.focused = true;
        ProjectImport {
            path,
            scan: None,
            selected: BTreeSet::new(),
            busy: false,
            picking: false,
            error: None,
            notice: None,
        }
    }

    pub fn importable(&self) -> Vec<&Candidate> {
        self.scan
            .iter()
            .flat_map(|scan| scan.candidates.iter())
            .filter(|candidate| !candidate.already_imported)
            .collect()
    }

    pub fn chosen(&self) -> Vec<String> {
        self.importable()
            .into_iter()
            .filter(|candidate| self.selected.contains(&candidate.name))
            .map(|candidate| candidate.name.clone())
            .collect()
    }

    pub fn choose(&mut self, names: &[String], want: bool) {
        for name in names {
            if want {
                self.selected.insert(name.clone());
            } else {
                self.selected.remove(name);
            }
        }
    }
}

impl Default for ProjectImport {
    fn default() -> Self {
        ProjectImport::new()
    }
}

impl Library {
    fn import_mut(&mut self) -> Option<&mut ProjectImport> {
        self.import.as_mut()
    }

    pub fn import_path(&mut self) -> &mut TextEdit {
        match &mut self.import {
            Some(panel) => &mut panel.path,
            None => &mut self.scratch,
        }
    }

    pub fn scan_project(&mut self) {
        let Some(panel) = self.import_mut() else {
            return;
        };
        let target = panel.path.text.trim().to_owned();
        if target.is_empty() {
            panel.error = Some("Enter a project folder first.".into());
            return;
        }
        panel.busy = true;
        panel.error = None;
        panel.notice = None;
        let actions = self.hub().library.clone();
        self.exec().clone().run(
            self,
            move || actions.scan_project(ScanProjectRequest { path: target }),
            |state, outcome| {
                let Some(panel) = state.import_mut() else {
                    return;
                };
                panel.busy = false;
                panel.selected.clear();
                match outcome {
                    Ok(found) => {
                        for candidate in &found.candidates {
                            if !candidate.already_imported {
                                panel.selected.insert(candidate.name.clone());
                            }
                        }
                        panel.scan = Some(found);
                    }
                    Err(error) => {
                        panel.scan = None;
                        panel.error = Some(message(&error));
                    }
                }
            },
        );
    }

    pub fn browse_project(&mut self, tx: &mut Tx) {
        let Some(panel) = self.import_mut() else {
            return;
        };
        if panel.busy || panel.picking {
            return;
        }
        panel.picking = true;
        let start = panel.path.text.trim().to_owned();
        let options = FileDialogOptions {
            title: "Choose a project folder".into(),
            directory: Some(start).filter(|dir| !dir.is_empty()).map(Into::into),
            ..Default::default()
        };
        tx.open_folder(options, |state: &mut Library, picked, _| {
            let Some(panel) = state.import_mut() else {
                return;
            };
            panel.picking = false;
            match picked {
                Ok(Some(folder)) => {
                    panel.path.replace_text(folder.to_string_lossy().into_owned());
                    state.scan_project();
                }
                Ok(None) => {}
                Err(error) => panel.error = Some(format!("The folder picker could not be opened. You can enter a path instead. ({error})")),
            }
        });
    }

    pub fn confirm_import(&mut self) {
        let Some(panel) = self.import_mut() else {
            return;
        };
        let Some(dir) = panel.scan.as_ref().map(|scan| scan.dir.clone()) else {
            return;
        };
        let names = panel.chosen();
        if names.is_empty() {
            return;
        }
        panel.busy = true;
        panel.error = None;
        panel.notice = None;
        let actions = self.hub().library.clone();
        let hub = self.hub().clone();
        self.exec().clone().run(
            self,
            move || {
                let mut effects = Effects::default();
                let outcome = actions.import_project(
                    ImportProjectRequest {
                        path: dir,
                        names,
                        all: false,
                    },
                    &mut effects,
                );
                (outcome, effects)
            },
            move |state, (outcome, effects)| {
                hub.dispatch(effects);
                let Some(panel) = state.import_mut() else {
                    return;
                };
                panel.busy = false;
                match outcome {
                    Ok(outcome) => {
                        let refused = !outcome.skipped.is_empty();
                        if refused || (outcome.imported == 0 && outcome.duplicates > 0) {
                            let wording = if refused {
                                format!(
                                    "{} could not be saved: {}",
                                    outcome.skipped.len(),
                                    outcome.skipped.join("; ")
                                )
                            } else {
                                format!(
                                    "All {} {} already saved here.",
                                    outcome.duplicates,
                                    if outcome.duplicates == 1 {
                                        "task is"
                                    } else {
                                        "tasks are"
                                    }
                                )
                            };
                            state.scan_project();
                            if let Some(panel) = state.import_mut() {
                                if refused {
                                    panel.error = Some(wording);
                                } else {
                                    panel.notice = Some(wording);
                                }
                            }
                            return;
                        }
                        state.finish_project_import(outcome.imported);
                    }
                    Err(error) => panel.error = Some(message(&error)),
                }
            },
        );
    }
}

fn checkbox(look: &Look, key: String, checked: bool, disabled: bool) -> El {
    let mut tick = row()
        .key(key)
        .w(16.0)
        .h(16.0)
        .center()
        .radius(size::RADIUS_SM)
        .border(
            1.0,
            if checked {
                look.accent
            } else {
                look.line_strong
            },
        )
        .fill(if checked { look.accent } else { look.sunken });
    if checked {
        tick = tick.child(icon(&icons::CHECK, 12.0).color(look.accent_fg));
    }
    if disabled {
        tick = tick.opacity(0.5);
    }
    tick
}

pub fn view(panel: &ProjectImport, look: &Look, kit: &Kit) -> El {
    let blocked = panel.busy || panel.picking;
    let field = TextInput::new(&panel.path)
        .width(None)
        .label("Project folder")
        .placeholder("~/src/my-project")
        .text_style(ts::MONO)
        .disabled(blocked)
        .on_edit(|state: &mut Library| state.import_path())
        .on_submit(|state: &mut Library, _| state.scan_project())
        .build(kit)
        .key("project-path");
    let browse = widgets::button(
        look,
        "project-browse",
        "Browse…",
        Some(&icons::FOLDER),
        Variant::Default,
    );
    let scan = widgets::button(
        look,
        "project-scan",
        if panel.scan.is_none() {
            "Scan"
        } else {
            "Rescan"
        },
        Some(&icons::SEARCH),
        Variant::Default,
    );
    let mut body = vec![col()
        .w_full()
        .gap(6.0)
        .child(widgets::section_label(look, "Project folder"))
        .child(
            row()
                .w_full()
                .gap(size::SP2)
                .center_y()
                .child(el().grow(1.0).min_w(0.0).child(field))
                .child(if blocked {
                    widgets::disabled(browse)
                } else {
                    browse.on_press_tx(|state: &mut Library, tx| state.browse_project(tx))
                })
                .child(if blocked {
                    widgets::disabled(scan)
                } else {
                    scan.on_press(|state: &mut Library| state.scan_project())
                }),
        )];
    if let Some(error) = &panel.error {
        body.push(widgets::status_line(
            look,
            "project-error",
            Tone::Error,
            error,
        ));
    } else if let Some(notice) = &panel.notice {
        body.push(widgets::status_line(
            look,
            "project-notice",
            Tone::Info,
            notice,
        ));
    }
    match &panel.scan {
        None => body.push(
            text(
                "Dev Console reads the task definitions a folder already publishes — package.json scripts, Makefile targets, justfile recipes and Docker Compose services — and offers them as commands that run in that folder. Nothing inside the folder is executed by scanning or by importing.",
            )
            .font(ts::SM)
            .color(look.muted)
            .wrap(),
        ),
        Some(scan) => {
            body.push(
                row()
                    .key("project-scan-summary")
                    .gap(size::SP2)
                    .center_y()
                    .child(text(scan.project.clone()).font(ts::STRONG).color(look.fg))
                    .child(text(scan.dir.clone()).font(ts::MONO_SM).color(look.muted).truncate()),
            );
            if scan.candidates.is_empty() {
                body.push(
                    text("No tasks found. Dev Console looks for package.json scripts, Makefile targets, justfile recipes and Docker Compose services in this folder itself.")
                        .key("project-empty")
                        .font(ts::SM)
                        .color(look.muted)
                        .wrap(),
                );
            } else {
                let importable: Vec<String> = panel.importable().iter().map(|candidate| candidate.name.clone()).collect();
                let chosen = panel.chosen().len();
                let everything = !importable.is_empty() && chosen == importable.len();
                let all_names = importable.clone();
                body.push(
                    row()
                        .w_full()
                        .gap(size::SP2)
                        .center_y()
                        .child(
                            widgets::button_sm(look, "project-select-all", if everything { "Select none" } else { "Select all" }, None, Variant::Ghost)
                                .on_press(move |state: &mut Library| {
                                    if let Some(panel) = state.import.as_mut() {
                                        panel.choose(&all_names, !everything);
                                    }
                                }),
                        )
                        .child(el().grow(1.0))
                        .child(text(format!("{chosen} of {} selected", importable.len())).key("project-import-count").font(ts::SM).color(look.muted)),
                );
                for source in SOURCES {
                    let items: Vec<&Candidate> = scan.candidates.iter().filter(|candidate| candidate.source == source).collect();
                    if items.is_empty() {
                        continue;
                    }
                    let open: Vec<String> = items.iter().filter(|candidate| !candidate.already_imported).map(|candidate| candidate.name.clone()).collect();
                    let all_on = open.iter().all(|name| panel.selected.contains(name));
                    let mut group = col()
                        .key(format!("project-group-{}", source.label()))
                        .w_full()
                        .gap(2.0)
                        .child(
                            row()
                                .w_full()
                                .center_y()
                                .child(widgets::section_label(look, source.label()))
                                .child(el().grow(1.0))
                                .child(
                                    widgets::button_sm(look, format!("project-group-all-{}", source.label()), if all_on { "None" } else { "All" }, None, Variant::Ghost)
                                        .on_press(move |state: &mut Library| {
                                            if let Some(panel) = state.import.as_mut() {
                                                panel.choose(&open, !all_on);
                                            }
                                        }),
                                ),
                        );
                    for candidate in items {
                        let checked = candidate.already_imported || panel.selected.contains(&candidate.name);
                        let name = candidate.name.clone();
                        let mut line = row()
                            .key(format!("project-candidate-{}", candidate.name))
                            .role("checkbox")
                            .label(candidate.name.clone())
                            .checked(checked)
                            .w_full()
                            .px(size::SP2)
                            .py(6.0)
                            .gap(size::SP2)
                            .radius(size::RADIUS)
                            .child(checkbox(look, format!("project-check-{}", candidate.name), checked, candidate.already_imported))
                            .child({
                                let mut words = col()
                                    .grow(1.0)
                                    .min_w(0.0)
                                    .gap(2.0)
                                    .child(text(candidate.name.clone()).font(ts::MONO).color(look.fg))
                                    .child(text(candidate.command.clone()).font(ts::MONO_SM).color(look.muted).truncate());
                                if !candidate.description.is_empty() {
                                    words = words.child(text(candidate.description.clone()).font(ts::SM).color(look.muted).wrap());
                                }
                                words
                            });
                        if candidate.already_imported {
                            line = line.opacity(0.7).child(widgets::badge(look, "Already saved", look.muted, look.panel).key(format!("project-already-saved-{}", candidate.name)));
                        } else {
                            line = line
                                .focusable(true)
                                .focus_ring(2.0, 1.0, look.focus)
                                .cursor(Cursor::Pointer)
                                .hover(look.fg.alpha(0.04))
                                .on_press(move |state: &mut Library| {
                                    if let Some(panel) = state.import.as_mut() {
                                        let want = !panel.selected.contains(&name);
                                        panel.choose(std::slice::from_ref(&name), want);
                                    }
                                });
                        }
                        group = group.child(line);
                    }
                    body.push(group);
                }
                let saved = scan.candidates.len() - importable.len();
                if saved > 0 {
                    body.push(text(format!("{saved} of these already run in this folder, so they are left alone.")).font(ts::SM).color(look.muted).wrap());
                }
            }
        }
    }
    let chosen = panel.chosen().len();
    let import = widgets::button(
        look,
        "project-import",
        &if panel.busy && panel.scan.is_some() {
            "Importing…".to_string()
        } else if chosen > 0 {
            format!("Import {chosen}")
        } else {
            "Import".to_string()
        },
        None,
        Variant::Primary,
    );
    let can_import = panel.scan.is_some() && chosen > 0 && !panel.picking && !panel.busy;
    dialogs::shell(
        look,
        "project-import-dialog",
        "Import tasks from a project",
        true,
        body,
        vec![
            el().grow(1.0),
            widgets::button(look, "project-cancel", "Cancel", None, Variant::Default)
                .on_press(|state: &mut Library| state.finish_project_import(0)),
            if can_import {
                import.on_press(|state: &mut Library| state.confirm_import())
            } else {
                widgets::disabled(import)
            },
        ],
        |state: &mut Library| state.finish_project_import(0),
    )
}
