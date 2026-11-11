//! The project environment panel: `ProjectEnvironment.svelte`.
//!
//! Local, encrypted versions of a project's `.env` files. The panel lists the saved versions and
//! copies one through the secret clipboard path; a vault lock closes it (see
//! `Library::apply_vault`), because what it shows is decrypted metadata.

use dev_console_core::environment::{ProjectMeta, SyncResult};
use dev_console_core::error::AppResult;
use dev_console_core::time_format::absolute_time;
use rune::{col, el, row, text, El, FileDialogOptions, Tx};

use super::{dialogs, Library};
use crate::hub::message;
use crate::icons;
use crate::theme::{size, text as ts, Look};
use crate::widgets::{self, Tone, Variant};

pub struct Environment {
    pub projects: Vec<ProjectMeta>,
    pub root: String,
    pub busy: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    /// The project picker is open.
    pub choosing: bool,
}

impl Environment {
    pub fn new(root: String) -> Environment {
        Environment {
            projects: Vec::new(),
            root,
            busy: false,
            error: None,
            notice: None,
            choosing: false,
        }
    }

    pub fn project(&self) -> Option<&ProjectMeta> {
        self.projects
            .iter()
            .find(|project| project.root == self.root)
    }

    /// The files with saved versions, in name order.
    pub fn files(&self) -> Vec<String> {
        let mut files: Vec<String> = self
            .project()
            .map(|project| {
                project
                    .snapshots
                    .iter()
                    .map(|snapshot| snapshot.file.clone())
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        files.dedup();
        files
    }
}

impl Library {
    /// Opens the panel, then lists the synced projects; a project the caller named is synced.
    pub fn open_environment(&mut self, root: String) {
        self.environment = Some(Environment::new(root.clone()));
        let actions = self.hub().library.clone();
        self.exec().clone().run(
            self,
            move || actions.environment_projects(),
            move |state, outcome: AppResult<Vec<ProjectMeta>>| {
                let Some(panel) = state.environment.as_mut() else {
                    return;
                };
                match outcome {
                    Ok(projects) => {
                        panel.projects = projects;
                        if root.is_empty() {
                            panel.root = panel
                                .projects
                                .first()
                                .map(|project| project.root.clone())
                                .unwrap_or_default();
                        } else {
                            state.sync_environment(root);
                        }
                    }
                    Err(error) => panel.error = Some(message(&error)),
                }
            },
        );
    }

    pub fn close_environment(&mut self) {
        self.environment = None;
    }

    pub fn sync_environment(&mut self, path: String) {
        let Some(panel) = self.environment.as_mut() else {
            return;
        };
        if panel.busy || path.is_empty() {
            return;
        }
        panel.busy = true;
        panel.error = None;
        panel.notice = None;
        let actions = self.hub().library.clone();
        self.exec().clone().run(
            self,
            move || actions.sync_environment(path),
            |state, outcome: AppResult<SyncResult>| {
                let Some(panel) = state.environment.as_mut() else {
                    return;
                };
                panel.busy = false;
                match outcome {
                    Ok(result) => {
                        panel.projects = result.projects;
                        panel.root = result.root;
                        panel.notice = Some(
                            "Synced locally. Unchanged files keep their existing version.".into(),
                        );
                    }
                    Err(error) => panel.error = Some(message(&error)),
                }
            },
        );
    }

    pub fn choose_environment(&mut self, root: String) {
        if let Some(panel) = self.environment.as_mut() {
            panel.choosing = false;
            panel.root = root.clone();
        }
        self.sync_environment(root);
    }

    pub fn browse_environment(&mut self, tx: &mut Tx) {
        let Some(panel) = self.environment.as_mut() else {
            return;
        };
        panel.error = None;
        let options = FileDialogOptions {
            title: "Choose a project folder".into(),
            directory: Some(panel.root.clone())
                .filter(|root| !root.is_empty())
                .map(Into::into),
            ..Default::default()
        };
        tx.open_folder(options, |state: &mut Library, picked, _| match picked {
            Ok(Some(folder)) => state.choose_environment(folder.to_string_lossy().into_owned()),
            Ok(None) => {}
            Err(error) => {
                if let Some(panel) = state.environment.as_mut() {
                    panel.error = Some(error.to_string());
                }
            }
        });
    }

    pub fn toggle_environment_sync(&mut self) {
        let Some(panel) = self.environment.as_mut() else {
            return;
        };
        let Some(enabled) = panel.project().map(|project| project.enabled) else {
            return;
        };
        if panel.busy {
            return;
        }
        panel.busy = true;
        panel.error = None;
        panel.notice = None;
        let root = panel.root.clone();
        let actions = self.hub().library.clone();
        self.exec().clone().run(
            self,
            move || {
                actions
                    .set_environment_sync(root, !enabled)
                    .and_then(|()| actions.environment_projects())
            },
            |state, outcome| {
                let Some(panel) = state.environment.as_mut() else {
                    return;
                };
                panel.busy = false;
                match outcome {
                    Ok(projects) => panel.projects = projects,
                    Err(error) => panel.error = Some(message(&error)),
                }
            },
        );
    }

    pub fn copy_environment(&mut self, id: String) {
        let Some(panel) = self.environment.as_mut() else {
            return;
        };
        if panel.busy {
            return;
        }
        panel.busy = true;
        panel.error = None;
        panel.notice = None;
        let actions = self.hub().library.clone();
        self.exec().clone().run(
            self,
            move || actions.copy_environment(id),
            |state, outcome| {
                let Some(panel) = state.environment.as_mut() else {
                    return;
                };
                panel.busy = false;
                match outcome {
                    Ok(_) => {
                        panel.notice = Some(
                            "Copied environment file. The secret clipboard expiry applies.".into(),
                        )
                    }
                    Err(error) => panel.error = Some(message(&error)),
                }
            },
        );
    }
}

pub fn view(_state: &Library, panel: &Environment, look: &Look) -> El {
    let busy = panel.busy;
    let mut body = vec![text(
        "Encrypted, local versions of the .env files at the top of a project folder. Sync starts when a tracked terminal enters the folder.",
    )
    .font(ts::BODY)
    .color(look.muted)
    .wrap()];

    let picker = widgets::button(
        look,
        "environment-project",
        &if panel.root.is_empty() {
            "Choose a project folder".to_string()
        } else {
            widgets::short_path(&panel.root)
        },
        Some(&icons::CHEVRON),
        Variant::Default,
    )
    .label("Project");
    let browse = widgets::button(
        look,
        "environment-browse",
        "Browse",
        Some(&icons::FOLDER),
        Variant::Default,
    );
    body.push(
        row()
            .w_full()
            .gap(size::SP2)
            .center_y()
            .child(text("Project").font(ts::BODY).color(look.fg).grow(1.0))
            .child(if busy {
                widgets::disabled(picker)
            } else {
                picker.on_press(|state: &mut Library| {
                    if let Some(panel) = state.environment.as_mut() {
                        panel.choosing = !panel.choosing;
                    }
                })
            })
            .child(if busy {
                widgets::disabled(browse)
            } else {
                browse.on_press_tx(|state: &mut Library, tx| state.browse_environment(tx))
            }),
    );
    if panel.choosing {
        let mut options = col()
            .key("environment-options")
            .w_full()
            .gap(2.0)
            .padding(4.0)
            .radius(size::RADIUS)
            .fill(look.fg.alpha(0.04));
        for project in &panel.projects {
            let root = project.root.clone();
            options = options.child(
                row()
                    .key(format!("environment-option-{}", project.root))
                    .role("option")
                    .label(project.root.clone())
                    .focusable(true)
                    .w_full()
                    .h(28.0)
                    .px(size::SP2)
                    .center_y()
                    .radius(size::RADIUS_SM)
                    .hover(look.fg.alpha(0.05))
                    .on_press(move |state: &mut Library| state.choose_environment(root.clone()))
                    .child(
                        text(widgets::short_path(&project.root))
                            .font(ts::MONO_SM)
                            .color(look.fg)
                            .truncate(),
                    ),
            );
        }
        if panel.projects.is_empty() {
            options = options.child(
                text("No projects synced yet. Use Browse to pick one.")
                    .font(ts::SM)
                    .color(look.muted),
            );
        }
        body.push(options);
    }
    if let Some(project) = panel.project() {
        let toggle = widgets::button_sm(
            look,
            "environment-toggle",
            if project.enabled {
                "Pause sync"
            } else {
                "Resume sync"
            },
            None,
            Variant::Default,
        );
        let sync = widgets::button_sm(
            look,
            "environment-sync",
            "Sync now",
            Some(&icons::UPDATE),
            Variant::Default,
        );
        let root = panel.root.clone();
        body.push(
            row()
                .w_full()
                .gap(size::SP2)
                .center_y()
                .child(
                    text(if project.enabled {
                        "Syncs every 30 seconds while the vault is unlocked."
                    } else {
                        "Sync paused. Saved versions are kept."
                    })
                    .font(ts::SM)
                    .color(look.muted)
                    .wrap()
                    .grow(1.0),
                )
                .child(if busy {
                    widgets::disabled(toggle)
                } else {
                    toggle.on_press(|state: &mut Library| state.toggle_environment_sync())
                })
                .child(if busy {
                    widgets::disabled(sync)
                } else {
                    sync.on_press(move |state: &mut Library| state.sync_environment(root.clone()))
                }),
        );
    }
    if let Some(error) = &panel.error {
        body.push(widgets::status_line(
            look,
            "environment-error",
            Tone::Error,
            error,
        ));
    }
    if let Some(notice) = &panel.notice {
        body.push(widgets::status_line(
            look,
            "environment-notice",
            Tone::Ok,
            notice,
        ));
    }
    let files = panel.files();
    if panel.project().is_some() && files.is_empty() {
        body.push(
            text("No .env files here yet. They are picked up when they appear.")
                .key("environment-empty")
                .font(ts::SM)
                .color(look.muted)
                .wrap(),
        );
    }
    for file in files {
        let mut section = col()
            .key(format!("environment-file-{file}"))
            .label(file.clone())
            .w_full()
            .gap(4.0)
            .child(text(file.clone()).font(ts::MONO).color(look.fg));
        for snapshot in panel
            .project()
            .into_iter()
            .flat_map(|project| project.snapshots.iter())
            .filter(|snapshot| snapshot.file == file)
        {
            let id = snapshot.id.clone();
            let copy = widgets::button_sm(
                look,
                format!("environment-copy-{}", snapshot.id),
                "Copy",
                Some(&icons::COPY),
                Variant::Default,
            );
            section = section.child(
                row()
                    .key(format!("environment-version-{}", snapshot.id))
                    .w_full()
                    .gap(size::SP3)
                    .px(size::SP2)
                    .py(4.0)
                    .center_y()
                    .radius(size::RADIUS)
                    .fill(look.sunken)
                    .child(
                        text(format!("Version {}", snapshot.version))
                            .font(ts::SM)
                            .color(look.fg),
                    )
                    .child(
                        text(absolute_time(&snapshot.saved_at))
                            .font(ts::SM)
                            .color(look.muted),
                    )
                    .child(
                        text(format!("{} bytes", snapshot.bytes))
                            .font(ts::SM)
                            .color(look.muted),
                    )
                    .child(el().grow(1.0))
                    .child(if busy {
                        widgets::disabled(copy)
                    } else {
                        copy.on_press(move |state: &mut Library| state.copy_environment(id.clone()))
                    }),
            );
        }
        body.push(section);
    }
    body.push(
        text("Versions stay here even if the file is deleted, and are part of your encrypted backups.")
            .font(ts::SM)
            .color(look.dim)
            .wrap(),
    );
    dialogs::shell(
        look,
        "environment-dialog",
        "Project environment",
        true,
        body,
        vec![
            el().grow(1.0),
            widgets::button(look, "environment-close", "Close", None, Variant::Default)
                .on_press(|state: &mut Library| state.close_environment()),
        ],
        |state: &mut Library| state.close_environment(),
    )
}
