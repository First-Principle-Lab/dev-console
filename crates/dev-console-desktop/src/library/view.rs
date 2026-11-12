//! The Library window's layout: a sidebar (the Commands/Secrets switch, the filter, the grouped
//! list, and the vault and rare actions at its foot), the detail pane with its action bar, and the
//! dialogs over them. The behaviour is that of `Library.svelte`, `EntryList.svelte`,
//! `EntryDetails.svelte` and `VaultStatus.svelte`; the layout is the native shell's own
//! (`docs/design/ui-revamp-scorecard.md`).

use dev_console_core::models::{EntryDetails, EntryMeta, FieldDisplay, Kind};
use dev_console_core::time_format::{absolute_time, relative_time_now};
use dev_console_core::validation::is_openable_url;
use rune::{col, el, icon, row, text, Cursor, Cx, Drag, El, Key, Phase, ScrollSpec, Transition};
use rune_kit::{Kit, Menu, MenuItem, Placement, TextInput};

use super::{dialogs, editor, environment, history, import, settings, Library};
use crate::icons;
use crate::theme::{size, text as ts, Look};
use crate::widgets::{self, Tone, Variant};

pub fn library_view(state: &Library, cx: &mut Cx<Library>) -> El {
    let look = Look::for_setting(&state.theme, cx.system_dark());
    let kit = look.kit();
    let mut layer = Vec::new();
    let editing = state.editing.is_some();
    let has_selection = state.selected_id.is_some() || state.selected_suggestion.is_some();
    let show_list = state.usage.open || !state.narrow || (!editing && !has_selection);
    let show_detail = state.usage.open || !state.narrow || has_selection || editing;
    // The panes are swapped and resized, never animated: a window the compositor resizes as it
    // maps must not open on a cross-fade between its two layouts.
    let mut panes = row()
        .key("panes")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .instant_changes();
    if show_list {
        panes = panes.child(sidebar(state, &look, &kit));
        if !state.narrow {
            panes = panes.child(sidebar_divider(&look));
        }
    }
    if show_detail {
        panes = panes.child(
            col()
                .key("detail-pane")
                .label("Entry details")
                .grow(1.0)
                .min_w(0.0)
                .h_full()
                .min_h(0.0)
                // What the pane shows is replaced, never morphed: the old entry must not be
                // legible through the new one for a third of a second.
                .instant_changes()
                .child(detail_body(state, &look, &kit, &mut layer)),
        );
    }
    let mut root = col()
        .key("library")
        .w_full()
        .h_full()
        .fill(look.content())
        .on_layout(|state: &mut Library, rect: [f32; 4]| state.set_width(rect[2]))
        .child(panes)
        .children(status(state, &look))
        .children(layer);
    // Drawn last so each is on top of what came before; `modal` makes everything under it inert.
    for overlay in [
        state
            .pending_delete
            .as_ref()
            .map(|target| dialogs::delete(target, &look)),
        state
            .pending_run
            .as_ref()
            .map(|target| dialogs::run(target, &look)),
        state
            .run_output
            .as_ref()
            .map(|output| dialogs::output(state, output, &look)),
        state.pending_close.then(|| dialogs::unsaved(&look)),
        state
            .import
            .as_ref()
            .map(|panel| import::view(panel, &look, &kit)),
        state
            .environment
            .as_ref()
            .map(|panel| environment::view(state, panel, &look)),
        state
            .settings
            .as_ref()
            .map(|panel| settings::view(state, panel, &look, &kit)),
    ]
    .into_iter()
    .flatten()
    {
        root = root.child(overlay);
    }
    root
}

fn sidebar_divider(look: &Look) -> El {
    row()
        .key("sidebar-resize")
        .role("separator")
        .label("Resize sidebar")
        .help("Drag to resize. Left and Right adjust width; Home resets it.")
        .focusable(true)
        .focus_ring(2.0, 0.0, look.focus)
        .cursor(Cursor::ResizeColumn)
        .w(6.0)
        .h_full()
        .fill(look.sidebar())
        .hover(look.accent_soft)
        .child(el().w(1.0).h_full().fill(look.line_strong))
        .on_drag(|state: &mut Library, drag: &mut Drag, tx| {
            tx.instant();
            match drag.phase {
                Phase::Down => drag.data = state.sidebar_width(),
                Phase::Move => state.resize_sidebar(drag.data + drag.dx),
                Phase::Up if drag.cancelled() => state.resize_sidebar(drag.data),
                Phase::Up => {}
            }
        })
        .on_key(|state: &mut Library, ev, tx| {
            let width = match ev.key {
                Key::Left => state.sidebar_width() - 16.0,
                Key::Right => state.sidebar_width() + 16.0,
                Key::Home => size::PANE_LIST,
                _ => return false,
            };
            tx.instant();
            state.resize_sidebar(width);
            true
        })
}

/// The left column: what kind of thing, the filter, the list, and at the bottom the vault and
/// the three things done rarely enough to be icons.
fn sidebar(state: &Library, look: &Look, kit: &Kit) -> El {
    let mut side = col()
        .key("list-pane")
        .label("Entry list")
        .h_full()
        .min_h(0.0)
        .fill(look.sidebar());
    side = if state.narrow {
        side.w_full()
    } else {
        side.w(state.sidebar_width())
    };
    if state.usage.open {
        // One builder for all three, so the dashboard's sidebar reads as a single list.
        return side
            .w(180.0)
            .child(
                col()
                    .key("usage-nav")
                    .role("tablist")
                    .w_full()
                    .padding(size::SP3)
                    .gap(size::SP1)
                    .child(
                        nav_item(look, "tab-commands", "Commands", &icons::COMMAND, false)
                            .on_press(|s: &mut Library| {
                                s.select_tab(Kind::Command);
                            }),
                    )
                    .child(
                        nav_item(look, "tab-secrets", "Secrets", &icons::SECRET, false).on_press(
                            |s: &mut Library| {
                                s.select_tab(Kind::Secret);
                            },
                        ),
                    )
                    .child(
                        nav_item(look, "tab-cliproxy", "CLI Proxy", &icons::HISTORY, true)
                            .on_press(|s: &mut Library| s.open_usage()),
                    ),
            )
            .child(el().grow(1.0))
            .child(sidebar_footer(state, look));
    }
    let mut scroll = col().w_full();
    if state.tab == Kind::Command && state.history_enabled {
        scroll = scroll.child(history::view(state, look));
    }
    scroll = scroll.child(
        row()
            .key("saved-entries-heading")
            .w_full()
            .mt(size::SP1)
            .px(size::SP4)
            .pt(size::SP3)
            .pb(size::SP2)
            .gap(size::SP2)
            .center_y()
            .border_sides([1.0, 0.0, 0.0, 0.0], look.line_hair)
            .child(
                text(if state.tab == Kind::Command {
                    "Saved commands"
                } else {
                    "Saved secrets"
                })
                .font(ts::PROJECT)
                .color(look.muted),
            )
            .child(
                text(state.entries.len().to_string())
                    .font(ts::SM)
                    .color(look.muted),
            ),
    );
    scroll = scroll.child(entry_list(state, look));
    side.child(toolbar(state, look, kit))
        .child(
            col()
                .key("list-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .instant_changes()
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(scroll),
        )
        .child(sidebar_footer(state, look))
}

fn toolbar(state: &Library, look: &Look, kit: &Kit) -> El {
    let hidden = state.secret_metadata_hidden();
    let tabs = row()
        .key("tabs")
        .role("tablist")
        .focus_group()
        .w_full()
        .h(32.0)
        .padding(2.0)
        .gap(2.0)
        .radius(size::RADIUS + 1.0)
        .fill(look.fg.alpha(0.06))
        .child(tab(
            state,
            look,
            Kind::Command,
            "Commands",
            Some(state.counts[0]),
        ))
        .child(tab(
            state,
            look,
            Kind::Secret,
            "Secrets",
            (!hidden).then_some(state.counts[1]),
        ));
    let filter = TextInput::new(&state.query)
        .width(None)
        .label("Search this tab")
        .placeholder(if state.tab == Kind::Secret {
            "Filter secrets"
        } else {
            "Filter commands"
        })
        .leading_icon(&icons::SEARCH)
        .on_edit(|state: &mut Library| &mut state.query)
        .on_change(|state: &mut Library, _| state.filter_changed())
        .build(kit)
        .key("tab-search");
    let add = widgets::button(
        look,
        "add-entry",
        "New",
        Some(&icons::PLUS),
        Variant::Default,
    )
    .h(32.0)
    .help(if state.tab == Kind::Secret {
        "New secret (Ctrl+N)"
    } else {
        "New command (Ctrl+N)"
    })
    .on_press(|state: &mut Library| state.start_new());
    let toolbar = col()
        .key("toolbar")
        .w_full()
        .padding(size::SP3)
        .gap(size::SP3)
        .child(tabs)
        .child(
            row()
                .w_full()
                .gap(size::SP2)
                .center_y()
                .child(el().grow(1.0).min_w(0.0).child(filter))
                .child(add),
        );
    // The library-wide actions sit in one row on both tabs; Import switches to Commands itself.
    toolbar.child(
        row()
            .key("library-actions")
            .w_full()
            .gap(size::SP1)
            .center_y()
            .flex_wrap()
            .child(
                widgets::button_sm(
                    look,
                    "import-project",
                    "Import",
                    Some(&icons::IMPORT),
                    Variant::Ghost,
                )
                .help("Import commands from a project folder")
                .on_press(|state: &mut Library| state.start_project_import()),
            )
            .child(
                widgets::button_sm(
                    look,
                    "open-environment",
                    "Environment",
                    Some(&icons::ENVIRONMENT),
                    Variant::Ghost,
                )
                .help("Manage environment files for the selected project")
                .on_press(|state: &mut Library| {
                    let root = state
                        .meta()
                        .and_then(|meta| meta.project.as_ref())
                        .map(|project| project.root.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    state.open_environment(root);
                }),
            )
            .child(
                widgets::button_sm(
                    look,
                    "tab-cliproxy",
                    "CLI Proxy",
                    Some(&icons::HISTORY),
                    Variant::Ghost,
                )
                .role("tab")
                .checked(false)
                .help("Codex cost, tokens and limits")
                .on_press(|s: &mut Library| s.open_usage()),
            ),
    )
}

/// A row of the usage dashboard's sidebar: icon and label, left-aligned, the selected one
/// filled like the active tab.
fn nav_item(
    look: &Look,
    key: &'static str,
    label: &'static str,
    glyph: &'static rune::Icon,
    active: bool,
) -> El {
    let ink = if active { look.fg } else { look.muted };
    let mut item = row()
        .key(key)
        .role("tab")
        .label(label)
        .checked(active)
        .focusable(true)
        .focus_selected(active)
        .focus_ring(2.0, 1.0, look.focus)
        .w_full()
        .h(32.0)
        .px(10.0)
        .gap(size::SP2)
        .center_y()
        .radius(size::RADIUS)
        .border(1.0, rune::Color::TRANSPARENT)
        .hover(look.fg.alpha(0.06))
        .cursor(Cursor::Pointer)
        .child(rune::icon(glyph, 14.0).color(ink))
        .child(
            text(label)
                .font(if active { ts::MEDIUM } else { ts::BODY })
                .color(ink),
        );
    if active {
        item = item.fill(look.accent_soft).border(1.0, look.accent_line);
    }
    item
}

fn tab(state: &Library, look: &Look, kind: Kind, label: &'static str, count: Option<usize>) -> El {
    let active = state.tab == kind && !state.usage.open;
    let mut chip = row()
        .key(if kind == Kind::Command {
            "tab-commands"
        } else {
            "tab-secrets"
        })
        .role("tab")
        .label(label)
        .checked(active)
        .focusable(true)
        .focus_selected(active)
        .focus_ring(2.0, 1.0, look.focus)
        .grow(1.0)
        .w_frac(0.5)
        .shrink()
        .h_full()
        .gap(6.0)
        .center()
        .radius(size::RADIUS - 1.0)
        .cursor(Cursor::Pointer)
        .on_press_tx(move |state: &mut Library, tx| {
            tx.instant();
            state.select_tab(kind);
        })
        .child(
            text(label)
                .font(if active { ts::MEDIUM } else { ts::BODY })
                .color(if active { look.fg } else { look.muted }),
        );
    if let Some(count) = count {
        chip = chip.child(text(count.to_string()).font(ts::XS).color(look.dim));
    }
    if active {
        chip = chip.fill(look.accent_soft).border(1.0, look.accent_line);
    }
    chip
}

/// The vault's state, which is also the control that changes it, and the rare actions.
fn sidebar_footer(state: &Library, look: &Look) -> El {
    let mut bar = row()
        .key("library-header")
        .w_full()
        .h(48.0)
        .px(size::SP3)
        .gap(size::SP2)
        .center_y()
        .border_sides([1.0, 0.0, 0.0, 0.0], look.line_hair);
    if let Some(vault) = &state.vault {
        bar = bar.child(vault_status(state, vault, look));
    }
    bar = bar.child(el().grow(1.0));
    if state.authenticating {
        bar = bar.child(
            text("Waiting…")
                .key("waiting")
                .role("status")
                .label("Waiting for authentication…")
                .font(ts::SM)
                .color(look.muted),
        );
    }
    // The usage dashboard narrows the sidebar, which leaves room for the gear alone.
    let settings = if state.usage.open {
        widgets::icon_button(look, "settings", "Settings", &icons::GEAR, look.muted)
    } else {
        widgets::button_sm(
            look,
            "settings",
            "Settings",
            Some(&icons::GEAR),
            Variant::Ghost,
        )
    };
    bar.child(settings.on_press(|state: &mut Library| state.open_settings()))
}

fn vault_status(state: &Library, vault: &dev_console_core::models::VaultState, look: &Look) -> El {
    let (ink, label) = if vault.unlocked {
        (look.muted, "Vault unlocked")
    } else if vault.manual_lock {
        (look.warn, "Vault locked")
    } else if vault.vault_available {
        (look.warn, "Vault unavailable")
    } else {
        (look.danger, "Keyring unavailable")
    };
    let action = if vault.unlocked { "Lock" } else { "Unlock" };
    let toggle = row()
        .key("vault-toggle")
        .role("button")
        .label(action)
        .help(match (&vault.reason, vault.unlocked) {
            (Some(reason), _) => reason.clone(),
            (None, true) => "Lock the vault".to_owned(),
            (None, false) => "Unlock the vault".to_owned(),
        })
        .focusable(true)
        .focus_ring(2.0, 1.0, look.focus)
        .h(size::CONTROL_H)
        .px(size::SP2)
        .gap(6.0)
        .center_y()
        .radius(size::RADIUS)
        .hover(look.fg.alpha(0.08))
        .press(look.fg.alpha(0.12))
        .cursor(Cursor::Pointer)
        .child(
            icon(
                if vault.unlocked {
                    &icons::UNLOCK
                } else {
                    &icons::LOCK
                },
                14.0,
            )
            .color(ink),
        )
        .child(text(label).font(ts::SM).color(ink));
    let toggle = if state.authenticating {
        widgets::disabled(toggle)
    } else if vault.unlocked {
        toggle.on_press_tx(|state: &mut Library, tx| {
            // Nothing decrypted may fade out on its way off the screen.
            tx.instant();
            state.lock();
        })
    } else {
        toggle.on_press(|state: &mut Library| state.unlock())
    };
    row().key("vault-status").center_y().child(toggle)
}

fn status(state: &Library, look: &Look) -> Option<El> {
    if let Some(error) = state.list_error.as_ref().filter(|error| {
        !(state.secret_metadata_hidden() && error.to_lowercase().contains("locked"))
    }) {
        return Some(widgets::toast(
            look,
            "list-error",
            Tone::Error,
            error,
            error,
        ));
    }
    let status = state.status.as_ref()?;
    let ok = super::STATUS_CONFIRMATIONS
        .iter()
        .any(|prefix| status.starts_with(prefix));
    // A copied command needs only the confirmation. The sentence after it is the clipboard
    // caveat, which matters for a secret and is kept in full there (and always in Settings).
    let shown = match status.split_once(". ") {
        Some((first, _)) if ok && state.tab == Kind::Command && status.starts_with("Copied") => {
            format!("{first}.")
        }
        _ => status.clone(),
    };
    Some(widgets::toast(
        look,
        "status",
        if ok { Tone::Ok } else { Tone::Info },
        &shown,
        status,
    ))
}

fn detail_body(state: &Library, look: &Look, kit: &Kit, layer: &mut Vec<El>) -> El {
    if state.usage.open {
        return super::usage::view(state, look);
    }
    if let Some(editor) = &state.editing {
        return editor::editor_view(state, editor, look, kit);
    }
    if state.tab == Kind::Command {
        if let Some(suggestion) = state
            .selected_suggestion
            .as_ref()
            .and_then(|key| state.suggestions.iter().find(|s| &s.key == key))
        {
            return history::preview(state, suggestion, look);
        }
    }
    if let Some(details) = &state.details {
        return details_view(state, details, look, kit, layer);
    }
    if state.loading || state.selected_id.is_some() {
        return widgets::empty_state(
            look,
            "detail-loading",
            &icons::UPDATE,
            "Loading entry…",
            "",
            Vec::new(),
        );
    }
    if state.tab == Kind::Secret && state.vault.as_ref().is_some_and(|vault| !vault.unlocked) {
        let unlock = widgets::button(
            look,
            "unlock-vault",
            if state.authenticating {
                "Unlocking…"
            } else {
                "Unlock vault"
            },
            None,
            Variant::Primary,
        );
        return widgets::empty_state(
            look,
            "vault-locked",
            &icons::LOCK,
            "Vault locked",
            "Secrets stay encrypted until you unlock.",
            vec![if state.authenticating {
                widgets::disabled(unlock)
            } else {
                unlock.on_press(|state: &mut Library| state.unlock())
            }],
        );
    }
    if state.entries.is_empty() && !state.loading && state.query.text.is_empty() {
        let secret = state.tab == Kind::Secret;
        return widgets::empty_state(
            look,
            "nothing-selected",
            if secret {
                &icons::SECRET
            } else {
                &icons::COMMAND
            },
            if secret {
                "No secrets yet"
            } else {
                "No commands yet"
            },
            if secret {
                "Save a token or key once, then copy it from anywhere."
            } else {
                "Save a command once, then find it from anywhere."
            },
            vec![
                widgets::button(
                    look,
                    "empty-new",
                    if secret { "New secret" } else { "New command" },
                    None,
                    Variant::Primary,
                )
                .on_press(|state: &mut Library| state.start_new()),
                row()
                    .key("shortcut-hint")
                    .pl(size::SP2)
                    .center_y()
                    .child(widgets::hint(look, &["Super", "F"], "opens the Finder")),
            ],
        );
    }
    widgets::empty_state(
        look,
        "select-entry",
        if state.tab == Kind::Secret {
            &icons::SECRET
        } else {
            &icons::COMMAND
        },
        if state.tab == Kind::Command {
            "Your command library"
        } else {
            "Your secret library"
        },
        if state.tab == Kind::Command {
            "Select a saved command to view, copy or run it. Suggested commands come from project history; choose Save to review one before adding it to your library."
        } else {
            "Select a secret to view its details or copy it. Values stay masked until you choose Reveal."
        },
        vec![widgets::button(
            look,
            "empty-selection-new",
            if state.tab == Kind::Command {
                "New command"
            } else {
                "New secret"
            },
            Some(&icons::PLUS),
            Variant::Default,
        )
        .on_press(|state: &mut Library| state.start_new())],
    )
}

/// A project group of the command list.
struct Group {
    key: String,
    name: String,
    root: String,
    entries: Vec<EntryMeta>,
}

/// The web list's grouping: by project root, else by a `project:` tag matched to a known project,
/// else "General", which sorts last.
fn groups(entries: &[EntryMeta], kind: Kind) -> Vec<Group> {
    use std::collections::HashMap;
    let slug = |name: &str| {
        name.trim()
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
    };
    let mut known: HashMap<String, Option<(String, String)>> = HashMap::new();
    for entry in entries {
        if let Some(project) = &entry.project {
            let key = slug(&project.name);
            let root = project.root.to_string_lossy().into_owned();
            let same = match known.get(&key) {
                None => true,
                Some(Some((previous, _))) => *previous == root,
                Some(None) => false,
            };
            known.insert(key, same.then(|| (root, project.name.clone())));
        }
    }
    let mut out: Vec<Group> = Vec::new();
    for entry in entries {
        let tag = entry
            .tags
            .iter()
            .find_map(|tag| tag.strip_prefix("project:"))
            .map(str::to_owned);
        let matched = tag
            .as_ref()
            .and_then(|tag| known.get(&slug(tag)).cloned().flatten());
        let root = entry
            .project
            .as_ref()
            .map(|project| project.root.to_string_lossy().into_owned())
            .or_else(|| matched.as_ref().map(|(root, _)| root.clone()))
            .unwrap_or_default();
        let key = if kind == Kind::Secret {
            String::new()
        } else if !root.is_empty() {
            root.clone()
        } else {
            tag.as_ref()
                .map(|tag| format!("tag:{}", tag.to_lowercase()))
                .unwrap_or_default()
        };
        let name = entry
            .project
            .as_ref()
            .map(|project| project.name.clone())
            .or_else(|| matched.as_ref().map(|(_, name)| name.clone()))
            .or_else(|| tag.clone())
            .or_else(|| {
                root.trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .filter(|last| !last.is_empty())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "General".into());
        match out.iter_mut().find(|group| group.key == key) {
            Some(group) => group.entries.push(entry.clone()),
            None => out.push(Group {
                key,
                name,
                root,
                entries: vec![entry.clone()],
            }),
        }
    }
    out.sort_by(|a, b| match (a.key.is_empty(), b.key.is_empty()) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => a.name.cmp(&b.name).then_with(|| a.key.cmp(&b.key)),
    });
    out
}

fn entry_list(state: &Library, look: &Look) -> El {
    let mut list = col()
        .key("entry-list")
        .role("listbox")
        .label(if state.tab == Kind::Secret {
            "Secrets"
        } else {
            "Commands"
        })
        .w_full()
        .px(size::SP2)
        .pb(size::SP2)
        .gap(1.0)
        // A source list: Up and Down move through the rows and the detail pane follows, Home
        // and End jump, and typing a name's first letters goes to it.
        .focus_group()
        .focus_follows();
    if state.loading && state.entries.is_empty() {
        for index in 0..3 {
            list = list.child(
                el().key(format!("skeleton-{index}"))
                    .w_full()
                    .h(size::ROW_H)
                    .radius(size::RADIUS_ROW)
                    .fill(look.fg.alpha(0.04)),
            );
        }
        return list;
    }
    if state.secret_metadata_hidden() {
        return list.child(
            row()
                .key("list-locked")
                .w_full()
                .padding(size::SP3)
                .gap(size::SP2)
                .center_y()
                .child(icon(&icons::LOCK, 13.0).color(look.dim))
                .child(
                    text("Secret metadata is hidden while the vault is locked.")
                        .font(ts::SM)
                        .color(look.muted)
                        .wrap()
                        .grow(1.0),
                ),
        );
    }
    if state.entries.is_empty() {
        let hint = if !state.query.text.is_empty() {
            "Nothing matches this filter. Clear the search or add a new entry."
        } else if state.tab == Kind::Secret {
            "Add a secret and it will be encrypted with the vault key."
        } else {
            "Add a command and it will be searchable from the Finder."
        };
        return list.child(
            text(hint)
                .key("list-empty")
                .font(ts::SM)
                .color(look.muted)
                .wrap()
                .w_full()
                .px(size::SP2)
                .py(size::SP2),
        );
    }
    let filter_active = !state.query.text.trim().is_empty();
    let all = groups(&state.entries, state.tab);
    let several = all.len() > 1;
    let busy = state.draft_dirty();
    for group in all {
        let open = group.key.is_empty() || filter_active || !state.collapsed.contains(&group.key);
        let mut section = col()
            .key(format!("group-{}", group.key))
            .role("group")
            .label(group.name.clone())
            .w_full()
            .gap(1.0);
        if state.tab == Kind::Command && !group.key.is_empty() {
            let key = group.key.clone();
            let root = group.root.clone();
            let chevron = icon(&icons::CHEVRON, 11.0)
                .color(look.dim)
                .icon_rotation(if open {
                    0.0
                } else {
                    -std::f32::consts::FRAC_PI_2
                });
            let mut head = group_head(look, Some(chevron), &group.name, group.entries.len())
                .key(format!("project-toggle-{}", group.key))
                .role("button")
                .label(group.name.clone())
                .expanded(open)
                .help(if group.root.is_empty() {
                    group.name.clone()
                } else {
                    widgets::short_path(&group.root)
                })
                .focusable(true)
                .focus_ring(2.0, 1.0, look.focus)
                .cursor(Cursor::Pointer)
                .hover_group_focus()
                .on_press_tx(move |state: &mut Library, tx| {
                    tx.instant();
                    state.toggle_group(&key, &root);
                });
            if !group.root.is_empty() {
                let root = group.root.clone();
                // The project's environment files, offered where the project is named and only
                // while the pointer or the keyboard is on it.
                head = head.child(
                    widgets::icon_button(
                        look,
                        format!("project-environment-{}", group.key),
                        "Environment",
                        &icons::ENVIRONMENT,
                        look.muted,
                    )
                    .help(format!("Environment files of {}", group.name))
                    .w(size::CONTROL_H_SM)
                    .h(size::CONTROL_H_SM)
                    .group_opacity(0.0, 1.0)
                    .on_press(move |state: &mut Library| state.open_environment(root.clone())),
                );
            }
            // Its own group, so the list's arrows step over a heading instead of folding it; Tab
            // still reaches it.
            section = section.child(row().w_full().focus_group().child(head));
        } else if state.tab == Kind::Command && several {
            section = section
                .child(group_head(look, None, "General", group.entries.len()).key("general-head"));
        }
        // Rows nest one step beyond the project heading.
        let headed = state.tab == Kind::Command && (several || !group.key.is_empty());
        if open {
            for entry in group.entries {
                section = section.child(entry_row(state, look, entry, busy, headed));
            }
        }
        list = list.child(section);
    }
    list
}

/// A project heading is stronger than its rows; nesting and spacing show their relationship.
fn group_head(look: &Look, lead: Option<El>, name: &str, count: usize) -> El {
    row()
        .w_full()
        .h(size::CONTROL_H)
        .mt(size::SP4)
        .pl(size::SP2)
        .pr(2.0)
        .gap(6.0)
        .center_y()
        .radius(size::RADIUS_ROW)
        // A group with no chevron keeps the chevron's column, so every heading starts on one line.
        .child(lead.unwrap_or_else(|| el().w(11.0)))
        .child(
            text(name.to_owned())
                .font(ts::PROJECT)
                .color(look.fg)
                .truncate()
                .min_w(0.0)
                .shrink(),
        )
        .child(text(count.to_string()).font(ts::SM).color(look.muted))
        .child(el().grow(1.0))
}

fn entry_row(state: &Library, look: &Look, entry: EntryMeta, busy: bool, indent: bool) -> El {
    let selected = state.selected_id.as_deref() == Some(entry.id.as_str());
    let id = entry.id.clone();
    let copy_target = entry.clone();
    let pin_target = entry.clone();
    // Pointer controls: Tab goes from the list to the detail bar, which has the same actions
    // for the selected entry, instead of stopping at a hidden button on every row.
    let small = |button: El| {
        button
            .w(size::CONTROL_H_SM)
            .h(size::CONTROL_H_SM)
            .tab_index(-1)
    };
    let pin = small(widgets::icon_button(
        look,
        format!("row-pin-{}", entry.id),
        &format!(
            "{} {}",
            if entry.pinned { "Unpin" } else { "Pin" },
            entry.name
        ),
        &icons::PIN,
        if entry.pinned { look.muted } else { look.dim },
    ));
    let copy = small(widgets::icon_button(
        look,
        format!("row-copy-{}", entry.id),
        &format!("Copy {}", entry.name),
        &icons::COPY,
        look.muted,
    ))
    .on_press(move |state: &mut Library| state.copy_entry(Some(copy_target.clone())));
    // The row's own actions exist for the pointer and the keyboard, and are drawn only while
    // one of them is on the row: at rest a list is names, not a grid of sixty icons.
    let pinned = entry.pinned;
    let pin = if busy {
        widgets::disabled(pin)
    } else {
        pin.on_press(move |state: &mut Library| state.toggle_pin(Some(pin_target.clone())))
    };
    let actions = row()
        .pin(None, Some(14.0), Some(6.0), None)
        .gap(0.0)
        .center_y()
        // Not part of the list's arrow order: the arrows move between entries.
        .focus_group()
        .child(
            copy.fill(if selected {
                look.selected
            } else {
                look.sidebar()
            })
            .group_opacity(0.0, 1.0),
        )
        // A pinned row keeps its pin on show: it is the row's state as well as its control.
        .child(if pinned {
            pin.fill(if selected {
                look.selected
            } else {
                look.sidebar()
            })
        } else {
            pin.fill(if selected {
                look.selected
            } else {
                look.sidebar()
            })
            .group_opacity(0.0, 1.0)
        });
    let mut item = row()
        .key(format!("entry-row-{}", entry.id))
        .role("option")
        .label(entry.name.clone())
        .checked(selected)
        .focusable(true)
        .focus_selected(selected)
        .focus_ring(2.0, 1.0, look.focus)
        .hover_group_focus()
        .w_full()
        .h(size::LIBRARY_ROW_H)
        .pl(if indent { size::SP2 + 29.0 } else { size::SP2 })
        .pr(if pinned {
            size::CONTROL_H_SM + size::SP2
        } else {
            size::SP2
        })
        .gap(6.0)
        .center_y()
        .radius(size::RADIUS_ROW)
        .border_sides(
            [0.0, 0.0, 0.0, 2.0],
            if selected {
                look.accent
            } else {
                rune::Color::TRANSPARENT
            },
        )
        .cursor(Cursor::Pointer)
        .help(entry.name.clone())
        .hover(look.fg.alpha(0.05))
        .on_press_tx(move |state: &mut Library, tx| {
            tx.instant();
            state.select(&id);
        })
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(4.0)
                .center_y()
                .child(
                    text(entry.name.clone())
                        .key(format!("entry-name-{}", entry.id))
                        .font(ts::LIBRARY_ROW)
                        .color(if selected { look.selected_fg } else { look.fg })
                        .truncate()
                        .w_full(),
                )
                .child(
                    text(if !entry.description.is_empty() {
                        entry.description.clone()
                    } else if !entry.working_directory.is_empty() {
                        widgets::short_path(&entry.working_directory)
                    } else {
                        format!(
                            "{} · Updated {}",
                            entry.entry_type,
                            relative_time_now(&entry.updated_at)
                        )
                    })
                    .key(format!("entry-caption-{}", entry.id))
                    .font(ts::SM)
                    .color(look.muted)
                    .truncate()
                    .w_full(),
                ),
        );
    item = item.child(actions);
    if selected {
        item = item.fill(look.selected);
    }
    item
}

/// One line under the name: what it is and when it last changed, in words.
fn meta_line(meta: &EntryMeta, look: &Look) -> El {
    let mut parts = Vec::new();
    if let Some(project) = &meta.project {
        parts.push(project.name.clone());
    }
    parts.extend(meta.tags.iter().map(|tag| format!("#{tag}")));
    if meta.pinned {
        parts.push("Pinned".into());
    }
    parts.push(format!("Updated {}", relative_time_now(&meta.updated_at)));
    text(parts.join("  ·  "))
        .key("details-meta")
        .font(ts::SM)
        .color(look.muted)
        .wrap()
        .w_full()
        .help(format!(
            "Revision {}, {}",
            meta.revision,
            absolute_time(&meta.updated_at)
        ))
}

/// Monospaced content on the sunken plate: a command, a secret, a field value.
pub(super) fn plate(look: &Look) -> El {
    row()
        .w_full()
        .min_h(40.0)
        .px(size::SP3)
        .py(10.0)
        .gap(size::SP2)
        .center_y()
        .radius(size::RADIUS_LG - 2.0)
        .fill(look.sunken)
        .border(1.0, look.line_hair)
}

fn details_view(
    state: &Library,
    details: &EntryDetails,
    look: &Look,
    kit: &Kit,
    layer: &mut Vec<El>,
) -> El {
    let meta = &details.meta;
    let secret = state.tab == Kind::Secret;

    let mut body = col()
        .key("entry-details")
        .w_full()
        .max_w(size::MEASURE)
        .px(size::SP6)
        .pt(size::SP5)
        .pb(size::SP6)
        .gap(size::SP5)
        .child(
            col()
                .w_full()
                .gap(6.0)
                .child(
                    text(meta.name.clone())
                        .key("details-name")
                        .font(ts::DETAIL_TITLE)
                        .color(look.fg)
                        .wrap_anywhere()
                        .selectable(true),
                )
                .child(meta_line(meta, look))
                .children((!meta.description.is_empty()).then(|| {
                    text(meta.description.clone())
                        .font(ts::BODY)
                        .color(look.fg)
                        .wrap()
                        .pt(size::SP1)
                })),
        );
    if secret {
        let shown = if state.revealed {
            details.value.clone().unwrap_or_default()
        } else {
            "•".repeat(10)
        };
        body = body.child(
            col()
                .w_full()
                .gap(size::SP2)
                .child(widgets::section_label(look, "Value"))
                .child(
                    plate(look).child(
                        text(shown)
                            .key("secret-value")
                            .label(if state.revealed {
                                "Secret value"
                            } else {
                                "Secret value (masked)"
                            })
                            .transition(Transition::none())
                            .font(ts::MONO)
                            .color(if state.revealed { look.fg } else { look.muted })
                            .selectable(state.revealed)
                            .grow(1.0)
                            .wrap_anywhere(),
                    ),
                )
                .child(
                    text(
                        "A copy leaves the clipboard on its own after the expiry set in Settings.",
                    )
                    .font(ts::SM)
                    .color(look.dim)
                    .wrap(),
                ),
        );
    } else {
        let mut section = col()
            .w_full()
            .gap(size::SP2)
            .child(widgets::section_label(look, "Command"))
            .child(
                plate(look).child(
                    text(details.command.clone().unwrap_or_default())
                        .key("command-text")
                        .font(ts::MONO)
                        .color(look.fg)
                        .selectable(true)
                        .wrap_anywhere()
                        .grow(1.0),
                ),
            );
        if !details.working_directory.is_empty() {
            section = section.child(
                row()
                    .gap(6.0)
                    .center_y()
                    .help(details.working_directory.clone())
                    .child(icon(&icons::FOLDER, 12.0).color(look.dim))
                    .child(
                        text(widgets::short_path(&details.working_directory))
                            .key("working-directory")
                            .label(format!("Runs in {}", details.working_directory))
                            .font(ts::MONO_SM)
                            .color(look.muted)
                            .truncate(),
                    ),
            );
        }
        body = body.child(section);
    }
    body = body.child(primary_actions(state, look, secret));
    if !details.notes.is_empty() {
        body = body.child(
            col()
                .w_full()
                .gap(size::SP2)
                .child(widgets::section_label(look, "Notes"))
                .child(
                    text(details.notes.clone())
                        .font(ts::BODY)
                        .color(look.fg)
                        .wrap()
                        .selectable(true),
                ),
        );
    }
    if !details.fields.is_empty() {
        let mut table = col().key("extra-fields").w_full();
        for (index, field) in details.fields.iter().enumerate() {
            let value: El = if field.display == FieldDisplay::Url && is_openable_url(&field.value) {
                let url = field.value.trim().to_owned();
                text(field.value.clone())
                    .key(format!("field-url-{}", field.id))
                    .role("link")
                    .help("Open in browser")
                    .font(ts::BODY)
                    .color(look.accent)
                    .cursor(Cursor::Pointer)
                    .truncate()
                    .on_press(move |state: &mut Library| state.open_url(url.clone()))
            } else {
                text(field.value.clone())
                    .font(ts::BODY)
                    .color(look.fg)
                    .wrap_anywhere()
                    .selectable(true)
            };
            let mut line = row()
                .w_full()
                .min_h(32.0)
                .py(6.0)
                .gap(size::SP3)
                .child(
                    text(if field.label.is_empty() {
                        "Field".to_owned()
                    } else {
                        field.label.clone()
                    })
                    .font(ts::BODY)
                    .color(look.muted)
                    .wrap()
                    .w(140.0),
                )
                .child(el().grow(1.0).min_w(0.0).child(value));
            if index > 0 {
                line = line.border_sides([1.0, 0.0, 0.0, 0.0], look.line_hair);
            }
            table = table.child(line);
        }
        body = body.child(
            col()
                .w_full()
                .gap(size::SP1)
                .child(widgets::section_label(look, "Fields"))
                .child(table),
        );
    }

    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(detail_header(state, meta, look, kit, layer))
        .child(
            col()
                .key("details-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(body),
        )
}

/// Stable navigation and editing controls; actions on the command live beside its content.
fn detail_header(
    state: &Library,
    meta: &EntryMeta,
    look: &Look,
    kit: &Kit,
    layer: &mut Vec<El>,
) -> El {
    let mut bar = row()
        .key("detail-header")
        .w_full()
        .h(56.0)
        .px(size::SP5)
        .gap(size::SP2)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], look.line_hair);
    if state.narrow {
        bar = bar.child(
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
    } else {
        bar = bar
            .child(
                icon(
                    if meta.kind == Kind::Secret {
                        &icons::SECRET
                    } else {
                        &icons::COMMAND
                    },
                    14.0,
                )
                .color(look.muted),
            )
            .child(
                text(if meta.kind == Kind::Secret {
                    "Secret"
                } else {
                    "Command"
                })
                .font(ts::SM)
                .color(look.muted),
            );
    }
    bar = bar.child(el().grow(1.0)).child(
        widgets::button_sm(look, "edit", "Edit", Some(&icons::EDIT), Variant::Ghost)
            .help("Edit entry (Ctrl+E)")
            .on_press_tx(|state: &mut Library, tx| {
                tx.instant();
                state.start_edit();
            }),
    );
    let more = Menu::new("entry-options")
        .open(state.entry_options_open)
        .placement(Placement::BottomEnd)
        .trigger(
            widgets::icon_button(look, "entry-more", "More actions", &icons::MORE, look.muted)
                .on_press_tx(|state: &mut Library, tx| {
                    tx.instant();
                    state.entry_options_open = !state.entry_options_open;
                }),
        )
        .item(
            MenuItem::new(if meta.pinned {
                "Unpin entry"
            } else {
                "Pin entry"
            })
            .icon(&icons::PIN)
            .on_select(|state: &mut Library| {
                state.entry_options_open = false;
                state.toggle_pin(None);
            }),
        )
        .separator()
        .item(
            MenuItem::new("Delete entry…")
                .icon(&icons::TRASH)
                .danger(true)
                .on_select(|state: &mut Library| {
                    state.entry_options_open = false;
                    state.request_delete(None);
                }),
        )
        .on_close(|state: &mut Library| state.entry_options_open = false)
        .build(kit, layer);
    bar.child(more)
}

fn primary_actions(state: &Library, look: &Look, secret: bool) -> El {
    let copy = widgets::button(
        look,
        "copy",
        if secret { "Copy value" } else { "Copy command" },
        Some(&icons::COPY),
        Variant::Primary,
    )
    .h(32.0);
    row()
        .key("detail-actions")
        .w_full()
        .gap(size::SP2)
        .center_y()
        .child(if state.saving {
            widgets::disabled(copy)
        } else {
            copy.on_press(|state: &mut Library| state.copy_entry(None))
        })
        .child(if secret {
            widgets::button(
                look,
                "reveal",
                if state.revealed {
                    "Hide value"
                } else {
                    "Reveal value"
                },
                Some(if state.revealed {
                    &icons::HIDE
                } else {
                    &icons::REVEAL
                }),
                Variant::Default,
            )
            .h(32.0)
            .on_press_tx(|state: &mut Library, tx| {
                tx.instant();
                state.reveal();
            })
        } else {
            widgets::button(look, "run", "Run…", Some(&icons::RUN), Variant::Default)
                .h(32.0)
                .help("Review the command before running")
                .on_press(|state: &mut Library| state.request_run())
        })
}

impl Library {
    pub fn open_url(&mut self, url: String) {
        if let Err(error) = self.hub().library.open_url(url) {
            self.status = Some(crate::hub::message(&error));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dev_console_core::context::FinderContext;

    fn meta(id: &str, tags: &[&str], project: Option<(&str, &str)>) -> EntryMeta {
        EntryMeta {
            id: id.into(),
            kind: Kind::Command,
            revision: 1,
            name: id.into(),
            entry_type: "Shell".into(),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            description: String::new(),
            pinned: false,
            updated_at: String::new(),
            usage_count: 0,
            last_used_at: None,
            working_directory: String::new(),
            project: project.map(|(root, name)| FinderContext {
                root: root.into(),
                name: name.into(),
            }),
        }
    }

    #[test]
    fn commands_group_by_project_and_general_sorts_last() {
        let entries = vec![
            meta("loose", &[], None),
            meta("api-test", &[], Some(("/src/api", "api"))),
            meta("tagged", &["project:api"], None),
            meta("web-build", &[], Some(("/src/web", "web"))),
            meta("orphan-tag", &["project:docs"], None),
        ];
        let groups = groups(&entries, Kind::Command);
        let names: Vec<_> = groups
            .iter()
            .map(|group| (group.name.as_str(), group.entries.len()))
            .collect();
        assert_eq!(
            names,
            vec![("api", 2), ("docs", 1), ("web", 1), ("General", 1)]
        );
        assert_eq!(
            groups[0].root, "/src/api",
            "a project tag joins the project with that name"
        );
        assert_eq!(groups[1].key, "tag:docs");
    }

    #[test]
    fn secrets_are_never_grouped() {
        let entries = vec![
            meta("a", &["project:api"], Some(("/src/api", "api"))),
            meta("b", &[], None),
        ];
        let groups = groups(&entries, Kind::Secret);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].entries.len(), 2);
    }
    #[test]
    fn same_named_projects_stay_separate_and_general_stays_available() {
        let entries = vec![
            meta("first", &[], Some(("/src/one/api", "api"))),
            meta("second", &[], Some(("/src/two/api", "api"))),
            meta("general", &[], None),
        ];
        let grouped = groups(&entries, Kind::Command);
        assert_eq!(grouped.len(), 3);
        assert_ne!(grouped[0].key, grouped[1].key);
        assert_eq!(grouped[2].name, "General");
        assert_eq!(grouped.iter().map(|g| g.entries.len()).sum::<usize>(), 3);
    }
}
