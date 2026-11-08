//! StatusNotifierItem tray, built on `rune::Tray`.
//!
//! Plan §3 ("Tray") requires the current menu to survive the port - Library, Finder, the vault
//! action, Settings and Quit - and requires that *the absence of a tray host must not prevent the
//! app from operating*. [`attach`]) encodes that second rule: a `TrayError` is reported and the app
//! carries on windowless, because on a headless host, over ssh, or on a desktop with no
//! StatusNotifierItem watcher there is nothing to register with.
//!
//! The vault action and Settings are present but *disabled* here. Their behaviour depends on the
//! vault and settings services that plan milestone 2 moves into `dev-console-core`; a tray item
//! that looks live and does nothing is worse than one that says it is not available yet, so the
//! items keep their position in the menu (a stable shape for the desktop check) with `enabled`
//! false, and milestone 2 flips them on together with the service calls behind them.
//!
//! ## Verified here, and not
//!
//! The icon buffer's shape and the menu's contents and ordering are unit-tested: they need no D-Bus.
//! Whether a real tray host shows the icon, whether the primary click is delivered as
//! `TrayEvent::Activate` with a usable XDG activation token, and whether the menu's separators
//! render as expected are all UNVERIFIED without a session bus with a watcher.

use rune::{AppHandle, Tray, TrayError, TrayEvent, TrayIcon, TrayItem, TraySpec};

use crate::identity::{FINDER_TITLE, LIBRARY_TITLE};
use crate::ui::{Finder, Library};

/// StatusNotifierItem / application id. Same string as the app id set for Wayland, so the tray
/// entry, the window's `app_id` and the desktop file agree.
pub const TRAY_ID: &str = "dev-console";

/// Menu id: show and focus the Library window.
pub const MENU_LIBRARY: &str = "library";
/// Menu id: show and focus the Finder.
pub const MENU_FINDER: &str = "finder";
/// Menu id: the lock/unlock action. Disabled until milestone 2 moves the vault service.
pub const MENU_VAULT: &str = "vault.toggle";
/// Menu id: open Settings. Disabled until milestone 2 moves the settings service.
pub const MENU_SETTINGS: &str = "settings";
/// Menu id: quit the resident process.
pub const MENU_QUIT: &str = "quit";

/// Tray icon edge length in device pixels. 22 matches the size the current Tauri tray icon uses and
/// the Size hint most StatusNotifierItem watchers request.
pub const ICON_SIZE: u32 = 22;

/// A plain filled disc, straight (not premultiplied) RGBA.
///
/// Deliberately not the app's real mark: the Lucide-matching SVG set is milestone 3/4 work and
/// `rune::TrayIcon` takes raw RGBA rather than an SVG, so the rasterisation step belongs with the
/// icon port rather than being invented twice. The shape only has to be recognisable at 22px for
/// the desktop check to confirm the icon registers.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let radius = (size as f32 - 1.0) / 2.0;
    let centre = radius;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - centre;
            let dy = y as f32 - centre;
            let inside = (dx * dx + dy * dy).sqrt() <= radius;
            // A neutral light grey: the tray's own theme decides how it reads, and AGENTS.md keeps
            // colours out of view code rather than hard-coding one here.
            let channel = if inside { 232 } else { 0 };
            let alpha = if inside { 255 } else { 0 };
            pixels.extend_from_slice(&[channel, channel, channel, alpha]);
        }
    }
    pixels
}

/// The tray icon.
pub fn tray_icon() -> TrayIcon {
    TrayIcon::rgba(ICON_SIZE, ICON_SIZE, icon_rgba(ICON_SIZE))
}

/// The menu, in the order the current app shows it.
pub fn menu() -> Vec<TrayItem> {
    vec![
        TrayItem::action(MENU_LIBRARY, format!("Show {LIBRARY_TITLE}")),
        TrayItem::action(MENU_FINDER, format!("Show {FINDER_TITLE}")),
        TrayItem::Separator,
        // Enabled by milestone 2, together with the vault service call behind it.
        TrayItem::action(MENU_VAULT, "Lock or unlock the vault (not available yet)").enabled(false),
        TrayItem::action(MENU_SETTINGS, "Settings (not available yet)").enabled(false),
        TrayItem::Separator,
        TrayItem::action(MENU_QUIT, "Quit"),
    ]
}

/// Registers with the tray host, or explains why it could not and returns `None`.
///
/// The returned `Tray` must be kept alive for as long as the tray should exist; dropping it
/// unregisters. A failure is *not* fatal: no tray host, no session bus and no desktop at all all
/// land here, and the app still answers its control socket and opens windows (plan §3).
pub fn attach(library: &AppHandle<Library>, finder: &AppHandle<Finder>) -> Option<Tray> {
    let spec = TraySpec::new(TRAY_ID, LIBRARY_TITLE, tray_icon())
        .tooltip(format!("{LIBRARY_TITLE} — right-click for the menu"))
        .menu(menu());

    let library_on_event = library.clone();
    let finder_on_event = finder.clone();
    match Tray::new(spec, move |event| match event {
        // The icon's primary click: show the Finder, which is what the tray is mostly for, and use
        // the activation token the host hands over when there is one so focus follows the click.
        TrayEvent::Activate { token } => match token {
            Some(token) => finder_on_event.activate(token),
            None => finder_on_event.open(),
        },
        TrayEvent::Menu(id) => match id.as_str() {
            MENU_LIBRARY => library_on_event.open(),
            MENU_FINDER => finder_on_event.open(),
            MENU_QUIT => library_on_event.quit(),
            // Disabled items cannot be chosen; an id added later should not be silently ignored.
            other => eprintln!("dev-console: unknown tray menu item {other:?}"),
        },
        // `TrayEvent` is `#[non_exhaustive]`: a host event this build does not handle is reported
        // rather than dropped in silence.
        _ => eprintln!("dev-console: unhandled tray event"),
    }) {
        Ok(tray) => Some(tray),
        Err(TrayError::Unsupported) => {
            eprintln!("dev-console: no system tray on this platform; the app still runs resident");
            None
        }
        Err(error @ TrayError::Backend(_)) => {
            eprintln!("dev-console: tray registration failed ({error}); the app still runs resident");
            None
        }
        // `TrayError` is `#[non_exhaustive]`: a failure mode added upstream is reported too.
        Err(error) => {
            eprintln!("dev-console: tray registration failed ({error}); the app still runs resident");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_buffer_matches_its_own_dimensions() {
        let pixels = icon_rgba(ICON_SIZE);
        assert_eq!(
            pixels.len(),
            (ICON_SIZE * ICON_SIZE * 4) as usize,
            "`Tray::new` rejects an icon whose byte count contradicts its size"
        );
        // Straight, not premultiplied: a covered texel must carry a colour for the watcher to tint.
        let centre = ((ICON_SIZE / 2) * ICON_SIZE + ICON_SIZE / 2) as usize * 4;
        assert_eq!(pixels[centre + 3], 255, "the disc centre is opaque");
        assert_eq!(pixels[0 + 3], 0, "the corner outside the disc is transparent");
    }

    #[test]
    fn an_outside_texel_carries_no_colour_so_it_cannot_haloe() {
        let pixels = icon_rgba(8);
        assert_eq!(&pixels[0..4], &[0, 0, 0, 0], "a transparent tray texel is fully zeroed");
    }

    #[test]
    fn the_menu_keeps_the_current_apps_order_and_gates_the_unwired_actions() {
        let items = menu();
        let ids: Vec<Option<&str>> = items
            .iter()
            .map(|item| match item {
                TrayItem::Action { id, enabled, .. } => {
                    assert_eq!(
                        *enabled,
                        matches!(id.as_str(), MENU_LIBRARY | MENU_FINDER | MENU_QUIT),
                        "{id} is enabled without the service that backs it"
                    );
                    Some(id.as_str())
                }
                TrayItem::Separator => None,
                // `TrayItem` is `#[non_exhaustive]`: a future item type must be reported by this test
                // rather than assumed to be an action or a separator.
                _ => Some("<unknown>"),
            })
            .collect();
        assert_eq!(
            ids,
            vec![
                Some(MENU_LIBRARY),
                Some(MENU_FINDER),
                None,
                Some(MENU_VAULT),
                Some(MENU_SETTINGS),
                None,
                Some(MENU_QUIT)
            ],
            "the tray's shape is part of the desktop contract the owner checks by hand"
        );
    }

    #[test]
    fn menu_ids_are_unique_and_match_their_labels_purpose() {
        let items = menu();
        let mut ids: Vec<&str> = Vec::new();
        for item in &items {
            if let TrayItem::Action { id, label, .. } = item {
                assert!(!label.trim().is_empty(), "{id} has an empty label");
                assert!(!ids.contains(&id.as_str()), "{id} appears twice");
                ids.push(id);
            }
        }
        assert!(ids.contains(&MENU_QUIT));
    }

    #[test]
    fn the_tray_id_is_the_application_identity() {
        assert_eq!(TRAY_ID, crate::APP_ID, "the tray entry, the app id and the desktop file must agree");
    }
}
