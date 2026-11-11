//! The Library's dialogs: the shared shell of `Dialog.svelte` and the four confirmations and
//! panels `Library.svelte` puts in it (delete, run, command output, unsaved changes).
//!
//! Rune's `modal` owns the focus trap, Escape, the inert background and restoring focus to the
//! opener, so the shell only has to draw the scrim and the panel and name the dismiss route.

use dev_console_core::models::{EntryDetails, EntryMeta};
use rune::{col, el, row, text, El, ScrollSpec};

use super::{Library, RunOutput, RunPhase, Stream};
use crate::icons;
use crate::theme::{size, text as ts, Look};
use crate::widgets::{self, Tone, Variant};

/// A modal panel over a scrim. `dismiss` is what Escape and a press outside the panel do.
///
/// The frame is `.surface-shell` (`Dialog.svelte`, `tokens.css`): `--shell-bg` fill, `--line-shell`
/// edge, `--radius-shell` arc, the `--shadow-shell` elevation and the `--shell-edge` inner top
/// highlight. One surface paints the whole panel, so the footer band carries only the hairline rule
/// `.foot { border-top: var(--border-hair) }` draws - never its own fill, which would read as a
/// second palette wherever `--bg-panel` and `--shell-bg` differ.
pub fn shell(
    look: &Look,
    key: &str,
    title: &str,
    wide: bool,
    body: Vec<El>,
    footer: Vec<El>,
    dismiss: impl Fn(&mut Library) + Clone + 'static,
) -> El {
    shell_sized(look, key, title, wide, None, body, footer, dismiss)
}

/// [`shell`] with a body of a set height (less in a window too short for it), for a sheet whose
/// pages differ in length: the sheet must not change size as the page does.
#[allow(clippy::too_many_arguments)]
pub fn shell_sized(
    look: &Look,
    key: &str,
    title: &str,
    wide: bool,
    body_height: Option<f32>,
    body: Vec<El>,
    footer: Vec<El>,
    dismiss: impl Fn(&mut Library) + Clone + 'static,
) -> El {
    let mut content = col()
        .key(format!("{key}-body"))
        .w_full()
        .gap(if wide { size::SP4 } else { size::SP3 })
        .px(size::SP5)
        .pb(size::SP5);
    for child in body {
        content = content.child(child);
    }
    let mut actions = row()
        .key(format!("{key}-footer"))
        .w_full()
        .gap(size::SP2)
        .center_y()
        .px(size::SP5)
        .py(size::SP3);
    for child in footer {
        actions = actions.child(child);
    }
    let panel = col()
        .key(key.to_owned())
        .role("dialog")
        .label(title.to_owned())
        .w_full()
        .max_w(if wide { 640.0 } else { 440.0 })
        .min_h(0.0)
        .shrink()
        .radius(size::RADIUS_SHELL)
        .clip_rounded(size::RADIUS_SHELL)
        .fill(look.shell)
        .border(1.0, look.line_shell)
        // `--shadow-shell`: 24px down, 60px blur, black at 45%.
        .shadow(24.0, 60.0, rune::Color::BLACK.alpha(0.45))
        // The `inset 0 1px 0 var(--shell-edge)` highlight of `.surface-shell`.
        .child(el().h(1.0).w_full().fill(look.shell_edge))
        .child(
            row()
                .w_full()
                .px(size::SP5)
                .pt(size::SP5)
                .pb(size::SP3)
                .child(
                    text(title.to_owned())
                        .key(format!("{key}-title"))
                        .font(ts::LG)
                        .color(look.fg)
                        .wrap(),
                ),
        )
        .child({
            let viewport = col().w_full().min_h(0.0).shrink().scroll(ScrollSpec {
                stick_bottom: false,
            });
            match body_height {
                Some(height) => viewport.h(height),
                None => viewport.max_h(if wide { 560.0 } else { 460.0 }),
            }
            .child(content)
        })
        .child(el().h(1.0).w_full().fill(look.line_hair))
        .child(actions)
        .modal(true, dismiss);
    col()
        .key(format!("{key}-layer"))
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .padding(size::SP4)
        .center()
        .fill(look.backdrop)
        .child(panel)
}

fn paragraph(look: &Look, words: &str) -> El {
    text(words.to_owned())
        .font(ts::BODY)
        .color(look.muted)
        .wrap()
}

fn code_block(look: &Look, key: &str, words: &str) -> El {
    row()
        .w_full()
        .px(size::SP3)
        .py(10.0)
        .radius(size::RADIUS_LG - 2.0)
        .fill(look.sunken)
        .child(
            text(words.to_owned())
                .key(key.to_owned())
                .font(ts::MONO)
                .color(look.fg)
                .selectable(true)
                .wrap_anywhere()
                .grow(1.0),
        )
}

pub fn delete(target: &EntryMeta, look: &Look) -> El {
    shell(
        look,
        "dialog",
        &format!("Delete “{}”?", target.name),
        false,
        vec![paragraph(
            look,
            &format!(
                "The {} is removed for good. Backups made earlier still hold it.",
                target.kind.as_str()
            ),
        )],
        vec![
            el().grow(1.0),
            widgets::button(look, "cancel-delete", "Cancel", None, Variant::Default)
                .on_press(|state: &mut Library| state.cancel_delete()),
            widgets::button(look, "confirm-delete", "Delete", None, Variant::Danger)
                .fill(look.danger_soft)
                .on_press(|state: &mut Library| state.confirm_delete()),
        ],
        |state: &mut Library| state.cancel_delete(),
    )
}

pub fn run(target: &EntryDetails, look: &Look) -> El {
    let mut body = vec![
        paragraph(
            look,
            "Output shows here. A command that uses sudo opens your terminal instead.",
        ),
        code_block(
            look,
            "run-command",
            target.command.as_deref().unwrap_or_default(),
        ),
    ];
    if !target.working_directory.is_empty() {
        body.push(
            row()
                .gap(6.0)
                .center_y()
                .help(target.working_directory.clone())
                .child(rune::icon(&icons::FOLDER, 12.0).color(look.dim))
                .child(
                    text(widgets::short_path(&target.working_directory))
                        .label(format!("Working directory: {}", target.working_directory))
                        .font(ts::MONO_SM)
                        .color(look.muted)
                        .truncate(),
                ),
        );
    }
    shell(
        look,
        "run-dialog",
        "Run this command?",
        false,
        body,
        vec![
            el().grow(1.0),
            widgets::button(look, "cancel-run", "Cancel", None, Variant::Default)
                .on_press(|state: &mut Library| state.cancel_run()),
            widgets::button(look, "confirm-run", "Run", None, Variant::Primary)
                .on_press(|state: &mut Library| state.confirm_run()),
        ],
        |state: &mut Library| state.cancel_run(),
    )
}

pub fn output(state: &Library, output: &RunOutput, look: &Look) -> El {
    let title = match output.phase {
        RunPhase::Running => format!("Running “{}”…", output.name),
        _ => format!("Result: {}", output.name),
    };
    let mut body = Vec::new();
    match &output.phase {
        RunPhase::Running => body.push(
            text("Running. Output appears when it finishes; closing this does not stop it.")
                .key("command-running")
                .role("status")
                .font(ts::BODY)
                .color(look.muted)
                .wrap(),
        ),
        RunPhase::Error(error) => body.push(widgets::status_line(
            look,
            "command-error",
            Tone::Error,
            error,
        )),
        RunPhase::Done(result) => {
            let failed = result.timed_out || result.exit_code != Some(0);
            let mut summary = if result.timed_out {
                "Timed out after 120 seconds.".to_string()
            } else {
                match result.exit_code {
                    None => "Stopped by a signal.".to_string(),
                    Some(code) => format!("Exit code {code}."),
                }
            };
            if result.truncated {
                summary.push_str(" Output was shortened to 64 KiB per stream.");
            }
            body.push(widgets::status_line(
                look,
                "command-exit",
                if failed { Tone::Error } else { Tone::Ok },
                &summary,
            ));
            for (stream, heading, label, value) in [
                (Stream::Stdout, "Output", "Copy output", &result.stdout),
                (Stream::Stderr, "Errors", "Copy errors", &result.stderr),
            ] {
                if value.is_empty() {
                    continue;
                }
                let key = if stream == Stream::Stdout {
                    "stdout"
                } else {
                    "stderr"
                };
                let copied = state.copied_stream == Some(stream);
                let button = widgets::button_sm(
                    look,
                    format!("copy-command-{key}"),
                    if copied { "Copied" } else { label },
                    Some(&icons::COPY),
                    Variant::Default,
                );
                body.push(
                    col()
                        .w_full()
                        .gap(6.0)
                        .child(
                            row()
                                .w_full()
                                .center_y()
                                .child(widgets::section_label(look, heading))
                                .child(el().grow(1.0))
                                .child(if state.copying_stream.is_some() {
                                    widgets::disabled(button)
                                } else {
                                    button.on_press(move |state: &mut Library| {
                                        state.copy_run_stream(stream)
                                    })
                                }),
                        )
                        .child(
                            col()
                                .w_full()
                                .max_h(240.0)
                                .scroll(ScrollSpec {
                                    stick_bottom: false,
                                })
                                .radius(size::RADIUS_LG - 2.0)
                                .fill(look.sunken)
                                .child(
                                    text(value.clone())
                                        .key(format!("command-{key}"))
                                        .font(ts::MONO_SM)
                                        .color(look.fg)
                                        .selectable(true)
                                        .wrap_anywhere()
                                        .w_full()
                                        .padding(size::SP3),
                                ),
                        ),
                );
            }
            if let Some(error) = &state.output_copy_error {
                body.push(widgets::status_line(
                    look,
                    "output-copy-error",
                    Tone::Error,
                    error,
                ));
            }
            if result.stdout.is_empty() && result.stderr.is_empty() {
                body.push(
                    text("The command produced no output.")
                        .font(ts::BODY)
                        .color(look.muted),
                );
            }
        }
    }
    shell(
        look,
        "output-dialog",
        &title,
        true,
        body,
        vec![
            el().grow(1.0),
            widgets::button(
                look,
                "close-command-output",
                "Close",
                None,
                Variant::Default,
            )
            .on_press(|state: &mut Library| state.close_run_output()),
        ],
        |state: &mut Library| state.close_run_output(),
    )
}

pub fn unsaved(look: &Look) -> El {
    shell(
        look,
        "prompt",
        "Unsaved changes",
        false,
        vec![paragraph(look, "This entry has edits that are not saved.")],
        vec![
            el().grow(1.0),
            widgets::button(look, "close-cancel", "Keep editing", None, Variant::Default)
                .on_press(|state: &mut Library| state.keep_editing()),
            widgets::button(look, "close-discard", "Discard", None, Variant::Default)
                .on_press(|state: &mut Library| state.discard_and_close()),
            widgets::button(look, "close-save", "Save", None, Variant::Primary)
                .on_press(|state: &mut Library| state.save_and_close()),
        ],
        |state: &mut Library| state.keep_editing(),
    )
}
