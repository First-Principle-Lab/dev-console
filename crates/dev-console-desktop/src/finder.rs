//! The Finder palette: the native port of `src/windows/Finder.svelte`.
//!
//! Every appearance starts from an empty query and a fresh search. Typing searches through
//! `FinderActions::search_entries`; Up/Down move the highlight with the shipped `nextSelection`
//! rule (`dev_console_core::finder_state`), Tab accepts the inline completion, Enter copies the
//! highlighted row through the core clipboard service (which owns expiry and lock clearing) and
//! hides the palette, Ctrl+O opens the row in the Library, Ctrl+P ignores the detected project for
//! this open, and Escape clears and hides.
//!
//! A search answer is applied only if it answers the newest request, and a locked vault's answer
//! never brings back secret rows: the same request-id and vault-generation guards `runSearch` has.

use dev_console_core::context::FinderContext;
use dev_console_core::finder_state::{displayed_query, next_selection, selected, tab_completion};
use dev_console_core::models::{
    CopyRequest, Kind, SearchRequest, SearchResponse, SearchResult, VaultState,
};
use dev_console_core::services::{AppEvent, Effects};
use dev_console_core::Surface;
use rune::{
    col, el, icon, row, text, text_input, Chord, Cursor, Cx, El, Key, KeyEvent, ScrollSpec,
    TextEdit, Tx,
};
use std::sync::LazyLock;

use crate::hub::{message, Exec, Hub};
use crate::icons;
use crate::theme::{size, text as ts, Look};
use crate::widgets;

/// The id of the synthetic "unlock the vault" row the palette shows while the vault is locked.
pub const UNLOCK_ID: &str = "__unlock_vault__";

/// The status line after a copy. The value itself never appears: only what happened.
pub const COPIED_COMMAND: &str = "Copied the command to the clipboard.";
pub const COPIED_SECRET: &str = "Copied the secret to the clipboard.";
pub const COPY_FAILED: &str = "Copy failed. Nothing was copied.";

static OPEN_DETAILS: LazyLock<Chord> =
    LazyLock::new(|| Chord::parse("ctrl+o").expect("valid chord"));
static IGNORE_CONTEXT: LazyLock<Chord> =
    LazyLock::new(|| Chord::parse("ctrl+p").expect("valid chord"));

pub struct Finder {
    hub: Hub,
    exec: Exec<Finder>,
    pub query: TextEdit,
    pub results: Vec<SearchResult>,
    pub selected_id: Option<String>,
    pub vault: Option<VaultState>,
    pub context: Option<FinderContext>,
    pub error: Option<String>,
    pub status: Option<String>,
    pub loading: bool,
    /// The theme setting, re-read on every appearance and on `theme-changed`.
    pub theme: String,
    request_id: u64,
}

impl std::fmt::Debug for Finder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Finder")
            .field("query", &self.query.text)
            .field("results", &self.results.len())
            .field("selected_id", &self.selected_id)
            .field("context", &self.context)
            .field("error", &self.error)
            .field("status", &self.status)
            .finish()
    }
}

impl Finder {
    pub fn new(hub: Hub, exec: Exec<Finder>) -> Finder {
        let info = hub.finder.app_state();
        let theme = hub.finder.get_theme();
        let mut query = TextEdit::new(String::new());
        query.focused = true;
        let mut finder = Finder {
            hub,
            exec,
            query,
            results: Vec::new(),
            selected_id: None,
            vault: Some(info.vault),
            context: info.finder_context,
            error: None,
            status: None,
            loading: false,
            theme,
            request_id: 0,
        };
        finder.search();
        finder
    }

    /// The rows on screen: the results, plus the unlock prompt while the vault is locked.
    pub fn rows(&self) -> Vec<SearchResult> {
        let mut rows = self.results.clone();
        if self.vault.as_ref().is_some_and(|vault| !vault.unlocked) {
            rows.push(SearchResult {
                id: UNLOCK_ID.into(),
                kind: Kind::Command,
                name: "Unlock vault to search secrets".into(),
                entry_type: "System".into(),
                tags: Vec::new(),
                description: String::new(),
                completion: None,
                pinned: false,
                usage_count: 0,
                last_used_at: None,
                in_context: false,
            });
        }
        rows
    }

    pub fn current(&self) -> Option<SearchResult> {
        let rows = self.rows();
        selected(&rows, self.selected_id.as_deref()).cloned()
    }

    /// Searches for the current query. Only the newest request's answer is applied.
    pub fn search(&mut self) {
        self.request_id += 1;
        let id = self.request_id;
        let query = self.query.text.clone();
        self.loading = true;
        self.error = None;
        // A "Copied" line belongs to the query it happened for.
        self.status = None;
        let finder = self.hub.finder.clone();
        self.exec.clone().run(
            self,
            move || {
                finder.search_entries(SearchRequest {
                    query,
                    request_id: id,
                    limit: None,
                })
            },
            |state, response| state.apply_search(response),
        );
    }

    /// Paints a search answer, unless a newer search has been asked since or the answer was read
    /// from a vault that has locked since: the shipped `runSearch` guards.
    pub fn apply_search(&mut self, response: SearchResponse) {
        if response.request_id != self.request_id {
            return;
        }
        self.loading = false;
        let generation = self
            .vault
            .as_ref()
            .map_or(response.vault_generation, |vault| vault.generation);
        if response.vault_generation < generation
            && self.vault.as_ref().is_some_and(|vault| !vault.unlocked)
        {
            return;
        }
        self.results = response.results;
        self.selected_id =
            next_selection(&self.results, self.selected_id.as_deref(), 0).map(str::to_owned);
    }

    /// The id the next answer must carry to be painted.
    pub fn request_id(&self) -> u64 {
        self.request_id
    }

    /// Moves the highlight by `delta`, clamping at both ends.
    pub fn move_selection(&mut self, delta: i64, tx: Option<&mut Tx>) {
        let rows = self.rows();
        self.selected_id =
            next_selection(&rows, self.selected_id.as_deref(), delta).map(str::to_owned);
        if let (Some(tx), Some(id)) = (tx, self.selected_id.as_deref()) {
            tx.scroll_into_view(row_key(id));
        }
    }

    /// What the field shows: the query plus the greyed completion of the highlighted row.
    pub fn completion_tail(&self) -> String {
        let current = self.current();
        match displayed_query(&self.query.text, current.as_ref()) {
            Some(full)
                if full.len() > self.query.text.len() && full.starts_with(&self.query.text) =>
            {
                full[self.query.text.len()..].to_owned()
            }
            _ => String::new(),
        }
    }

    /// Tab: replace the query with the highlighted row's name and keep that row highlighted.
    pub fn accept_completion(&mut self) -> bool {
        let current = self.current();
        let Some(completed) = tab_completion(&self.query.text, current.as_ref()) else {
            return false;
        };
        let keep = current.map(|row| row.id);
        self.query.replace_text(completed);
        self.search();
        if let Some(keep) = keep {
            if self.results.iter().any(|row| row.id == keep) {
                self.selected_id = Some(keep);
            }
        }
        true
    }

    /// Enter: copy the highlighted row, or unlock when it is the unlock prompt.
    pub fn activate(&mut self) {
        let Some(entry) = self.current() else {
            return;
        };
        if entry.id == UNLOCK_ID {
            self.unlock();
            return;
        }
        let finder = self.hub.finder.clone();
        let hub = self.hub.clone();
        let request = CopyRequest {
            kind: entry.kind,
            id: entry.id.clone(),
            expected_revision: 0,
        };
        self.exec.clone().run(
            self,
            move || finder.copy_entry(request),
            move |state, outcome| match outcome {
                Ok(_) => {
                    state.status = Some(
                        if entry.kind == Kind::Secret {
                            COPIED_SECRET
                        } else {
                            COPIED_COMMAND
                        }
                        .into(),
                    );
                    state.error = None;
                    hub.perform(state.hub.finder.hide_self(Surface::Finder));
                }
                Err(error) => {
                    let text = message(&error);
                    state.error = Some(if text.is_empty() {
                        COPY_FAILED.into()
                    } else {
                        text
                    });
                }
            },
        );
    }

    /// Ctrl+O: show the highlighted row in the Library, then step out of the way.
    pub fn open_details(&mut self) {
        let Some(entry) = self.current().filter(|entry| entry.id != UNLOCK_ID) else {
            return;
        };
        match self
            .hub
            .finder
            .open_library(Surface::Finder, entry.kind, Some(entry.id))
        {
            Ok(command) => {
                self.hub.perform(command);
                self.hub.perform(self.hub.finder.hide_self(Surface::Finder));
            }
            Err(error) => self.error = Some(message(&error)),
        }
    }

    /// Ctrl+P, or a press on the chip: rank everything, not just this project's, for this open.
    pub fn ignore_context(&mut self) {
        if self.context.is_none() {
            return;
        }
        self.context = None;
        self.selected_id = None;
        let _ = self.hub.finder.clear_finder_context();
        self.search();
    }

    /// Escape: clear and hide.
    pub fn dismiss(&mut self) {
        self.request_id += 1;
        self.query.clear();
        self.results.clear();
        self.selected_id = None;
        self.error = None;
        self.loading = false;
        self.hub.perform(self.hub.finder.hide_self(Surface::Finder));
    }

    /// Unlocks through the system's authentication prompt, then searches again.
    pub fn unlock(&mut self) {
        let finder = self.hub.finder.clone();
        let hub = self.hub.clone();
        self.exec.clone().run(
            self,
            move || {
                let mut effects = Effects::default();
                let outcome = finder.unlock_vault(&mut effects);
                (outcome, effects)
            },
            move |state, (outcome, effects)| {
                match outcome {
                    Ok(vault) => {
                        state.vault = Some(vault);
                        state.search();
                    }
                    Err(error) => state.error = Some(message(&error)),
                }
                hub.dispatch(effects);
            },
        );
    }

    /// Every appearance starts again: empty query, fresh search, focus in the field.
    pub fn reset(&mut self) {
        self.query.clear();
        self.query.focused = true;
        self.error = None;
        self.status = None;
        self.theme = self.hub.finder.get_theme();
        self.search();
    }

    /// The field's key bindings, the port of the shipped `onKeydown`.
    pub fn route_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        match ev.key {
            Key::Down => self.move_selection(1, Some(tx)),
            Key::Up => self.move_selection(-1, Some(tx)),
            Key::Tab => {
                self.accept_completion();
            }
            Key::Enter => self.activate(),
            Key::Escape => self.dismiss(),
            _ if OPEN_DETAILS.matches(ev) => self.open_details(),
            _ if IGNORE_CONTEXT.matches(ev) && self.context.is_some() => self.ignore_context(),
            _ => return false,
        }
        true
    }

    pub fn on_event(&mut self, event: &AppEvent, _tx: &mut Tx) {
        match event {
            AppEvent::FinderShown => self.reset(),
            AppEvent::VaultStateChanged(vault) => {
                let was_locked = self.vault.as_ref().is_some_and(|old| !old.unlocked);
                self.vault = Some(vault.clone());
                if was_locked && vault.unlocked {
                    self.search();
                } else if !vault.unlocked {
                    self.results.retain(|row| row.kind != Kind::Secret);
                    if self.current().is_none() {
                        self.selected_id = self.rows().first().map(|row| row.id.clone());
                    }
                }
            }
            AppEvent::EntriesChanged(_) => self.search(),
            AppEvent::FinderContext(context) => {
                if *context == self.context {
                    return;
                }
                self.context = context.clone();
                // The project's commands now lead, so the selection starts again at the top.
                self.selected_id = None;
                self.search();
            }
            AppEvent::ThemeChanged(theme) => self.theme = theme.clone(),
            _ => {}
        }
    }
}

fn row_key(id: &str) -> String {
    format!("finder-row-{id}")
}

/// Draws the palette.
pub fn finder_view(state: &Finder, cx: &mut Cx<Finder>) -> El {
    let look = Look::for_setting(&state.theme, cx.system_dark());
    let kit = look.kit();
    let _ = kit;
    col()
        .key("finder")
        .w_full()
        .h_full()
        .fill(look.shell)
        .child(search_bar(state, &look))
        .child(el().h(1.0).w_full().fill(look.line_hair))
        .child(results(state, &look))
        .children(feedback(state, &look))
        .child(el().h(1.0).w_full().fill(look.line_hair))
        .child(footer(state, &look))
}

fn search_bar(state: &Finder, look: &Look) -> El {
    let field = text_input(&state.query, ts::SEARCH)
        .key("finder-input")
        .single_line()
        .w_full()
        .role("combobox")
        .label("Search commands and secrets")
        .color(look.fg)
        .placeholder("Search commands and secrets…", look.placeholder)
        .on_edit(|state: &mut Finder| &mut state.query)
        .on_input_change(|state: &mut Finder, _| state.search())
        .on_input_key(|state: &mut Finder, ev: &KeyEvent, tx: &mut Tx| state.route_key(ev, tx));
    let tail = state.completion_tail();
    // The completion is drawn behind the field: the typed text, invisible, then the rest of the
    // highlighted row's name in the dim colour, so the tail starts exactly where the caret is.
    let ghost = row()
        .absolute(0.0, 0.0)
        .h_full()
        .center_y()
        .inert()
        .child(
            text(state.query.text.clone())
                .font(ts::SEARCH)
                .color(rune::Color::TRANSPARENT),
        )
        .child(
            text(tail)
                .key("finder-completion")
                .font(ts::SEARCH)
                .color(look.dim),
        );
    let mut bar = row()
        .key("finder-search")
        .w_full()
        .h(size::SEARCH_H + 2.0)
        .px(size::SP3 + 2.0)
        .gap(size::SP2)
        .center_y()
        .child(icon(&icons::SEARCH, 16.0).color(look.muted))
        .child(
            el().grow(1.0)
                .min_w(0.0)
                .h_full()
                .center_y()
                .child(ghost)
                .child(field),
        );
    if let Some(context) = &state.context {
        bar = bar.child(
            row()
                .key("finder-context")
                .role("button")
                .label(format!(
                    "Ranking the project {} first. Activate to ignore it.",
                    context.name
                ))
                .help(format!(
                    "{} — its commands are ranked first. Click, or press Ctrl+P, to ignore that.",
                    context.name
                ))
                .h(22.0)
                .px(size::SP2)
                .gap(4.0)
                .center_y()
                .radius(999.0)
                .fill(look.accent_soft)
                .border(1.0, look.accent_line)
                .cursor(Cursor::Pointer)
                .on_press(|state: &mut Finder| state.ignore_context())
                .child(icon(&icons::PROJECT, 12.0).color(look.accent))
                .child(
                    text(context.name.clone())
                        .font(ts::XS)
                        .color(look.accent)
                        .truncate()
                        .max_w(160.0),
                ),
        );
    }
    if let Some(vault) = &state.vault {
        let (ink, fill, glyph, label) = if vault.unlocked {
            (look.ok, look.ok_soft, &icons::UNLOCK, "Vault unlocked")
        } else {
            (look.warn, look.warn_soft, &icons::LOCK, "Vault locked")
        };
        bar = bar.child(
            row()
                .key("finder-vault")
                .h(22.0)
                .px(size::SP2)
                .gap(4.0)
                .center_y()
                .radius(999.0)
                .fill(fill)
                .child(icon(glyph, 12.0).color(ink))
                .child(text(label).font(ts::XS).color(ink)),
        );
    }
    bar
}

fn results(state: &Finder, look: &Look) -> El {
    let rows = state.rows();
    let mut list = col()
        .key("finder-results")
        .role("listbox")
        .label("Results")
        .w_full()
        .px(size::SP2)
        .py(size::SP2)
        .gap(2.0);
    if rows.is_empty() {
        let (key, line) = if state.loading {
            ("finder-searching", "Searching…".to_string())
        } else if !state.query.text.is_empty() {
            (
                "finder-empty",
                format!(
                    "No match for {} — try a tag, a type, or part of the name",
                    state.query.text
                ),
            )
        } else {
            ("finder-nothing", "Nothing saved yet".to_string())
        };
        list = list.child(
            row()
                .key(key)
                .h(size::ROW_H)
                .px(size::SP3)
                .gap(size::SP2)
                .center_y()
                .child(icon(&icons::SEARCH, 14.0).color(look.dim))
                .child(text(line).font(ts::SM).color(look.muted).truncate()),
        );
    }
    let total = rows.len();
    for (index, entry) in rows.into_iter().enumerate() {
        list = list.child(result_row(state, look, entry, index, total));
    }
    col()
        .key("finder-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(list)
}

fn result_row(state: &Finder, look: &Look, entry: SearchResult, index: usize, total: usize) -> El {
    let active = state.selected_id.as_deref() == Some(entry.id.as_str());
    let system = entry.id == UNLOCK_ID;
    let secret = entry.kind == Kind::Secret;
    let glyph = if system {
        row()
            .w(size::TILE_SM + 2.0)
            .h(size::TILE_SM + 2.0)
            .center()
            .radius(size::RADIUS)
            .fill(look.warn_soft)
            .child(icon(&icons::LOCK, 13.0).color(look.warn))
    } else {
        widgets::kind_tile(look, secret, size::TILE_SM + 2.0)
    };
    let context_line = if system {
        "Secret metadata stays encrypted until you unlock the vault.".to_string()
    } else {
        let mut parts = Vec::new();
        if !entry.entry_type.is_empty() {
            parts.push(entry.entry_type.clone());
        }
        if !entry.description.is_empty() {
            parts.push(entry.description.clone());
        }
        parts.extend(entry.tags.iter().take(2).map(|tag| format!("#{tag}")));
        parts.join("  ·  ")
    };
    let mut body = col().grow(1.0).min_w(0.0).gap(1.0).child(
        text(entry.name.clone())
            .font(ts::ROW)
            .color(if active { look.selected_fg } else { look.fg })
            .truncate(),
    );
    if !context_line.is_empty() {
        body = body.child(
            text(context_line)
                .font(ts::ROW_META)
                .color(look.muted)
                .truncate(),
        );
    }
    let mut trail = row().gap(size::SP1).center_y();
    if entry.in_context {
        let name = state
            .context
            .as_ref()
            .map_or("the current project".to_string(), |context| {
                context.name.clone()
            });
        trail = trail.child(
            row()
                .key(format!("finder-in-project-{}", entry.id))
                .role("img")
                .label(format!("in {name}"))
                .help(format!("in {name}"))
                .child(icon(&icons::PROJECT, 11.0).color(look.accent)),
        );
    }
    if active {
        trail = trail.child(widgets::keycap(look, "Enter"));
    }
    let id = entry.id.clone();
    let mut item = row()
        .key(row_key(&entry.id))
        .role("option")
        .label(entry.name.clone())
        .position_in_set(index + 1, total)
        .w_full()
        .min_h(size::ROW_H + 4.0)
        .px(size::SP2)
        .py(5.0)
        .gap(size::SP2 + 2.0)
        .center_y()
        .radius(size::RADIUS_ROW)
        .cursor(Cursor::Pointer)
        .hover(look.fg.alpha(0.04))
        .on_press(move |state: &mut Finder| state.selected_id = Some(id.clone()))
        .on_double_press(|state: &mut Finder| state.activate())
        .child(glyph)
        .child(body)
        .child(trail);
    if active {
        item = item.fill(look.selected).border(1.0, look.selected_edge);
    }
    item
}

fn feedback(state: &Finder, look: &Look) -> Option<El> {
    if let Some(error) = &state.error {
        return Some(
            row()
                .px(size::SP3)
                .pb(size::SP2)
                .child(widgets::status_line(
                    look,
                    "finder-error",
                    widgets::Tone::Error,
                    error,
                )),
        );
    }
    state.status.as_ref().map(|status| {
        row()
            .px(size::SP3)
            .pb(size::SP2)
            .child(widgets::status_line(
                look,
                "finder-status",
                widgets::Tone::Ok,
                status,
            ))
    })
}

fn footer(state: &Finder, look: &Look) -> El {
    row()
        .key("finder-footer")
        .w_full()
        .h(size::FOOTER_H)
        .px(size::SP3)
        .gap(size::SP3)
        .center_y()
        .fill(look.panel)
        .child(widgets::hint(look, &["Up", "Down"], "select"))
        .child(widgets::hint(look, &["Tab"], "complete"))
        .child(widgets::hint(look, &["Enter"], "copy"))
        .child(widgets::hint(look, &["Ctrl", "O"], "details"))
        .child(widgets::hint(look, &["Esc"], "dismiss"))
        .child(el().grow(1.0))
        .child(
            text(state.rows().len().to_string())
                .key("finder-count")
                .font(ts::XS)
                .color(look.muted),
        )
}
