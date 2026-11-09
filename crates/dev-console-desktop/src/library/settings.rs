//! The Settings panel: `SettingsPanel.svelte`.
//!
//! General settings (theme, clipboard expiry, autostart, history suggestions, terminal), the Finder's
//! project ranking, backups, export and import of encrypted archives, and the recovery credential.
//! Choosing a theme previews it at once; closing without saving puts the saved one back.
//!
//! The recovery credential decrypts every archive, so it is shown only on request, copied through
//! the core clipboard service (expiry and clearing on lock), and dropped the moment the vault locks
//! ([`SettingsPanel::on_lock`]); an answer that arrives after a lock is ignored by its epoch.

use dev_console_core::argv::{format_argv, parse_argv};
use dev_console_core::backup::{ArchiveSummary, BackupInfo};
use dev_console_core::execution::TerminalOption;
use dev_console_core::models::{ClipboardExclusion, Kind, ListRequest, Settings};
use dev_console_core::services::Effects;
use dev_console_core::services::RecoveryCredential;
use dev_console_core::time_format::absolute_time;
use rune::{col, el, icon, row, text, Cursor, El, TextEdit, Transition};
use rune_kit::{Kit, TextInput};

use super::{dialogs, Library};
use crate::hub::message;
use crate::icons;
use crate::theme::{self, size, text as ts, Look};
use crate::widgets::{self, Tone, Variant};

/// Which archive a review card is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewKind {
    Backup,
    File,
}

pub struct SettingsPanel {
    pub settings: Option<Settings>,
    /// The theme on disk, restored if the panel closes without saving.
    pub saved_theme: String,
    pub expiry: TextEdit,
    pub terminal: TextEdit,
    pub terminal_tokens: Vec<String>,
    pub terminal_error: Option<String>,
    pub terminals: Vec<TerminalOption>,
    pub terminal_choice: String,
    pub choosing_theme: bool,
    pub choosing_terminal: bool,
    pub exclusion: Option<ClipboardExclusion>,
    pub backup_dir: String,
    pub dismissed: usize,
    pub current_commands: usize,
    pub current_secrets: Option<usize>,
    pub backups: Vec<BackupInfo>,
    pub backup_busy: bool,
    pub backup_message: Option<String>,
    pub backup_error: Option<String>,
    pub export_path: TextEdit,
    pub export_confirm: Option<String>,
    pub import_path: TextEdit,
    pub review: Option<(ReviewKind, ArchiveSummary)>,
    pub pre_backup: bool,
    pub recovery: Option<RecoveryCredential>,
    pub recovery_error: Option<String>,
    pub recovery_copied_until: Option<i64>,
    pub error: Option<String>,
    pub saving: bool,
    lock_epoch: u64,
}

impl SettingsPanel {
    pub fn new(theme: String) -> SettingsPanel {
        SettingsPanel {
            settings: None,
            saved_theme: theme,
            expiry: TextEdit::new(String::new()),
            terminal: TextEdit::new(String::new()),
            terminal_tokens: Vec::new(),
            terminal_error: None,
            terminals: Vec::new(),
            terminal_choice: "custom".into(),
            choosing_theme: false,
            choosing_terminal: false,
            exclusion: None,
            backup_dir: String::new(),
            dismissed: 0,
            current_commands: 0,
            current_secrets: None,
            backups: Vec::new(),
            backup_busy: false,
            backup_message: None,
            backup_error: None,
            export_path: TextEdit::new(String::new()),
            export_confirm: None,
            import_path: TextEdit::new(String::new()),
            review: None,
            pre_backup: true,
            recovery: None,
            recovery_error: None,
            recovery_copied_until: None,
            error: None,
            saving: false,
            lock_epoch: 0,
        }
    }

    /// A lock drops the revealed credential and invalidates any reveal still in flight.
    pub fn on_lock(&mut self) {
        self.lock_epoch += 1;
        if let Some(recovery) = self.recovery.as_mut() {
            zero(&mut recovery.secret);
        }
        self.recovery = None;
        self.recovery_copied_until = None;
        self.current_secrets = None;
    }

    fn set_terminal(&mut self) {
        self.terminal_error = None;
        match parse_argv(&self.terminal.text) {
            Ok(tokens) => {
                self.terminal_tokens = tokens.clone();
                if let Some(settings) = self.settings.as_mut() {
                    settings.terminal = tokens;
                }
            }
            Err(error) => {
                self.terminal_tokens.clear();
                self.terminal_error = Some(error.to_string());
            }
        }
    }

    fn matches(command: &[String], option: &TerminalOption) -> bool {
        command.len() == option.command.len()
            && command.first().and_then(|first| first.rsplit('/').next()) == Some(option.id)
            && command
                .iter()
                .skip(1)
                .zip(option.command.iter().skip(1))
                .all(|(a, b)| a == b)
    }

    /// The expiry field as a number, clamped to the field's 5..600 range.
    fn expiry_seconds(&self) -> Option<u64> {
        self.expiry
            .text
            .trim()
            .parse::<u64>()
            .ok()
            .map(|seconds| seconds.clamp(5, 600))
    }
}

impl Drop for SettingsPanel {
    fn drop(&mut self) {
        self.on_lock();
    }
}

fn zero(secret: &mut String) {
    // SAFETY: overwriting with zero bytes keeps the string valid UTF-8.
    unsafe { secret.as_bytes_mut().fill(0) };
    secret.clear();
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

fn ms_to_iso(ms: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .ok()
        .and_then(|stamp| {
            stamp
                .format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_default()
}

impl Library {
    fn panel(&mut self) -> Option<&mut SettingsPanel> {
        self.settings.as_mut()
    }

    pub fn open_settings(&mut self) {
        if self.settings.is_some() {
            return;
        }
        self.settings_serial += 1;
        let serial = self.settings_serial;
        let sensitive_serial = self.sensitive_serial;
        self.settings = Some(SettingsPanel::new(self.theme.clone()));
        let actions = self.hub().library.clone();
        self.exec().clone().run(
            self,
            move || {
                let settings = actions.get_settings();
                let app = actions.app_state();
                let terminals = actions.list_terminals();
                let backups = actions.list_backups();
                let dismissed = actions.dismissed_suggestion_count().unwrap_or(0);
                let commands = actions
                    .list_entries(ListRequest {
                        kind: Kind::Command,
                        ..Default::default()
                    })
                    .map_or(0, |all| all.len());
                let secrets = actions
                    .list_entries(ListRequest {
                        kind: Kind::Secret,
                        ..Default::default()
                    })
                    .ok()
                    .map(|all| all.len());
                (
                    settings, app, terminals, backups, dismissed, commands, secrets,
                )
            },
            move |state, (settings, app, terminals, backups, dismissed, commands, secrets)| {
                if serial != state.settings_serial {
                    return;
                }
                let secrets = if sensitive_serial == state.sensitive_serial {
                    secrets
                } else {
                    None
                };
                let Some(panel) = state.panel() else {
                    return;
                };
                panel.saved_theme = settings.theme.clone();
                panel
                    .expiry
                    .replace_text(settings.clipboard_expiry_seconds.to_string());
                panel.terminal.replace_text(format_argv(&settings.terminal));
                panel.terminal_tokens = settings.terminal.clone();
                panel.terminal_choice = terminals
                    .iter()
                    .find(|option| SettingsPanel::matches(&settings.terminal, option))
                    .map_or("custom".to_string(), |option| option.id.to_string());
                panel.terminals = terminals;
                panel.exclusion = Some(app.clipboard_exclusion);
                panel.backup_dir = app.backup_dir;
                panel.dismissed = dismissed;
                panel.current_commands = commands;
                panel.current_secrets = secrets;
                match backups {
                    Ok(backups) => panel.backups = backups,
                    Err(error) => panel.backup_error = Some(message(&error)),
                }
                panel.settings = Some(settings);
            },
        );
    }

    /// Close without saving: put the saved theme back.
    pub fn close_settings(&mut self) {
        self.settings_serial += 1;
        if let Some(panel) = self.settings.take() {
            self.theme = panel.saved_theme.clone();
        }
    }

    pub fn preview_theme(&mut self, theme: &str) {
        if let Some(panel) = self.panel() {
            panel.choosing_theme = false;
            if let Some(settings) = panel.settings.as_mut() {
                settings.theme = theme.to_owned();
            }
        }
        self.theme = theme.to_owned();
    }

    pub fn choose_terminal(&mut self, id: &str) {
        let Some(panel) = self.panel() else {
            return;
        };
        panel.choosing_terminal = false;
        panel.terminal_choice = id.to_owned();
        if let Some(option) = panel.terminals.iter().find(|option| option.id == id) {
            let text = format_argv(&option.command);
            panel.terminal.replace_text(text);
            panel.set_terminal();
        }
    }

    pub fn terminal_edited(&mut self) {
        if let Some(panel) = self.panel() {
            panel.set_terminal();
        }
    }

    pub fn settings_toggle(&mut self, pick: fn(&mut Settings) -> &mut bool) {
        if let Some(settings) = self.panel().and_then(|panel| panel.settings.as_mut()) {
            let flag = pick(settings);
            *flag = !*flag;
        }
    }

    pub fn save_settings(&mut self) {
        let hub = self.hub().clone();
        let actions = self.hub().library.clone();
        let Some(panel) = self.panel() else {
            return;
        };
        if panel.terminal_error.is_some() || panel.saving {
            return;
        }
        let Some(mut settings) = panel.settings.clone() else {
            return;
        };
        if let Some(seconds) = panel.expiry_seconds() {
            settings.clipboard_expiry_seconds = seconds;
        }
        panel.error = None;
        panel.saving = true;
        self.exec().clone().run(
            self,
            move || {
                let mut effects = Effects::default();
                let saved = actions.save_settings(settings, &mut effects);
                (saved, effects)
            },
            move |state, (saved, effects)| {
                match saved {
                    Ok(saved) => {
                        state.theme = saved.theme.clone();
                        if let Some(panel) = state.panel() {
                            panel.saved_theme = saved.theme.clone();
                        }
                        state.settings = None;
                        state.on_saved_settings(&saved);
                    }
                    Err(error) => {
                        if let Some(panel) = state.panel() {
                            panel.saving = false;
                            panel.error = Some(message(&error));
                        }
                    }
                }
                hub.dispatch(effects);
            },
        );
    }

    fn on_saved_settings(&mut self, saved: &Settings) {
        self.history_enabled = saved.history_suggestions_enabled;
        if saved.history_suggestions_enabled {
            self.load_suggestions();
        } else {
            self.suggestions.clear();
        }
    }

    pub fn restore_suggestions(&mut self) {
        let actions = self.hub().library.clone();
        self.exec().clone().run(
            self,
            move || actions.clear_dismissed_suggestions(),
            |state, outcome| match outcome {
                Ok(_) => {
                    if let Some(panel) = state.panel() {
                        panel.dismissed = 0;
                    }
                    state.load_suggestions();
                }
                Err(error) => {
                    if let Some(panel) = state.panel() {
                        panel.error = Some(message(&error));
                    }
                }
            },
        );
    }

    fn refresh_backups(&mut self) {
        let actions = self.hub().library.clone();
        self.exec().clone().run(
            self,
            move || actions.list_backups(),
            |state, outcome| {
                if let Some(panel) = state.panel() {
                    match outcome {
                        Ok(backups) => panel.backups = backups,
                        Err(error) => panel.backup_error = Some(message(&error)),
                    }
                }
            },
        );
    }

    fn begin_backup_work(&mut self) -> bool {
        let Some(panel) = self.panel() else {
            return false;
        };
        if panel.backup_busy {
            return false;
        }
        panel.backup_busy = true;
        panel.backup_error = None;
        panel.backup_message = None;
        true
    }

    pub fn back_up_now(&mut self) {
        if !self.begin_backup_work() {
            return;
        }
        let actions = self.hub().library.clone();
        self.exec().clone().run(
            self,
            move || actions.create_backup(),
            |state, outcome| {
                if let Some(panel) = state.panel() {
                    panel.backup_busy = false;
                    match outcome {
                        Ok(_) => panel.backup_message = Some("Encrypted backup written.".into()),
                        Err(error) => panel.backup_error = Some(message(&error)),
                    }
                }
                state.refresh_backups();
            },
        );
    }

    /// Export. A normal export never replaces an existing file; that takes the explicit card.
    pub fn export_archive(&mut self, overwrite: bool) {
        let Some(path) = self
            .panel()
            .map(|panel| panel.export_path.text.trim().to_owned())
        else {
            return;
        };
        if path.is_empty() || !self.begin_backup_work() {
            return;
        }
        let actions = self.hub().library.clone();
        let target = path.clone();
        self.exec().clone().run(
            self,
            move || actions.export_archive(target, Some(overwrite)),
            move |state, outcome| {
                let Some(panel) = state.panel() else {
                    return;
                };
                panel.backup_busy = false;
                match outcome {
                    Ok(_) => {
                        panel.export_confirm = None;
                        panel.backup_message = Some(format!("Archive exported to {path}."));
                    }
                    Err(error) => {
                        let text = message(&error);
                        if text.contains("already exists") {
                            panel.export_confirm = Some(path);
                        }
                        panel.backup_error = Some(text);
                    }
                }
            },
        );
    }

    pub fn review_backup(&mut self, name: String) {
        let actions = self.hub().library.clone();
        if let Some(panel) = self.panel() {
            panel.backup_error = None;
        }
        self.exec().clone().run(
            self,
            move || actions.inspect_backup(name),
            |state, outcome| {
                if let Some(panel) = state.panel() {
                    match outcome {
                        Ok(summary) => panel.review = Some((ReviewKind::Backup, summary)),
                        Err(error) => panel.backup_error = Some(message(&error)),
                    }
                }
            },
        );
    }

    pub fn review_import(&mut self) {
        let Some(path) = self
            .panel()
            .map(|panel| panel.import_path.text.trim().to_owned())
        else {
            return;
        };
        if path.is_empty() {
            return;
        }
        let actions = self.hub().library.clone();
        if let Some(panel) = self.panel() {
            panel.backup_error = None;
        }
        self.exec().clone().run(
            self,
            move || actions.inspect_archive(path),
            |state, outcome| {
                if let Some(panel) = state.panel() {
                    match outcome {
                        Ok(summary) => panel.review = Some((ReviewKind::File, summary)),
                        Err(error) => panel.backup_error = Some(message(&error)),
                    }
                }
            },
        );
    }

    pub fn cancel_review(&mut self) {
        if let Some(panel) = self.panel() {
            panel.review = None;
        }
    }

    /// Replaces the whole library with the reviewed archive, backing up first unless declined.
    pub fn confirm_apply(&mut self) {
        let Some((kind, summary, pre_backup)) = self.panel().and_then(|panel| {
            panel
                .review
                .take()
                .map(|(kind, summary)| (kind, summary, panel.pre_backup))
        }) else {
            return;
        };
        if !self.begin_backup_work() {
            return;
        }
        let actions = self.hub().library.clone();
        let hub = self.hub().clone();
        let source = summary.source.clone();
        self.exec().clone().run(
            self,
            move || {
                if pre_backup {
                    if let Err(error) = actions.create_backup() {
                        return (Err(error), Effects::default());
                    }
                }
                let mut effects = Effects::default();
                let outcome = match kind {
                    ReviewKind::Backup => actions.restore_backup(source, &mut effects),
                    ReviewKind::File => actions.import_archive(source, &mut effects),
                };
                (outcome, effects)
            },
            move |state, (outcome, effects)| {
                if let Some(panel) = state.panel() {
                    panel.backup_busy = false;
                    match outcome {
                        Ok(outcome) => {
                            panel.backup_message = Some(format!(
                                "Replaced the library with {} ({} commands, {} secrets){}.",
                                summary.source,
                                outcome.commands,
                                outcome.secrets,
                                if pre_backup {
                                    " - your previous data was backed up first"
                                } else {
                                    ""
                                }
                            ))
                        }
                        Err(error) => panel.backup_error = Some(message(&error)),
                    }
                }
                state.refresh_backups();
                hub.dispatch(effects);
            },
        );
    }

    pub fn reveal_recovery(&mut self) {
        let serial = self.settings_serial;
        let sensitive_serial = self.sensitive_serial;
        let Some(epoch) = self.panel().map(|panel| {
            panel.recovery_error = None;
            panel.lock_epoch
        }) else {
            return;
        };
        let actions = self.hub().library.clone();
        self.exec().clone().run(
            self,
            move || actions.recovery_key(),
            move |state, outcome| {
                if serial != state.settings_serial || sensitive_serial != state.sensitive_serial {
                    if let Ok(mut revealed) = outcome {
                        zero(&mut revealed.secret);
                    }
                    return;
                }
                let Some(panel) = state.panel() else {
                    return;
                };
                match outcome {
                    // A lock between asking and answering voids the answer.
                    Ok(mut revealed) if epoch != panel.lock_epoch => zero(&mut revealed.secret),
                    Ok(revealed) => panel.recovery = Some(revealed),
                    Err(error) => {
                        panel.recovery = None;
                        panel.recovery_error = Some(message(&error));
                    }
                }
            },
        );
    }

    pub fn copy_recovery(&mut self) {
        if self.panel().is_none_or(|panel| panel.recovery.is_none()) {
            return;
        }
        let actions = self.hub().library.clone();
        self.exec().clone().run(
            self,
            move || actions.copy_recovery_key(),
            |state, outcome| {
                if let Some(panel) = state.panel() {
                    match outcome {
                        Ok(result) => panel.recovery_copied_until = Some(result.expires_at_ms),
                        Err(error) => panel.recovery_error = Some(message(&error)),
                    }
                }
            },
        );
    }

    pub(crate) fn settings_field(
        &mut self,
        pick: fn(&mut SettingsPanel) -> &mut TextEdit,
    ) -> &mut TextEdit {
        match &mut self.settings {
            Some(panel) => pick(panel),
            None => &mut self.scratch,
        }
    }
}

fn section(look: &Look, key: &str, title: &str) -> El {
    col()
        .key(key.to_owned())
        .label(title.to_owned())
        .w_full()
        .gap(size::SP2)
        .padding(size::SP3)
        .radius(size::RADIUS)
        .fill(look.panel)
        .border(1.0, look.line)
        .child(text(title.to_owned()).font(ts::STRONG).color(look.fg))
}

fn detail(look: &Look, words: &str) -> El {
    text(words.to_owned())
        .font(ts::SM)
        .color(look.muted)
        .wrap()
        .w_full()
}

fn check_row(
    look: &Look,
    key: &str,
    label: &str,
    checked: bool,
    pick: fn(&mut Settings) -> &mut bool,
) -> El {
    let mut tick = row()
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
    row()
        .key(key.to_owned())
        .role("checkbox")
        .label(label.to_owned())
        .checked(checked)
        .focusable(true)
        .focus_ring(2.0, 1.0, look.focus)
        .gap(size::SP2)
        .py(4.0)
        .center_y()
        .cursor(Cursor::Pointer)
        .on_press(move |state: &mut Library| state.settings_toggle(pick))
        .child(tick)
        .child(text(label.to_owned()).font(ts::BODY).color(look.fg))
}

fn pre_backup_row(look: &Look, checked: bool) -> El {
    let mut tick = row()
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
    row()
        .key("prebackup")
        .role("checkbox")
        .label("Back up the current data first (recommended)")
        .checked(checked)
        .focusable(true)
        .gap(size::SP2)
        .center_y()
        .cursor(Cursor::Pointer)
        .on_press(|state: &mut Library| {
            if let Some(panel) = state.settings.as_mut() {
                panel.pre_backup = !panel.pre_backup;
            }
        })
        .child(tick)
        .child(
            text("Back up the current data first (recommended)")
                .font(ts::BODY)
                .color(look.fg),
        )
}

fn options_list(
    look: &Look,
    key: &str,
    options: Vec<(String, String, bool)>,
    choose: fn(&mut Library, &str),
) -> El {
    let mut list = col()
        .key(key.to_owned())
        .role("listbox")
        .w_full()
        .gap(1.0)
        .padding(4.0);
    for (id, label, current) in options {
        let pick = id.clone();
        let mut item = row()
            .key(format!("{key}-{id}"))
            .role("option")
            .label(label.clone())
            .checked(current)
            .focusable(true)
            .w_full()
            .h(28.0)
            .px(size::SP2)
            .gap(size::SP2)
            .center_y()
            .radius(size::RADIUS_SM)
            .cursor(Cursor::Pointer)
            .hover(look.fg.alpha(0.06))
            .on_press(move |state: &mut Library| choose(state, &pick))
            .child(text(label).font(ts::BODY).color(look.fg).grow(1.0));
        if current {
            item = item.child(icon(&icons::CHECK, 13.0).color(look.accent));
        }
        list = list.child(item);
    }
    col()
        .w_full()
        .max_h(220.0)
        .radius(size::RADIUS)
        .fill(look.raised)
        .border(1.0, look.line_strong)
        .scroll(rune::ScrollSpec {
            stick_bottom: false,
        })
        .child(list)
}

pub fn view(state: &Library, panel: &SettingsPanel, look: &Look, kit: &Kit) -> El {
    let mut body = Vec::new();

    let version = state
        .app
        .as_ref()
        .map_or_else(String::new, |app| app.version.clone());
    body.push(
        section(look, "updates", "Updates")
            .child(
                text(format!("Dev Console {version}"))
                    .key("current-version")
                    .font(ts::BODY)
                    .color(look.fg),
            )
            .child(
                detail(look, "Updates are only available in the installed app.")
                    .key("update-status"),
            ),
    );

    if let Some(settings) = &panel.settings {
        let theme_label = theme::label(&settings.theme);
        let theme_button = widgets::button(
            look,
            "theme-choice",
            &if settings.theme == theme::SYSTEM {
                "Follow system".to_string()
            } else {
                theme_label
            },
            Some(&icons::CHEVRON),
            Variant::Default,
        )
        .label("Theme")
        .on_press(|state: &mut Library| {
            if let Some(panel) = state.settings.as_mut() {
                panel.choosing_theme = !panel.choosing_theme;
                panel.choosing_terminal = false;
            }
        });
        let mut general = section(look, "general", "General").child(
            row()
                .w_full()
                .gap(size::SP3)
                .child(
                    col()
                        .grow(1.0)
                        .gap(4.0)
                        .child(text("Theme").font(ts::SM).color(look.muted))
                        .child(theme_button),
                )
                .child(
                    col()
                        .w(200.0)
                        .gap(4.0)
                        .child(
                            text("Clipboard expiry (seconds)")
                                .font(ts::SM)
                                .color(look.muted),
                        )
                        .child(
                            TextInput::new(&panel.expiry)
                                .width(None)
                                .label("Clipboard expiry (seconds)")
                                .on_edit(|state: &mut Library| {
                                    state.settings_field(|panel| &mut panel.expiry)
                                })
                                .build(kit)
                                .key("clipboard-expiry"),
                        ),
                ),
        );
        if panel.choosing_theme {
            let options = theme::choices()
                .into_iter()
                .map(|(id, label)| (id.to_string(), label, id == settings.theme))
                .collect();
            general = general.child(options_list(look, "theme-options", options, |state, id| {
                state.preview_theme(id)
            }));
        }
        general = general
            .child(check_row(look, "autostart", "Start at login (hidden in the tray)", settings.autostart, |settings| &mut settings.autostart))
            .child(check_row(
                look,
                "history-suggestions-setting",
                "Suggest commands from my shell history",
                settings.history_suggestions_enabled,
                |settings| &mut settings.history_suggestions_enabled,
            ))
            .child(detail(
                look,
                "Counts useful commands by their actual project and folder while Dev Console is running. Enable the zsh integration in the history panel, then open a new terminal. No old history is assigned to a project. Turning this off stops collection and suggestions.",
            ));
        if panel.dismissed > 0 {
            general = general.child(
                widgets::button_sm(
                    look,
                    "history-suggestions-restore",
                    &format!(
                        "Bring back {} dismissed {}",
                        panel.dismissed,
                        if panel.dismissed == 1 {
                            "suggestion"
                        } else {
                            "suggestions"
                        }
                    ),
                    None,
                    Variant::Default,
                )
                .on_press(|state: &mut Library| state.restore_suggestions())
                .self_start(),
            );
        }
        let terminal_name = panel
            .terminals
            .iter()
            .find(|option| option.id == panel.terminal_choice)
            .map_or("Custom command".to_string(), |option| {
                option.name.to_string()
            });
        general = general
            .child(text("Terminal").font(ts::SM).color(look.muted))
            .child(
                widgets::button(
                    look,
                    "terminal-choice",
                    &terminal_name,
                    Some(&icons::CHEVRON),
                    Variant::Default,
                )
                .label("Terminal")
                .on_press(|state: &mut Library| {
                    if let Some(panel) = state.settings.as_mut() {
                        panel.choosing_terminal = !panel.choosing_terminal;
                        panel.choosing_theme = false;
                    }
                }),
            );
        if panel.choosing_terminal {
            let mut options: Vec<(String, String, bool)> = panel
                .terminals
                .iter()
                .map(|option| {
                    (
                        option.id.to_string(),
                        option.name.to_string(),
                        option.id == panel.terminal_choice,
                    )
                })
                .collect();
            options.push((
                "custom".into(),
                "Custom command".into(),
                panel.terminal_choice == "custom",
            ));
            general = general.child(options_list(
                look,
                "terminal-options",
                options,
                |state, id| state.choose_terminal(id),
            ));
        }
        if panel.terminals.is_empty() {
            general = general.child(detail(
                look,
                "No supported terminal was found in PATH. Enter a custom command below.",
            ));
        }
        if panel.terminal_choice == "custom" {
            general = general
                .child(
                    text("Terminal command (quotes kept)")
                        .font(ts::SM)
                        .color(look.muted),
                )
                .child(
                    TextInput::new(&panel.terminal)
                        .width(None)
                        .label("Terminal command")
                        .text_style(ts::MONO)
                        .invalid(panel.terminal_error.is_some())
                        .on_edit(|state: &mut Library| {
                            state.settings_field(|panel| &mut panel.terminal)
                        })
                        .on_change(|state: &mut Library, _| state.terminal_edited())
                        .build(kit)
                        .key("terminal"),
                );
        }
        if let Some(error) = &panel.terminal_error {
            general = general.child(widgets::status_line(
                look,
                "terminal-error",
                Tone::Error,
                error,
            ));
        } else if !panel.terminal_tokens.is_empty() {
            let mut tokens = row()
                .key("terminal-tokens")
                .gap(4.0)
                .center_y()
                .flex_wrap()
                .child(text("Runs:").font(ts::SM).color(look.muted));
            for token in &panel.terminal_tokens {
                tokens = tokens.child(
                    row()
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(size::RADIUS_SM)
                        .fill(look.raised)
                        .child(text(token.clone()).font(ts::MONO_SM).color(look.fg)),
                );
            }
            general = general.child(tokens);
        }
        general = general.child(
            detail(
                look,
                &match &panel.exclusion {
                    Some(exclusion) => format!(
                        "Clipboard history exclusion: {} - {}",
                        exclusion.mechanism, exclusion.detail
                    ),
                    None => "Checking clipboard history support…".to_string(),
                },
            )
            .key("clipboard-exclusion"),
        );
        body.push(general);
        body.push(
            section(look, "finder-settings", "Finder")
                .child(check_row(
                    look,
                    "project-context",
                    "Rank the project I was just working in first",
                    settings.project_context_enabled,
                    |settings| &mut settings.project_context_enabled,
                ))
                .child(
                    detail(
                        look,
                        "From the focused terminal window, Dev Console works out which repository the shell is in and lists its commands first, marked with a folder. Off, it never asks the compositor what is focused and never reads /proc at all.",
                    )
                    .key("project-context-detail"),
                ),
        );
    } else {
        body.push(text("Loading…").font(ts::BODY).color(look.muted));
    }

    let busy = panel.backup_busy;
    let backup_now = widgets::button_sm(look, "backup-now", "Back up now", None, Variant::Default);
    let mut backups = section(look, "backups", "Backups")
        .child(detail(
            look,
            "Archives are encrypted to two credentials: this machine's vault key and your recovery credential below. They open with either one, so a lost keyring never loses a backup. The newest 8 backups are kept; restoring or importing replaces everything and is always confirmed against the archive's own contents first.",
        ))
        .child(
            row()
                .key("backup-dir")
                .gap(4.0)
                .child(text("Backup folder:").font(ts::SM).color(look.muted))
                .child(text(if panel.backup_dir.is_empty() { "…".to_string() } else { panel.backup_dir.clone() }).font(ts::MONO_SM).color(look.fg).wrap_anywhere()),
        )
        .child(
            row()
                .w_full()
                .center_y()
                .child(if busy { widgets::disabled(backup_now) } else { backup_now.on_press(|state: &mut Library| state.back_up_now()) })
                .child(el().grow(1.0))
                .child(text(format!("{} of 8 kept", panel.backups.len())).key("backup-count").font(ts::SM).color(look.muted)),
        );
    if panel.backups.is_empty() {
        backups = backups.child(detail(look, "No backups yet.").key("backup-empty"));
    } else {
        let mut list = col().key("backup-list").w_full().gap(2.0);
        for item in &panel.backups {
            let name = item.name.clone();
            let restore = widgets::button_sm(
                look,
                format!("restore-backup-{}", item.name),
                "Review & restore",
                None,
                Variant::Default,
            );
            list = list.child(
                row()
                    .w_full()
                    .gap(size::SP2)
                    .py(2.0)
                    .center_y()
                    .child(
                        text(item.name.clone())
                            .font(ts::MONO_SM)
                            .color(look.fg)
                            .truncate()
                            .grow(1.0)
                            .min_w(0.0)
                            .help(item.name.clone()),
                    )
                    .child(
                        text(absolute_time(&ms_to_iso(item.modified_at_ms)))
                            .font(ts::SM)
                            .color(look.muted),
                    )
                    .child(
                        text(format!("{} KB", (item.size as f64 / 1024.0).round()))
                            .font(ts::SM)
                            .color(look.muted),
                    )
                    .child(if busy {
                        widgets::disabled(restore)
                    } else {
                        restore
                            .on_press(move |state: &mut Library| state.review_backup(name.clone()))
                    }),
            );
        }
        backups = backups.child(list);
    }
    body.push(backups);

    let export = widgets::button_sm(look, "export-archive", "Export", None, Variant::Default);
    let mut export_section = section(look, "export", "Export a portable archive")
        .child(detail(look, "Writes one new encrypted file containing commands and secrets. An existing file is never overwritten without asking first."))
        .child(
            row()
                .w_full()
                .gap(size::SP2)
                .center_y()
                .child(
                    el().grow(1.0).min_w(0.0).child(
                        TextInput::new(&panel.export_path).width(None)
                            .label("Export destination file")
                            .placeholder("/path/to/dev-console-export.age")
                            .text_style(ts::MONO)
                            .on_edit(|state: &mut Library| state.settings_field(|panel| &mut panel.export_path))
                            .build(kit)
                            .key("archive-path"),
                    ),
                )
                .child(if busy || panel.export_path.text.trim().is_empty() { widgets::disabled(export) } else { export.on_press(|state: &mut Library| state.export_archive(false)) }),
        );
    if let Some(path) = &panel.export_confirm {
        export_section = export_section.child(
            col()
                .key("export-overwrite-card")
                .role("alertdialog")
                .label("Confirm replacing the existing export")
                .w_full()
                .gap(size::SP2)
                .padding(size::SP3)
                .radius(size::RADIUS)
                .fill(look.warn_soft)
                .border(1.0, look.warn)
                .child(
                    text(format!("{path} already exists. Nothing has been changed."))
                        .font(ts::SM)
                        .color(look.fg)
                        .wrap(),
                )
                .child(
                    row()
                        .w_full()
                        .gap(size::SP2)
                        .child(el().grow(1.0))
                        .child(
                            widgets::button_sm(
                                look,
                                "export-overwrite-cancel",
                                "Keep both",
                                None,
                                Variant::Default,
                            )
                            .on_press(|state: &mut Library| {
                                if let Some(panel) = state.settings.as_mut() {
                                    panel.export_confirm = None;
                                }
                            }),
                        )
                        .child(
                            widgets::button_sm(
                                look,
                                "export-overwrite-confirm",
                                "Replace it",
                                None,
                                Variant::Danger,
                            )
                            .on_press(|state: &mut Library| state.export_archive(true)),
                        ),
                ),
        );
    }
    body.push(export_section);

    let review_button = widgets::button_sm(
        look,
        "import-archive",
        "Review & import",
        None,
        Variant::Default,
    );
    body.push(
        section(look, "import", "Import an archive file")
            .child(detail(
                look,
                "Restore a file you exported or copied from another machine (its recovery credential must match). You will see what it contains and confirm before anything is replaced.",
            ))
            .child(
                row()
                    .w_full()
                    .gap(size::SP2)
                    .center_y()
                    .child(
                        el().grow(1.0).min_w(0.0).child(
                            TextInput::new(&panel.import_path).width(None)
                                .label("Archive file to import")
                                .placeholder("/path/to/dev-console-export.age")
                                .text_style(ts::MONO)
                                .on_edit(|state: &mut Library| state.settings_field(|panel| &mut panel.import_path))
                                .build(kit)
                                .key("import-path"),
                        ),
                    )
                    .child(if busy || panel.import_path.text.trim().is_empty() { widgets::disabled(review_button) } else { review_button.on_press(|state: &mut Library| state.review_import()) }),
            ),
    );

    let mut recovery = section(look, "recovery", "Recovery credential").child(detail(
        look,
        "If this machine's keyring is lost or you move to a new computer, this credential is the only way to open existing backups and exports. Store a copy somewhere separate from this machine. Anyone who holds it can read every archive you create from now on.",
    ));
    match &panel.recovery {
        Some(credential) => {
            let copied = panel
                .recovery_copied_until
                .is_some_and(|until| until > now_ms());
            recovery = recovery
                .child(
                    row()
                        .w_full()
                        .padding(size::SP2)
                        .radius(size::RADIUS)
                        .fill(look.sunken)
                        .border(1.0, look.line)
                        .child(text(credential.secret.clone()).key("recovery-secret").transition(Transition::none()).font(ts::MONO_SM).color(look.fg).wrap_anywhere().grow(1.0)),
                )
                .child(detail(look, &format!("Saved at {}", credential.path)))
                .child(
                    row()
                        .w_full()
                        .gap(size::SP2)
                        .center_y()
                        .child(widgets::button_sm(look, "recovery-copy", if copied { "Copied" } else { "Copy to clipboard" }, Some(&icons::COPY), Variant::Default).on_press(|state: &mut Library| state.copy_recovery()))
                        .child(
                            detail(
                                look,
                                if copied {
                                    "Copied through the app: it clears with the clipboard expiry, and as soon as the vault locks."
                                } else {
                                    "Copies through the app, with the same expiry as a saved secret."
                                },
                            )
                            .key("recovery-copy-note")
                            .grow(1.0),
                        ),
                )
                .child(
                    detail(
                        look,
                        "On a fresh install: unlock the (empty) vault once, put this credential in ~/.config/dev-console/recovery.agekey (mode 0600), restart Dev Console, then use Import above.",
                    )
                    .key("recovery-steps"),
                );
        }
        None => {
            recovery = recovery.child(
                row()
                    .w_full()
                    .gap(size::SP2)
                    .center_y()
                    .child(
                        widgets::button_sm(
                            look,
                            "recovery-reveal",
                            "Show recovery credential",
                            Some(&icons::REVEAL),
                            Variant::Default,
                        )
                        .on_press(|state: &mut Library| state.reveal_recovery()),
                    )
                    .child(
                        detail(look, "Hidden by default - it decrypts every archive.").grow(1.0),
                    ),
            );
        }
    }
    if let Some(error) = &panel.recovery_error {
        recovery = recovery.child(widgets::status_line(
            look,
            "recovery-error",
            Tone::Error,
            error,
        ));
    }
    body.push(recovery);

    if let Some((kind, summary)) = &panel.review {
        let verb = if *kind == ReviewKind::Backup {
            "Restore"
        } else {
            "Import"
        };
        let current = format!(
            "{} commands · {}",
            panel.current_commands,
            panel.current_secrets.map_or(
                "secret count unavailable while the vault is locked".to_string(),
                |secrets| format!("{secrets} secrets")
            )
        );
        let fact = |label: &str, value: String| {
            row()
                .w_full()
                .gap(size::SP2)
                .child(
                    text(label.to_owned())
                        .font(ts::SM)
                        .color(look.muted)
                        .w(90.0),
                )
                .child(
                    text(value)
                        .font(ts::SM)
                        .color(look.fg)
                        .wrap_anywhere()
                        .grow(1.0),
                )
        };
        body.push(
            col()
                .key("review-card")
                .role("alertdialog")
                .label("Confirm replacing the library")
                .w_full()
                .gap(size::SP2)
                .padding(size::SP3)
                .radius(size::RADIUS)
                .fill(look.danger_soft)
                .border(1.0, look.danger)
                .child(text(format!("{verb} this archive?")).key("review-title").font(ts::STRONG).color(look.fg))
                .child(fact("Created", absolute_time(&summary.created_at)))
                .child(fact("Contains", format!("{} commands · {} secrets", summary.commands, summary.secrets)))
                .child(fact("Currently", current).key("review-current"))
                .child(fact("File", summary.source.clone()))
                .child(
                    text("This replaces your entire library: every command and secret above is swapped for the archive's contents. Entries that only exist here are gone afterwards.")
                        .key("review-warning")
                        .font(ts::SM)
                        .color(look.danger)
                        .wrap(),
                )
                .child(pre_backup_row(look, panel.pre_backup))
                .child(
                    row()
                        .w_full()
                        .gap(size::SP2)
                        .child(el().grow(1.0))
                        .child(widgets::button_sm(look, "review-cancel", "Cancel", None, Variant::Default).on_press(|state: &mut Library| state.cancel_review()))
                        .child(widgets::button_sm(look, "review-confirm", &format!("{verb} and replace"), None, Variant::Danger).on_press(|state: &mut Library| state.confirm_apply())),
                ),
        );
    }
    if panel.backup_busy {
        body.push(
            text("Working…")
                .role("status")
                .font(ts::SM)
                .color(look.muted),
        );
    }
    if let Some(message) = &panel.backup_message {
        body.push(widgets::status_line(
            look,
            "backup-message",
            Tone::Ok,
            message,
        ));
    }
    if let Some(error) = &panel.backup_error {
        body.push(widgets::status_line(
            look,
            "backup-error",
            Tone::Error,
            error,
        ));
    }
    if let Some(error) = &panel.error {
        body.push(widgets::status_line(
            look,
            "settings-error",
            Tone::Error,
            error,
        ));
    }

    let save = widgets::button(
        look,
        "save-settings",
        if panel.saving { "Saving…" } else { "Save" },
        None,
        Variant::Primary,
    );
    dialogs::shell(
        look,
        "settings-panel",
        "Settings",
        true,
        body,
        vec![
            widgets::button(look, "close-settings", "Close", None, Variant::Default)
                .on_press(|state: &mut Library| state.close_settings()),
            el().grow(1.0),
            if panel.saving || panel.terminal_error.is_some() || panel.settings.is_none() {
                widgets::disabled(save)
            } else {
                save.on_press(|state: &mut Library| state.save_settings())
            },
        ],
        |state: &mut Library| state.close_settings(),
    )
    // The panel can hold the recovery credential, so it leaves in the frame it is closed.
    .transition(Transition::none())
}
