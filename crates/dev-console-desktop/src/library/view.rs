//! The Library window's layout: header, tabs and toolbar, the grouped entry list, the details
//! pane, and the dialogs over them. Ports the markup of `Library.svelte`, `EntryList.svelte`,
//! `EntryDetails.svelte`, `ExtraFields.svelte` (read-only half), `VaultStatus.svelte` and `Tabs.svelte`.

use dev_console_core::models::{EntryDetails, EntryMeta, FieldDisplay, Kind};
use dev_console_core::time_format::{absolute_time, relative_time_now};
use dev_console_core::validation::is_openable_url;
use rune::{col, el, icon, row, text, Cursor, Cx, El, ScrollSpec, Transition};
use rune_kit::{Kit, TextInput};

use super::{dialogs, editor, environment, history, import, settings, Library};
use crate::icons;
use crate::theme::{size, text as ts, Look};
use crate::widgets::{self, Tone, Variant};

pub fn library_view(state: &Library, cx: &mut Cx<Library>) -> El {
    let look = Look::for_setting(&state.theme, cx.system_dark());
    let kit = look.kit();
    let mut root = col()
        .key("library")
        .w_full()
        .h_full()
        .fill(look.bg)
        .on_layout(|state: &mut Library, rect: [f32; 4]| state.set_width(rect[2]))
        .child(header(state, &look))
        .child(toolbar(state, &look, &kit))
        .children(status(state, &look))
        .child(panes(state, &look, &kit));
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

fn header(state: &Library, look: &Look) -> El {
    let mut bar = row()
        .key("library-header")
        .w_full()
        .h(48.0)
        .px(size::SP3)
        .gap(size::SP3)
        .center_y()
        .child(
            row()
                .gap(size::SP2)
                .center_y()
                .child(
                    row()
                        .w(24.0)
                        .h(24.0)
                        .center()
                        .radius(size::RADIUS)
                        .fill(look.accent_soft)
                        .child(icon(&icons::COMMAND, 14.0).color(look.accent)),
                )
                .child(text("Dev Console").font(ts::STRONG).color(look.fg)),
        )
        .child(el().grow(1.0));
    if state.authenticating {
        bar = bar.child(
            text("Waiting for authentication…")
                .key("waiting")
                .role("status")
                .font(ts::SM)
                .color(look.muted),
        );
    }
    if let Some(vault) = &state.vault {
        bar = bar.child(vault_status(state, vault, look));
    }
    bar.child(
        widgets::button_sm(
            look,
            "settings",
            "Settings",
            Some(&icons::GEAR),
            Variant::Ghost,
        )
        .on_press(|state: &mut Library| state.open_settings()),
    )
}

fn vault_status(state: &Library, vault: &dev_console_core::models::VaultState, look: &Look) -> El {
    let (ink, label) = if vault.unlocked {
        (look.ok, "Vault unlocked")
    } else if vault.manual_lock {
        (look.warn, "Vault locked")
    } else if vault.vault_available {
        (look.warn, "Vault unavailable")
    } else {
        (look.danger, "Keyring unavailable")
    };
    let mut text_col = col().child(text(label).font(ts::SM).color(look.fg));
    if !vault.unlocked && vault.keyring_available && !vault.manual_lock {
        text_col = text_col.child(
            text("keyring present, vault sealed")
                .font(ts::XS)
                .color(look.muted),
        );
    }
    let toggle = widgets::button_sm(
        look,
        "vault-toggle",
        if vault.unlocked { "Lock" } else { "Unlock" },
        Some(if vault.unlocked {
            &icons::LOCK
        } else {
            &icons::UNLOCK
        }),
        if vault.unlocked {
            Variant::Ghost
        } else {
            Variant::Primary
        },
    );
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
    let mut chip = row()
        .key("vault-status")
        .h(34.0)
        .pl(size::SP2)
        .pr(4.0)
        .gap(size::SP2)
        .center_y()
        .radius(999.0)
        .fill(look.panel)
        .border(1.0, look.line)
        .child(
            row()
                .w(22.0)
                .h(22.0)
                .center()
                .radius(999.0)
                .fill(ink.alpha(0.16))
                .child(
                    icon(
                        if vault.unlocked {
                            &icons::UNLOCK
                        } else {
                            &icons::LOCK
                        },
                        12.0,
                    )
                    .color(ink),
                ),
        )
        .child(text_col)
        .child(toggle);
    if let Some(reason) = &vault.reason {
        chip = chip.help(reason.clone());
    }
    chip
}

fn toolbar(state: &Library, look: &Look, kit: &Kit) -> El {
    let hidden = state.secret_metadata_hidden();
    let tabs = row()
        .key("tabs")
        .role("tablist")
        .focus_group()
        .gap(size::SP1)
        .child(tab(
            state,
            look,
            Kind::Command,
            "Commands",
            &icons::COLLECTION,
            Some(state.counts[0]),
        ))
        .child(tab(
            state,
            look,
            Kind::Secret,
            "Secrets",
            &icons::SECRET,
            (!hidden).then_some(state.counts[1]),
        ));
    let filter = TextInput::new(&state.query)
        .width(None)
        .label("Search this tab")
        .placeholder("Search this tab")
        .leading_icon(&icons::SEARCH)
        .on_edit(|state: &mut Library| &mut state.query)
        .on_change(|state: &mut Library, _| state.filter_changed())
        .build(kit)
        .key("tab-search");
    let mut tools = row()
        .w_full()
        .gap(size::SP2)
        .center_y()
        .child(el().grow(1.0).min_w(0.0).child(filter))
        .child(
            widgets::button(
                look,
                "add-entry",
                "New",
                Some(&icons::PLUS),
                Variant::Primary,
            )
            .on_press(|state: &mut Library| state.start_new()),
        );
    {
        tools = tools.child(
            widgets::button(
                look,
                "open-environment",
                if state.narrow { "Env" } else { "Environment" },
                Some(&icons::LOCK),
                Variant::Default,
            )
            .on_press(|state: &mut Library| {
                let root = state
                    .meta()
                    .and_then(|meta| meta.project.as_ref())
                    .map(|project| project.root.to_string_lossy().into_owned())
                    .unwrap_or_default();
                state.open_environment(root);
            }),
        );
        if state.tab == Kind::Command {
            tools = tools.child(
                widgets::button(
                    look,
                    "import-project",
                    if state.narrow {
                        "Import"
                    } else {
                        "Import from project"
                    },
                    Some(&icons::FOLDER),
                    Variant::Default,
                )
                .help("Import the tasks a project folder defines as commands")
                .label("Import from project")
                .on_press(|state: &mut Library| state.start_project_import()),
            );
        }
    }
    col()
        .key("toolbar")
        .w_full()
        .px(size::SP3)
        .pb(size::SP2)
        .gap(size::SP2)
        .fill(look.panel)
        .border_sides([0.0, 0.0, 1.0, 0.0], look.line)
        .child(row().pt(size::SP2).child(tabs))
        .child(tools)
}

fn tab(
    state: &Library,
    look: &Look,
    kind: Kind,
    label: &'static str,
    glyph: &'static rune::Icon,
    count: Option<usize>,
) -> El {
    let active = state.tab == kind;
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
        .h(size::TAB_H)
        .px(size::SP3)
        .gap(size::SP2)
        .center_y()
        .radius(size::RADIUS)
        .cursor(Cursor::Pointer)
        .hover(look.fg.alpha(0.05))
        .on_press(move |state: &mut Library| {
            state.select_tab(kind);
        })
        .child(icon(glyph, 14.0).color(if active { look.fg } else { look.muted }))
        .child(
            text(label)
                .font(if active { ts::STRONG } else { ts::BODY })
                .color(if active { look.fg } else { look.muted }),
        );
    if let Some(count) = count {
        chip = chip.child(
            row()
                .h(18.0)
                .min_w(20.0)
                .px(6.0)
                .center()
                .radius(999.0)
                .fill(if active {
                    look.accent_soft
                } else {
                    look.raised
                })
                .child(text(count.to_string()).font(ts::XS).color(if active {
                    look.accent
                } else {
                    look.muted
                })),
        );
    }
    if active {
        chip = chip
            .fill(look.selected)
            .border_sides([0.0, 0.0, 2.0, 0.0], look.accent);
    }
    chip
}

fn status(state: &Library, look: &Look) -> Option<El> {
    let line = if let Some(error) = state.list_error.as_ref().filter(|error| {
        !(state.secret_metadata_hidden() && error.to_lowercase().contains("locked"))
    }) {
        widgets::status_line(look, "list-error", Tone::Error, error)
    } else if let Some(status) = &state.status {
        let ok = ["Copied", "Saved", "Deleted", "Launched", "Imported"]
            .iter()
            .any(|prefix| status.starts_with(prefix));
        widgets::status_line(
            look,
            "status",
            if ok { Tone::Ok } else { Tone::Info },
            status,
        )
    } else {
        return None;
    };
    Some(row().w_full().px(size::SP3).pt(size::SP2).child(line))
}

fn panes(state: &Library, look: &Look, kit: &Kit) -> El {
    let editing = state.editing.is_some();
    let show_list = !state.narrow || (!editing && state.selected_id.is_none());
    let show_detail = !state.narrow || state.selected_id.is_some() || editing;
    let mut panes = row().key("panes").w_full().grow(1.0).min_h(0.0);
    if show_list {
        let mut list = col()
            .key("list-pane")
            .label("Entry list")
            .h_full()
            .min_h(0.0)
            .fill(look.panel);
        list = if state.narrow {
            list.w_full()
        } else {
            list.w(size::PANE_LIST)
                .border_sides([0.0, 1.0, 0.0, 0.0], look.line)
        };
        if state.tab == Kind::Command && state.history_enabled {
            list = list.child(history::view(state, look));
        }
        list = list.child(
            col()
                .key("list-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(entry_list(state, look)),
        );
        panes = panes.child(list);
    }
    if show_detail {
        let mut detail = col()
            .key("detail-pane")
            .label("Entry details")
            .grow(1.0)
            .min_w(0.0)
            .h_full()
            .min_h(0.0);
        if state.narrow && !editing && state.selected_id.is_some() {
            detail = detail.child(
                row().px(size::SP3).pt(size::SP2).child(
                    widgets::button_sm(
                        look,
                        "back-to-list",
                        "List",
                        Some(&icons::BACK),
                        Variant::Ghost,
                    )
                    .on_press(|state: &mut Library| state.back_to_list()),
                ),
            );
        }
        detail = detail.child(detail_body(state, look, kit));
        panes = panes.child(detail);
    }
    panes
}

fn detail_body(state: &Library, look: &Look, kit: &Kit) -> El {
    if let Some(editor) = &state.editing {
        return editor::editor_view(state, editor, look, kit);
    }
    if let Some(details) = &state.details {
        return details_view(state, details, look);
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
            Some(&icons::UNLOCK),
            Variant::Primary,
        );
        return widgets::empty_state(
            look,
            "vault-locked",
            &icons::LOCK,
            "Vault locked",
            "Secrets stay encrypted on disk. Unlocking asks the system for confirmation; a manual lock is never cleared automatically.",
            vec![if state.authenticating { widgets::disabled(unlock) } else { unlock.on_press(|state: &mut Library| state.unlock()) }],
        );
    }
    if state.entries.is_empty() && !state.loading {
        return widgets::empty_state(
            look,
            "nothing-selected",
            &icons::PLUS,
            "Nothing selected",
            "Create an entry, or press the shortcut below to search everything at once.",
            vec![
                widgets::button(
                    look,
                    "empty-new",
                    if state.tab == Kind::Secret {
                        "New secret"
                    } else {
                        "New command"
                    },
                    Some(&icons::PLUS),
                    Variant::Primary,
                )
                .on_press(|state: &mut Library| state.start_new()),
                row()
                    .key("shortcut-hint")
                    .gap(6.0)
                    .center_y()
                    .child(widgets::hint(look, &["Super", "F"], "Finder")),
            ],
        );
    }
    widgets::empty_state(
        look,
        "select-entry",
        &icons::SEARCH,
        "Select an entry",
        "Pick a row on the left to see it here.",
        Vec::new(),
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
        .py(size::SP2)
        .gap(2.0);
    if state.loading && state.entries.is_empty() {
        for index in 0..3 {
            list = list.child(
                el().key(format!("skeleton-{index}"))
                    .w_full()
                    .h(size::ROW_H - 6.0)
                    .radius(size::RADIUS_ROW)
                    .fill(look.raised),
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
                .child(icon(&icons::LOCK, 13.0).color(look.muted))
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
        return list.child(el().h(240.0).w_full().child(widgets::empty_state(
            look,
            "list-empty",
            &icons::INBOX,
            "Nothing here yet",
            hint,
            Vec::new(),
        )));
    }
    let filter_active = !state.query.text.trim().is_empty();
    let all = groups(&state.entries, state.tab);
    let several = all.len() > 1;
    let busy = state.draft_dirty();
    for group in all {
        let open = group.key.is_empty() || filter_active || state.expanded.contains(&group.key);
        let mut section = col()
            .key(format!("group-{}", group.key))
            .role("group")
            .label(group.name.clone())
            .w_full()
            .gap(2.0);
        if state.tab == Kind::Command && !group.key.is_empty() {
            let key = group.key.clone();
            let root = group.root.clone();
            section =
                section.child(
                    row()
                        .key(format!("project-toggle-{}", group.key))
                        .role("button")
                        .label(group.name.clone())
                        .expanded(open)
                        .help(if group.root.is_empty() {
                            group.name.clone()
                        } else {
                            group.root.clone()
                        })
                        .focusable(true)
                        .focus_ring(2.0, 1.0, look.focus)
                        .w_full()
                        .h(30.0)
                        .px(size::SP2)
                        .gap(6.0)
                        .center_y()
                        .radius(size::RADIUS)
                        .cursor(Cursor::Pointer)
                        .hover(look.fg.alpha(0.05))
                        .on_press(move |state: &mut Library| state.toggle_group(&key, &root))
                        .child(icon(&icons::CHEVRON, 13.0).color(look.muted).icon_rotation(
                            if open {
                                0.0
                            } else {
                                -std::f32::consts::FRAC_PI_2
                            },
                        ))
                        .child(icon(&icons::FOLDER, 14.0).color(look.muted))
                        .child(
                            text(group.name.clone())
                                .font(ts::STRONG)
                                .color(look.fg)
                                .truncate()
                                .grow(1.0),
                        )
                        .child(count_chip(look, group.entries.len())),
                );
        } else if state.tab == Kind::Command && several {
            section = section.child(
                row()
                    .key("general-head")
                    .h(30.0)
                    .px(size::SP2)
                    .gap(6.0)
                    .center_y()
                    .child(text("General").font(ts::STRONG).color(look.muted))
                    .child(count_chip(look, group.entries.len())),
            );
        }
        if open {
            if !group.root.is_empty() {
                let root = group.root.clone();
                section = section.child(
                    row()
                        .key(format!("project-environment-{}", group.key))
                        .role("button")
                        .label("Environment")
                        .focusable(true)
                        .w_full()
                        .h(28.0)
                        .px(size::SP3)
                        .gap(6.0)
                        .center_y()
                        .radius(size::RADIUS)
                        .cursor(Cursor::Pointer)
                        .hover(look.fg.alpha(0.05))
                        .on_press(move |state: &mut Library| state.open_environment(root.clone()))
                        .child(icon(&icons::LOCK, 13.0).color(look.muted))
                        .child(text("Environment").font(ts::SM).color(look.muted)),
                );
            }
            for entry in group.entries {
                section = section.child(entry_row(state, look, entry, busy));
            }
        }
        list = list.child(section);
    }
    list
}

fn count_chip(look: &Look, count: usize) -> El {
    row()
        .h(18.0)
        .min_w(20.0)
        .px(6.0)
        .center()
        .radius(999.0)
        .fill(look.raised)
        .child(text(count.to_string()).font(ts::XS).color(look.muted))
}

fn entry_row(state: &Library, look: &Look, entry: EntryMeta, busy: bool) -> El {
    let selected = state.selected_id.as_deref() == Some(entry.id.as_str());
    let id = entry.id.clone();
    let copy_target = entry.clone();
    let pin_target = entry.clone();
    let delete_target = entry.clone();
    let pin = widgets::icon_button(
        look,
        format!("row-pin-{}", entry.id),
        &format!(
            "{} {}",
            if entry.pinned { "Unpin" } else { "Pin" },
            entry.name
        ),
        if entry.pinned {
            &icons::UNPIN
        } else {
            &icons::PIN
        },
        if entry.pinned {
            look.accent
        } else {
            look.muted
        },
    );
    let delete = widgets::icon_button(
        look,
        format!("row-delete-{}", entry.id),
        &format!("Delete {}", entry.name),
        &icons::TRASH,
        look.danger,
    );
    let mut item = row()
        .key(format!("entry-row-{}", entry.id))
        .role("option")
        .label(entry.name.clone())
        .checked(selected)
        .focusable(true)
        .focus_ring(2.0, 1.0, look.focus)
        .w_full()
        .h(size::ROW_H + 4.0)
        .pl(size::SP3)
        .pr(4.0)
        .gap(2.0)
        .center_y()
        .radius(size::RADIUS_ROW)
        .cursor(Cursor::Pointer)
        .hover(look.fg.alpha(0.04))
        .on_press(move |state: &mut Library| state.select(&id))
        .child(
            text(entry.name.clone())
                .font(ts::ROW)
                .color(if selected { look.selected_fg } else { look.fg })
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        )
        .child(
            widgets::icon_button(
                look,
                format!("row-copy-{}", entry.id),
                &format!("Copy {}", entry.name),
                &icons::COPY,
                look.muted,
            )
            .on_press(move |state: &mut Library| state.copy_entry(Some(copy_target.clone()))),
        )
        .child(if busy {
            widgets::disabled(pin)
        } else {
            pin.on_press(move |state: &mut Library| state.toggle_pin(Some(pin_target.clone())))
        })
        .child(if busy {
            widgets::disabled(delete)
        } else {
            delete.on_press(move |state: &mut Library| {
                state.request_delete(Some(delete_target.clone()))
            })
        });
    if selected {
        item = item.fill(look.selected).border(1.0, look.selected_edge);
    }
    item
}

fn details_view(state: &Library, details: &EntryDetails, look: &Look) -> El {
    let meta = &details.meta;
    let secret = state.tab == Kind::Secret;
    let mut context = row()
        .gap(6.0)
        .center_y()
        .child(text(meta.entry_type.clone()).font(ts::SM).color(look.muted));
    for tag in &meta.tags {
        context = context.child(text(format!("· #{tag}")).font(ts::SM).color(look.muted));
    }
    if meta.pinned {
        context = context
            .child(text("·").font(ts::SM).color(look.muted))
            .child(icon(&icons::PIN, 11.0).color(look.accent))
            .child(text("Pinned").font(ts::SM).color(look.muted));
    }
    context = context
        .child(
            text(format!("· r{}", meta.revision))
                .font(ts::SM)
                .color(look.dim),
        )
        .child(
            text(format!("· updated {}", relative_time_now(&meta.updated_at)))
                .font(ts::SM)
                .color(look.dim)
                .help(absolute_time(&meta.updated_at)),
        );
    let kind_badge = row()
        .h(22.0)
        .px(8.0)
        .gap(5.0)
        .center_y()
        .radius(999.0)
        .fill(if secret {
            look.secret_soft
        } else {
            look.accent_soft
        })
        .border(
            1.0,
            if secret {
                look.kind_secret.alpha(0.5)
            } else {
                look.accent_line
            },
        )
        .child(
            icon(
                if secret {
                    &icons::SECRET
                } else {
                    &icons::COMMAND
                },
                11.0,
            )
            .color(if secret {
                look.kind_secret
            } else {
                look.accent
            }),
        )
        .child(
            text(if secret { "Secret" } else { "Command" })
                .font(ts::XS)
                .color(if secret {
                    look.kind_secret
                } else {
                    look.accent
                }),
        );

    let mut body = col()
        .key("entry-details")
        .w_full()
        .px(size::SP4)
        .py(size::SP4)
        .gap(size::SP4)
        .child(
            row()
                .w_full()
                .gap(size::SP3)
                .child(widgets::kind_tile(look, secret, size::TILE_LG))
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(6.0)
                        .child(
                            text(meta.name.clone())
                                .key("details-name")
                                .font(ts::XL)
                                .color(look.fg)
                                .wrap(),
                        )
                        .child(
                            row()
                                .gap(size::SP2)
                                .center_y()
                                .child(kind_badge)
                                .child(context),
                        ),
                ),
        );
    if !meta.description.is_empty() {
        body = body.child(
            text(meta.description.clone())
                .font(ts::BODY)
                .color(look.fg)
                .wrap(),
        );
    }
    if secret {
        let shown = if state.revealed {
            details.value.clone().unwrap_or_default()
        } else {
            "•".repeat(10)
        };
        body = body.child(
            col()
                .w_full()
                .gap(6.0)
                .child(
                    row()
                        .gap(6.0)
                        .child(widgets::section_label(look, "Value"))
                        .children((!state.revealed).then(|| text("(masked)").font(ts::XS).color(look.muted))),
                )
                .child(
                    row()
                        .w_full()
                        .min_h(44.0)
                        .px(size::SP3)
                        .gap(size::SP2)
                        .center_y()
                        .radius(size::RADIUS)
                        .fill(look.sunken)
                        .border(1.0, look.line)
                        .child(text(shown).key("secret-value").transition(Transition::none()).font(ts::MONO).color(look.fg).selectable(state.revealed).grow(1.0).wrap_anywhere())
                        .child(
                            widgets::button_sm(look, "reveal-inline", if state.revealed { "Hide" } else { "Reveal" }, Some(if state.revealed { &icons::HIDE } else { &icons::REVEAL }), Variant::Ghost)
                                .on_press_tx(|state: &mut Library, tx| {
                                    tx.instant();
                                    state.reveal();
                                }),
                        ),
                )
                .child(
                    text("Values are never shown until you ask, never written to logs, and copied to the clipboard only for the expiry window you set.")
                        .font(ts::SM)
                        .color(look.muted)
                        .wrap(),
                ),
        );
    } else {
        let mut section = col()
            .w_full()
            .gap(6.0)
            .child(widgets::section_label(look, "Command"))
            .child(
                row()
                    .w_full()
                    .px(size::SP3)
                    .py(size::SP3)
                    .radius(size::RADIUS)
                    .fill(look.sunken)
                    .border(1.0, look.line)
                    .child(
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
                    .gap(4.0)
                    .child(text("Runs in").font(ts::SM).color(look.muted))
                    .child(
                        text(details.working_directory.clone())
                            .font(ts::MONO_SM)
                            .color(look.fg)
                            .selectable(true),
                    ),
            );
        }
        body = body.child(section);
    }
    if !details.notes.is_empty() {
        body = body.child(
            col()
                .w_full()
                .gap(6.0)
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
        let mut grid = row()
            .key("extra-fields")
            .w_full()
            .gap(size::SP2)
            .flex_wrap();
        for field in &details.fields {
            let value: El = if field.display == FieldDisplay::Url && is_openable_url(&field.value) {
                let url = field.value.trim().to_owned();
                text(field.value.clone())
                    .key(format!("field-url-{}", field.id))
                    .role("link")
                    .help("Open in browser")
                    .font(ts::MONO)
                    .color(look.accent)
                    .cursor(Cursor::Pointer)
                    .on_press(move |state: &mut Library| state.open_url(url.clone()))
            } else {
                text(field.value.clone())
                    .font(ts::MONO)
                    .color(look.fg)
                    .wrap_anywhere()
                    .selectable(true)
            };
            grid = grid.child(
                col()
                    .min_w(240.0)
                    .max_w(420.0)
                    .gap(4.0)
                    .padding(size::SP2)
                    .radius(size::RADIUS)
                    .fill(look.sunken)
                    .border(1.0, look.line)
                    .child(widgets::section_label(
                        look,
                        if field.label.is_empty() {
                            "Field"
                        } else {
                            &field.label
                        },
                    ))
                    .child(value),
            );
        }
        body = body.child(
            col()
                .w_full()
                .gap(6.0)
                .child(widgets::section_label(look, "Custom fields"))
                .child(grid),
        );
    }

    let busy = state.saving;
    let copy = widgets::button(look, "copy", "Copy", Some(&icons::COPY), Variant::Primary);
    let actions = row()
        .key("detail-actions")
        .w_full()
        .h(56.0)
        .px(size::SP4)
        .gap(size::SP2)
        .center_y()
        .border_sides([1.0, 0.0, 0.0, 0.0], look.line)
        .child(if busy {
            widgets::disabled(copy)
        } else {
            copy.on_press(|state: &mut Library| state.copy_entry(None))
        })
        .child(if secret {
            widgets::button(
                look,
                "reveal",
                if state.revealed { "Hide" } else { "Reveal" },
                Some(&icons::REVEAL),
                Variant::Default,
            )
            .on_press_tx(|state: &mut Library, tx| {
                tx.instant();
                state.reveal();
            })
        } else {
            widgets::button(look, "run", "Run", Some(&icons::RUN), Variant::Default)
                .on_press(|state: &mut Library| state.request_run())
        })
        .child(el().grow(1.0))
        .child(
            widgets::button(look, "edit", "Edit", Some(&icons::EDIT), Variant::Default)
                .on_press(|state: &mut Library| state.start_edit()),
        )
        .child(
            widgets::button(
                look,
                "pin",
                if meta.pinned { "Unpin" } else { "Pin" },
                Some(&icons::PIN),
                Variant::Default,
            )
            .on_press(|state: &mut Library| state.toggle_pin(None)),
        )
        .child(
            widgets::button(
                look,
                "delete",
                "Delete",
                Some(&icons::TRASH),
                Variant::Danger,
            )
            .on_press(|state: &mut Library| state.request_delete(None)),
        );

    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
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
        .child(actions)
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
