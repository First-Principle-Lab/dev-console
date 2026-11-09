//! The entry editor: `EntryEditor.svelte` and the editable half of `ExtraFields.svelte`.
//!
//! The draft lives in Rune text edits so the fields are real, focusable inputs; [`Editor::draft`]
//! reads them back into the core `EntryDraft` that validation and the repository take, and
//! [`Editor::dirty`] compares that with the record the editor was opened on.

use dev_console_core::models::{EntryDetails, EntryDraft, ExtraField, FieldDisplay, Kind};
use dev_console_core::validation::{name_has_spaces, validate_editor_draft, NAME_SPACES_MESSAGE};
use rune::{col, el, row, text, El, FileDialogOptions, TextEdit, Tx};
use rune_kit::{Kit, TextArea, TextInput};

use super::Library;
use crate::icons;
use crate::theme::{size, text as ts, Look};
use crate::widgets::{self, Variant};

pub struct FieldEdit {
    pub id: String,
    pub label: TextEdit,
    pub value: TextEdit,
    pub display: FieldDisplay,
}

pub struct Editor {
    pub kind: Kind,
    pub id: Option<String>,
    pub expected_revision: i64,
    pub entry_type: String,
    pub tags: Vec<String>,
    pub description: String,
    pub pinned: bool,
    pub name: TextEdit,
    pub working_directory: TextEdit,
    pub command: TextEdit,
    /// A secret's value. Masked unless `value_visible`; zeroed by [`Editor::wipe`].
    pub value: TextEdit,
    pub notes: TextEdit,
    pub fields: Vec<FieldEdit>,
    pub value_visible: bool,
    /// Save was pressed at least once, so validation messages may show.
    pub attempted: bool,
    pub baseline: EntryDraft,
    pub picking_folder: bool,
    pub folder_error: Option<String>,
    next_field: u64,
}

impl Editor {
    pub fn new(draft: EntryDraft) -> Editor {
        let mut name = TextEdit::new(draft.name.clone());
        name.focused = true;
        Editor {
            kind: draft.kind,
            id: draft.id.clone(),
            expected_revision: draft.expected_revision,
            entry_type: draft.entry_type.clone(),
            tags: draft.tags.clone(),
            description: draft.description.clone(),
            pinned: draft.pinned,
            name,
            working_directory: TextEdit::new(draft.working_directory.clone()),
            command: TextEdit::new(draft.command.clone()),
            value: TextEdit::new(draft.value.clone()).secure(),
            notes: TextEdit::new(draft.notes.clone()),
            fields: draft
                .fields
                .iter()
                .map(|field| FieldEdit {
                    id: field.id.clone(),
                    label: TextEdit::new(field.label.clone()),
                    value: TextEdit::new(field.value.clone()),
                    display: field.display,
                })
                .collect(),
            value_visible: false,
            attempted: false,
            baseline: draft,
            picking_folder: false,
            folder_error: None,
            next_field: 0,
        }
    }

    /// `draftFromDetails`: the saved record as a draft.
    pub fn from_details(details: &EntryDetails, kind: Kind) -> Editor {
        Editor::new(EntryDraft {
            kind,
            id: Some(details.meta.id.clone()),
            expected_revision: details.meta.revision,
            name: details.meta.name.clone(),
            entry_type: details.meta.entry_type.clone(),
            tags: details.meta.tags.clone(),
            description: details.meta.description.clone(),
            notes: details.notes.clone(),
            fields: details.fields.clone(),
            pinned: details.meta.pinned,
            command: details.command.clone().unwrap_or_default(),
            working_directory: details.working_directory.clone(),
            value: details.value.clone().unwrap_or_default(),
        })
    }

    pub fn draft(&self) -> EntryDraft {
        EntryDraft {
            kind: self.kind,
            id: self.id.clone(),
            expected_revision: self.expected_revision,
            name: self.name.text.clone(),
            entry_type: self.entry_type.clone(),
            tags: self.tags.clone(),
            description: self.description.clone(),
            notes: self.notes.text.clone(),
            fields: self
                .fields
                .iter()
                .map(|field| ExtraField {
                    id: field.id.clone(),
                    label: field.label.text.clone(),
                    value: field.value.text.clone(),
                    display: field.display,
                })
                .collect(),
            pinned: self.pinned,
            command: self.command.text.clone(),
            working_directory: self.working_directory.text.clone(),
            value: self.value.text.clone(),
        }
    }

    pub fn dirty(&self) -> bool {
        self.draft() != self.baseline
    }

    /// `changeProject`: a new folder replaces the folder-derived project tag.
    pub fn folder_changed(&mut self) {
        self.tags.retain(|tag| !tag.starts_with("project:"));
    }

    pub fn set_folder(&mut self, folder: String) {
        self.working_directory.replace_text(folder);
        self.folder_changed();
    }

    pub fn add_field(&mut self) {
        self.next_field += 1;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        let mut label = TextEdit::new(String::new());
        label.focused = true;
        self.fields.push(FieldEdit {
            id: format!("f-{stamp:x}-{}", self.next_field),
            label,
            value: TextEdit::new(String::new()),
            display: FieldDisplay::Text,
        });
    }

    pub fn move_field(&mut self, index: usize, delta: isize) {
        let target = index as isize + delta;
        if target < 0 || target as usize >= self.fields.len() {
            return;
        }
        let field = self.fields.remove(index);
        self.fields.insert(target as usize, field);
    }

    pub fn remove_field(&mut self, index: usize) {
        if index < self.fields.len() {
            let mut field = self.fields.remove(index);
            field.value.clear();
        }
    }

    pub fn cycle_display(&mut self, index: usize) {
        if let Some(field) = self.fields.get_mut(index) {
            field.display = match field.display {
                FieldDisplay::Text => FieldDisplay::Multiline,
                FieldDisplay::Multiline => FieldDisplay::Url,
                FieldDisplay::Url => FieldDisplay::Text,
            };
        }
    }

    /// Zeroes the secret value and every field value before the draft is dropped.
    pub fn wipe(&mut self) {
        self.wipe_value();
        for field in &mut self.fields {
            field.value.clear();
        }
        wipe_string(&mut self.baseline.value);
    }

    pub fn wipe_value(&mut self) {
        self.value.clear();
        wipe_string(&mut self.baseline.value);
        self.value_visible = false;
    }
}

fn wipe_string(value: &mut String) {
    // SAFETY: zero bytes remain valid UTF-8, and the allocation is still owned here.
    unsafe { value.as_bytes_mut().fill(0) };
    value.clear();
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.value.clear();
        wipe_string(&mut self.baseline.value);
    }
}

impl Library {
    pub(crate) fn extra_field(&mut self, index: usize, value: bool) -> &mut TextEdit {
        match self
            .editing
            .as_mut()
            .and_then(|editor| editor.fields.get_mut(index))
        {
            Some(field) if value => &mut field.value,
            Some(field) => &mut field.label,
            None => &mut self.scratch,
        }
    }

    /// Browse…: the XDG portal folder picker. Cancelling leaves the draft alone.
    pub fn browse_folder(&mut self, tx: &mut Tx) {
        let Some(editor) = self.editing.as_mut() else {
            return;
        };
        editor.picking_folder = true;
        editor.folder_error = None;
        let start = editor.working_directory.text.trim().to_owned();
        let options = FileDialogOptions {
            title: "Choose a project folder".into(),
            directory: Some(start).filter(|dir| !dir.is_empty()).map(Into::into),
            ..Default::default()
        };
        tx.open_folder(options, |state: &mut Library, picked, _| {
            let Some(editor) = state.editing.as_mut() else {
                return;
            };
            editor.picking_folder = false;
            match picked {
                Ok(Some(folder)) => editor.set_folder(folder.to_string_lossy().into_owned()),
                Ok(None) => {}
                Err(error) => {
                    editor.folder_error = Some(format!("Could not open the folder picker: {error}"))
                }
            }
        });
    }
}

/// Draws the editor form.
pub fn editor_view(state: &Library, editor: &Editor, look: &Look, kit: &Kit) -> El {
    let command = editor.kind == Kind::Command;
    let draft = editor.draft();
    let dirty = editor.dirty();
    let problem = validate_editor_draft(&draft);
    let spaced = name_has_spaces(&editor.name.text);

    let mut head = row()
        .key("editor-head")
        .w_full()
        .gap(size::SP3)
        .center_y()
        .child(widgets::kind_tile(look, !command, size::TILE_LG))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(
                    text(format!(
                        "{} {}",
                        if editor.id.is_some() { "Edit" } else { "New" },
                        if command { "command" } else { "secret" }
                    ))
                    .key("editor-title")
                    .font(ts::LG)
                    .color(look.fg),
                )
                .child(if dirty {
                    row()
                        .key("dirty-indicator")
                        .role("status")
                        .self_start()
                        .h(18.0)
                        .px(6.0)
                        .gap(4.0)
                        .center_y()
                        .radius(999.0)
                        .fill(look.warn_soft)
                        .child(rune::icon(&icons::ALERT, 10.0).color(look.warn))
                        .child(text("unsaved changes").font(ts::XS).color(look.warn))
                } else {
                    text(if command {
                        "Runs in your terminal after approval"
                    } else {
                        "Encrypted with the vault key"
                    })
                    .font(ts::SM)
                    .color(look.muted)
                }),
        )
        .child(
            widgets::button_sm(look, "cancel-edit", "Cancel", None, Variant::Default)
                .on_press(|state: &mut Library| state.cancel_edit()),
        );
    let save = widgets::button_sm(
        look,
        "save",
        if state.saving { "Saving…" } else { "Save" },
        Some(&icons::CHECK),
        Variant::Primary,
    );
    head = head.child(if state.saving {
        widgets::disabled(save)
    } else {
        save.on_press(|state: &mut Library| state.save_draft())
    });

    let mut form = col()
        .key("entry-editor")
        .role("form")
        .w_full()
        .gap(size::SP3)
        .child(head);

    if let Some(error) = &state.save_error {
        form = form.child(widgets::status_line(
            look,
            "save-error",
            widgets::Tone::Error,
            error,
        ));
    } else if let (Some(problem), true, false) = (&problem, editor.attempted, spaced) {
        form = form.child(widgets::status_line(
            look,
            "validation-hint",
            widgets::Tone::Warn,
            problem,
        ));
    }

    let mut name = labelled(
        look,
        "Name",
        TextInput::new(&editor.name)
            .width(None)
            .label("Name")
            .invalid(spaced || (editor.attempted && editor.name.text.trim().is_empty()))
            .on_edit(|state: &mut Library| state.editor_field(|editor| &mut editor.name))
            .on_submit(|state: &mut Library, _| state.save_draft())
            .build(kit)
            .key("field-name"),
    );
    if spaced {
        name = name.child(widgets::status_line(
            look,
            "name-spaces-hint",
            widgets::Tone::Warn,
            NAME_SPACES_MESSAGE,
        ));
    }
    form = form.child(name);

    if command {
        let browse = widgets::button(
            look,
            "editor-project-browse",
            if editor.picking_folder {
                "Choosing…"
            } else {
                "Browse…"
            },
            Some(&icons::FOLDER),
            Variant::Default,
        );
        let mut folder = labelled(
            look,
            "Project folder",
            row()
                .w_full()
                .gap(size::SP2)
                .center_y()
                .child(
                    el().grow(1.0).min_w(0.0).child(
                        TextInput::new(&editor.working_directory)
                            .width(None)
                            .label("Project folder")
                            .placeholder("General, or choose a project folder")
                            .text_style(ts::MONO)
                            .on_edit(|state: &mut Library| {
                                state.editor_field(|editor| &mut editor.working_directory)
                            })
                            .on_change(|state: &mut Library, _| {
                                if let Some(editor) = state.editing.as_mut() {
                                    editor.folder_changed();
                                }
                            })
                            .build(kit)
                            .key("field-working-directory"),
                    ),
                )
                .child(if editor.picking_folder {
                    widgets::disabled(browse)
                } else {
                    browse.on_press_tx(|state: &mut Library, tx| state.browse_folder(tx))
                }),
        );
        if let Some(error) = &editor.folder_error {
            folder = folder.child(widgets::status_line(
                look,
                "folder-error",
                widgets::Tone::Warn,
                error,
            ));
        }
        form = form.child(folder).child(labelled(
            look,
            "Command",
            TextArea::new(&editor.command)
                .width(None)
                .label("Command")
                .rows(5)
                .on_edit(|state: &mut Library| state.editor_field(|editor| &mut editor.command))
                .build(kit)
                .key("field-command"),
        ));
    } else {
        let toggle = widgets::button_sm(
            look,
            "toggle-value",
            if editor.value_visible { "Hide" } else { "Show" },
            Some(&icons::REVEAL),
            Variant::Ghost,
        )
        .on_press(|state: &mut Library| {
            if let Some(editor) = state.editing.as_mut() {
                editor.value_visible = !editor.value_visible;
            }
        });
        form = form.child(
            labelled(
                look,
                "Secret value",
                row()
                    .w_full()
                    .gap(size::SP2)
                    .center_y()
                    .child(
                        el().grow(1.0).min_w(0.0).child(
                            TextInput::new(&editor.value)
                                .width(None)
                                .label("Secret value")
                                .secure()
                                .revealed(editor.value_visible)
                                .text_style(ts::MONO)
                                .on_edit(|state: &mut Library| {
                                    state.editor_field(|editor| &mut editor.value)
                                })
                                .build(kit)
                                .key("field-value"),
                        ),
                    )
                    .child(toggle),
            )
            .child(
                text("Hidden while you type. It is written to the vault encrypted, never to logs.")
                    .font(ts::SM)
                    .color(look.muted),
            ),
        );
    }

    form = form.child(labelled(
        look,
        "Notes",
        TextArea::new(&editor.notes)
            .width(None)
            .label("Notes")
            .rows(4)
            .on_edit(|state: &mut Library| state.editor_field(|editor| &mut editor.notes))
            .build(kit)
            .key("field-notes"),
    ));

    if !command {
        form = form.child(labelled(
            look,
            "Custom fields",
            extra_fields_editor(editor, look, kit),
        ));
    }

    col()
        .key("editor-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(rune::ScrollSpec {
            stick_bottom: false,
        })
        .child(col().w_full().px(size::SP4).py(size::SP4).child(form))
}

fn labelled(look: &Look, label: &str, control: El) -> El {
    col()
        .w_full()
        .gap(6.0)
        .child(widgets::section_label(look, label))
        .child(control)
}

fn extra_fields_editor(editor: &Editor, look: &Look, kit: &Kit) -> El {
    let mut list = col().key("extra-fields").w_full().gap(size::SP2);
    let last = editor.fields.len().saturating_sub(1);
    for (index, field) in editor.fields.iter().enumerate() {
        let display = match field.display {
            FieldDisplay::Text => "Text",
            FieldDisplay::Multiline => "Multiline",
            FieldDisplay::Url => "URL",
        };
        let value: El = if field.display == FieldDisplay::Multiline {
            TextArea::new(&field.value)
                .width(None)
                .label("Field value")
                .rows(3)
                .on_edit(move |state: &mut Library| state.extra_field(index, true))
                .build(kit)
        } else {
            TextInput::new(&field.value)
                .width(None)
                .label("Field value")
                .placeholder(if field.display == FieldDisplay::Url {
                    "https://example.test"
                } else {
                    "Value"
                })
                .on_edit(move |state: &mut Library| state.extra_field(index, true))
                .build(kit)
        };
        let up = widgets::button_sm(look, format!("field-up-{index}"), "↑", None, Variant::Ghost);
        let down = widgets::button_sm(
            look,
            format!("field-down-{index}"),
            "↓",
            None,
            Variant::Ghost,
        );
        list = list.child(
            col()
                .key(format!("extra-field-{}", field.id))
                .w_full()
                .gap(6.0)
                .padding(size::SP2)
                .radius(size::RADIUS)
                .border(1.0, look.line)
                .child(
                    row()
                        .w_full()
                        .gap(size::SP2)
                        .center_y()
                        .child(
                            el().grow(1.0).min_w(0.0).child(
                                TextInput::new(&field.label)
                                    .width(None)
                                    .label("Field label")
                                    .placeholder("Label")
                                    .on_edit(move |state: &mut Library| {
                                        state.extra_field(index, false)
                                    })
                                    .build(kit),
                            ),
                        )
                        .child(
                            widgets::button_sm(
                                look,
                                format!("field-kind-{index}"),
                                display,
                                None,
                                Variant::Default,
                            )
                            .label(format!("Field kind: {display}"))
                            .on_press(move |state: &mut Library| {
                                if let Some(editor) = state.editing.as_mut() {
                                    editor.cycle_display(index);
                                }
                            }),
                        ),
                )
                .child(value)
                .child(
                    row()
                        .gap(size::SP1)
                        .child(if index == 0 {
                            widgets::disabled(up)
                        } else {
                            up.on_press(move |state: &mut Library| {
                                if let Some(editor) = state.editing.as_mut() {
                                    editor.move_field(index, -1);
                                }
                            })
                        })
                        .child(if index == last {
                            widgets::disabled(down)
                        } else {
                            down.on_press(move |state: &mut Library| {
                                if let Some(editor) = state.editing.as_mut() {
                                    editor.move_field(index, 1);
                                }
                            })
                        })
                        .child(
                            widgets::button_sm(
                                look,
                                format!("field-remove-{index}"),
                                "Remove",
                                None,
                                Variant::Danger,
                            )
                            .on_press(move |state: &mut Library| {
                                if let Some(editor) = state.editing.as_mut() {
                                    editor.remove_field(index);
                                }
                            }),
                        ),
                ),
        );
    }
    list.child(
        widgets::button_sm(
            look,
            "add-field",
            "Add field",
            Some(&icons::PLUS),
            Variant::Ghost,
        )
        .on_press(|state: &mut Library| {
            if let Some(editor) = state.editing.as_mut() {
                editor.add_field();
            }
        }),
    )
}
