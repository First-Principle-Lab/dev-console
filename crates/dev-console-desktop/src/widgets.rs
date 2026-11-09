//! The shared controls of `src/components/`: Button, Badge, Keycap, StatusLine, EmptyState and the
//! kind tile, drawn in the Dev Console palette.
//!
//! These return plain elements and leave the handler to the caller (`.on_press(..)`), so one
//! builder serves both windows' state types. A disabled control is drawn dimmed and the caller
//! attaches no handler, which is how the web's `disabled` attribute behaves.

use rune::{col, el, icon, row, text, Cursor, El, Icon};

use crate::icons;
use crate::theme::{size, text as ts, Look};

/// `Button.svelte`'s variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Default,
    Primary,
    Ghost,
    Danger,
}

/// A button: an optional icon, a label, the variant's colours and the hover/press overlays.
pub fn button(
    look: &Look,
    key: impl Into<String>,
    label: &str,
    glyph: Option<&'static Icon>,
    variant: Variant,
) -> El {
    button_sized(look, key, label, glyph, variant, false)
}

/// The `size="sm"` button, as the header and the output panel use it.
pub fn button_sm(
    look: &Look,
    key: impl Into<String>,
    label: &str,
    glyph: Option<&'static Icon>,
    variant: Variant,
) -> El {
    button_sized(look, key, label, glyph, variant, true)
}

fn button_sized(
    look: &Look,
    key: impl Into<String>,
    label: &str,
    glyph: Option<&'static Icon>,
    variant: Variant,
    small: bool,
) -> El {
    let (fill, ink, border) = match variant {
        Variant::Default => (look.raised, look.fg, Some(look.line)),
        Variant::Primary => (look.accent, look.accent_fg, None),
        Variant::Ghost => (rune::Color::TRANSPARENT, look.muted, None),
        Variant::Danger => (rune::Color::TRANSPARENT, look.danger, None),
    };
    let height = if small {
        size::CONTROL_H_SM
    } else {
        size::CONTROL_H
    };
    let mut button = row()
        .key(key)
        .role("button")
        .label(label.to_owned())
        .focusable(true)
        .focus_ring(2.0, 1.0, look.focus)
        .h(height)
        .px(if small { size::SP2 } else { size::SP3 })
        .gap(6.0)
        .center_y()
        .radius(size::RADIUS)
        .fill(fill)
        .hover(look.fg.alpha(0.06))
        .press(look.fg.alpha(0.1))
        .cursor(Cursor::Pointer);
    if let Some(border) = border {
        button = button.border(1.0, border);
    }
    if let Some(glyph) = glyph {
        button = button.child(icon(glyph, 14.0).color(ink));
    }
    let font = if variant == Variant::Primary {
        ts::STRONG
    } else {
        ts::BODY
    };
    button.child(
        text(label.to_owned())
            .font(if small { ts::SM } else { font })
            .color(ink),
    )
}

/// An icon-only button, labelled for assistive technology and the hover tooltip.
pub fn icon_button(
    look: &Look,
    key: impl Into<String>,
    label: &str,
    glyph: &'static Icon,
    tint: rune::Color,
) -> El {
    row()
        .key(key)
        .role("button")
        .label(label.to_owned())
        .help(label.to_owned())
        .focusable(true)
        .focus_ring(2.0, 1.0, look.focus)
        .w(size::CONTROL_H)
        .h(size::CONTROL_H)
        .center()
        .radius(size::RADIUS)
        .hover(look.fg.alpha(0.08))
        .press(look.fg.alpha(0.12))
        .cursor(Cursor::Pointer)
        .child(icon(glyph, 15.0).color(tint))
}

/// Dims a control the caller left without a handler.
pub fn disabled(button: El) -> El {
    button.disabled(true).opacity(0.45).cursor(Cursor::Default)
}

/// `Badge.svelte`: a small rounded chip.
pub fn badge(look: &Look, label: &str, ink: rune::Color, fill: rune::Color) -> El {
    let _ = look;
    row()
        .h(20.0)
        .px(7.0)
        .gap(4.0)
        .center_y()
        .radius(999.0)
        .fill(fill)
        .child(text(label.to_owned()).font(ts::XS).color(ink))
}

/// `Keycap.svelte`: one key of a shortcut hint.
pub fn keycap(look: &Look, label: &str) -> El {
    row()
        .h(20.0)
        .px(6.0)
        .center_y()
        .radius(size::RADIUS_SM)
        .fill(look.raised)
        .border(1.0, look.line)
        .child(text(label.to_owned()).font(ts::XS).color(look.muted))
}

/// A shortcut hint: the keys joined by `+`, then what they do.
pub fn hint(look: &Look, keys: &[&str], action: &str) -> El {
    let mut chip = row().gap(4.0).center_y();
    for (index, key) in keys.iter().enumerate() {
        if index > 0 {
            chip = chip.child(text("+").font(ts::XS).color(look.dim));
        }
        chip = chip.child(keycap(look, key));
    }
    chip.child(text(action.to_owned()).font(ts::XS).color(look.dim))
}

/// `StatusLine.svelte`'s tones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Info,
    Warn,
    Error,
}

/// One line of feedback under the toolbar. Errors are an alert, the rest a polite status.
pub fn status_line(look: &Look, key: &'static str, tone: Tone, message: &str) -> El {
    let (ink, fill, glyph) = match tone {
        Tone::Ok => (look.ok, look.ok_soft, &icons::CHECK),
        Tone::Info => (look.muted, look.raised, &icons::INBOX),
        Tone::Warn => (look.warn, look.warn_soft, &icons::ALERT),
        Tone::Error => (look.danger, look.danger_soft, &icons::ALERT),
    };
    row()
        .key(key)
        .role(if tone == Tone::Error {
            "alert"
        } else {
            "status"
        })
        .label(message.to_owned())
        .w_full()
        .min_h(30.0)
        .px(size::SP3)
        .py(6.0)
        .gap(size::SP2)
        .center_y()
        .radius(size::RADIUS)
        .fill(fill)
        .child(icon(glyph, 14.0).color(ink))
        .child(
            text(message.to_owned())
                .font(ts::SM)
                .color(if tone == Tone::Info { look.fg } else { ink })
                .wrap()
                .grow(1.0),
        )
}

/// `EmptyState.svelte`: a centred tile, a title, a hint and any actions.
pub fn empty_state(
    look: &Look,
    key: &'static str,
    glyph: &'static Icon,
    title: &str,
    hint_text: &str,
    actions: Vec<El>,
) -> El {
    let mut actions_row = row().gap(size::SP2).center_y();
    for action in actions {
        actions_row = actions_row.child(action);
    }
    col()
        .key(key)
        .w_full()
        .h_full()
        .center()
        .gap(size::SP2)
        .px(size::SP5)
        .child(
            row()
                .w(size::TILE_LG + 8.0)
                .h(size::TILE_LG + 8.0)
                .center()
                .radius(size::RADIUS_LG)
                .fill(look.raised)
                .border(1.0, look.line)
                .child(icon(glyph, 18.0).color(look.muted)),
        )
        .child(text(title.to_owned()).font(ts::LG).color(look.fg))
        .child(
            text(hint_text.to_owned())
                .font(ts::SM)
                .color(look.muted)
                .wrap()
                .max_w(380.0)
                .align(rune::Align::Center),
        )
        .child(el().h(size::SP1))
        .child(actions_row)
}

/// The kind glyph on a tinted tile: a terminal for commands, a key for secrets.
pub fn kind_tile(look: &Look, secret: bool, side: f32) -> El {
    let (glyph, ink, fill) = if secret {
        (&icons::SECRET, look.kind_secret, look.secret_soft)
    } else {
        (&icons::COMMAND, look.kind_command, look.accent_soft)
    };
    row()
        .w(side)
        .h(side)
        .center()
        .radius(size::RADIUS)
        .fill(fill)
        .child(icon(glyph, side * 0.55).color(ink))
}

/// An uppercase section heading (`h3` in the details pane).
pub fn section_label(look: &Look, label: &str) -> El {
    text(label.to_uppercase()).font(ts::LABEL).color(look.muted)
}
