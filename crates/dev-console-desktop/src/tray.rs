//! StatusNotifierItem tray, built on `rune::Tray`: the menu of `src-tauri/src/tray.rs`.
//!
//! Open library, Open finder, Add command, Add secret, Lock/Unlock vault, Settings and Quit, with the
//! vault item and the tooltip following the vault state ([`refresh`]). The absence of a tray host is
//! reported and the app carries on resident: the control socket and the compositor shortcut still
//! open the windows.

use std::sync::Arc;

use dev_console_core::models::Kind;
use dev_console_core::services::{Effects, WindowCommand};
use rune::{Tray, TrayError, TrayEvent, TrayIcon, TrayItem, TraySpec};

use crate::hub::Hub;
use crate::identity::LIBRARY_TITLE;

/// StatusNotifierItem / application id. Same string as the app id set for Wayland, so the tray
/// entry, the window's `app_id` and the desktop file agree.
pub const TRAY_ID: &str = "dev-console";

pub const MENU_LIBRARY: &str = "open-library";
pub const MENU_FINDER: &str = "open-finder";
pub const MENU_ADD_COMMAND: &str = "add-command";
pub const MENU_ADD_SECRET: &str = "add-secret";
pub const MENU_VAULT: &str = "vault-toggle";
pub const MENU_SETTINGS: &str = "settings";
pub const MENU_QUIT: &str = "quit";

/// Tray icon edge length in device pixels.
pub const ICON_SIZE: u32 = 22;

/// A plain filled disc, straight (not premultiplied) RGBA. The tray's own theme decides how it
/// reads; a neutral light grey keeps it visible on dark and light bars.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let radius = (size as f32 - 1.0) / 2.0;
    let centre = radius;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - centre;
            let dy = y as f32 - centre;
            let inside = (dx * dx + dy * dy).sqrt() <= radius;
            let channel = if inside { 232 } else { 0 };
            let alpha = if inside { 255 } else { 0 };
            pixels.extend_from_slice(&[channel, channel, channel, alpha]);
        }
    }
    pixels
}

pub fn tray_icon() -> TrayIcon {
    TrayIcon::rgba(ICON_SIZE, ICON_SIZE, icon_rgba(ICON_SIZE))
}

/// The menu, in the order the shipped app shows it.
pub fn menu(unlocked: bool) -> Vec<TrayItem> {
    vec![
        TrayItem::action(MENU_LIBRARY, "Open library"),
        TrayItem::action(MENU_FINDER, "Open finder"),
        TrayItem::Separator,
        TrayItem::action(MENU_ADD_COMMAND, "Add command"),
        TrayItem::action(MENU_ADD_SECRET, "Add secret"),
        TrayItem::Separator,
        TrayItem::action(
            MENU_VAULT,
            if unlocked {
                "Lock vault"
            } else {
                "Unlock vault"
            },
        ),
        TrayItem::action(MENU_SETTINGS, "Settings"),
        TrayItem::Separator,
        TrayItem::action(MENU_QUIT, "Quit"),
    ]
}

pub fn tooltip(unlocked: bool) -> String {
    format!(
        "Dev Console — vault {}",
        if unlocked { "unlocked" } else { "locked" }
    )
}

/// What picking a menu id asks the resident process to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    ShowLibrary,
    ShowFinder,
    New(Kind),
    ToggleVault,
    Settings,
    Quit,
    Unknown,
}

pub fn menu_action(id: &str) -> MenuAction {
    match id {
        MENU_LIBRARY => MenuAction::ShowLibrary,
        MENU_FINDER => MenuAction::ShowFinder,
        MENU_ADD_COMMAND => MenuAction::New(Kind::Command),
        MENU_ADD_SECRET => MenuAction::New(Kind::Secret),
        MENU_VAULT => MenuAction::ToggleVault,
        MENU_SETTINGS => MenuAction::Settings,
        MENU_QUIT => MenuAction::Quit,
        _ => MenuAction::Unknown,
    }
}

fn perform(hub: &Hub, action: MenuAction) {
    match action {
        MenuAction::ShowLibrary => {
            if let Some(windows) = hub.windows() {
                windows.library.open();
            }
        }
        MenuAction::ShowFinder => hub.show_finder(None),
        MenuAction::New(kind) => hub.perform(WindowCommand::ShowLibrary {
            kind,
            entry_id: None,
        }),
        MenuAction::ToggleVault => {
            let hub = hub.clone();
            // Unlocking asks polkit, which blocks; never on the tray's thread.
            std::thread::spawn(move || {
                let mut effects = Effects::default();
                let result = if hub.library.app_state().vault.unlocked {
                    hub.library.lock_vault(&mut effects)
                } else {
                    hub.library.unlock_vault(&mut effects)
                };
                if let Err(error) = result {
                    eprintln!("dev-console: vault action from the tray failed: {error}");
                }
                hub.dispatch(effects);
            });
        }
        MenuAction::Settings => {
            let mut effects = Effects::default();
            hub.library.open_settings(&mut effects);
            hub.dispatch(effects);
            if let Some(windows) = hub.windows() {
                windows.library.open();
            }
        }
        MenuAction::Quit => hub.perform(WindowCommand::Quit),
        MenuAction::Unknown => eprintln!("dev-console: unknown tray menu item"),
    }
}

/// Registers with the tray host, or explains why it could not and returns `None`.
pub fn attach(hub: &Hub) -> Option<Arc<Tray>> {
    let unlocked = hub.library.app_state().vault.unlocked;
    let spec = TraySpec::new(TRAY_ID, LIBRARY_TITLE, tray_icon())
        .tooltip(tooltip(unlocked))
        .menu(menu(unlocked));
    let on_event = hub.clone();
    let result = Tray::new(spec, move |event| match event {
        // The icon's primary click shows the Finder, with the host's activation token when there is
        // one so focus follows the click.
        TrayEvent::Activate { token } => on_event.show_finder(token),
        TrayEvent::Menu(id) => perform(&on_event, menu_action(&id)),
        _ => {}
    });
    match result {
        Ok(tray) => Some(Arc::new(tray)),
        Err(TrayError::Unsupported) => {
            eprintln!("dev-console: no system tray on this platform; the app still runs resident");
            None
        }
        Err(error) => {
            eprintln!(
                "dev-console: tray registration failed ({error}); the app still runs resident"
            );
            None
        }
    }
}

/// Makes the vault item and the tooltip follow the vault.
pub fn refresh(tray: &Tray, unlocked: bool) {
    tray.set_menu(menu(unlocked));
    tray.set_tooltip(tooltip(unlocked));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_buffer_matches_its_own_dimensions() {
        let pixels = icon_rgba(ICON_SIZE);
        assert_eq!(pixels.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        let centre = ((ICON_SIZE / 2) * ICON_SIZE + ICON_SIZE / 2) as usize * 4;
        assert_eq!(pixels[centre + 3], 255, "the disc centre is opaque");
        assert_eq!(
            &pixels[0..4],
            &[0, 0, 0, 0],
            "a transparent texel is fully zeroed"
        );
    }

    #[test]
    fn the_menu_matches_the_shipped_tray_and_follows_the_vault() {
        let ids = |unlocked| -> Vec<Option<String>> {
            menu(unlocked)
                .into_iter()
                .map(|item| match item {
                    TrayItem::Action { id, .. } => Some(id),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            ids(true),
            [
                Some(MENU_LIBRARY),
                Some(MENU_FINDER),
                None,
                Some(MENU_ADD_COMMAND),
                Some(MENU_ADD_SECRET),
                None,
                Some(MENU_VAULT),
                Some(MENU_SETTINGS),
                None,
                Some(MENU_QUIT)
            ]
            .map(|id| id.map(str::to_owned))
            .to_vec()
        );
        let label = |unlocked| {
            menu(unlocked).into_iter().find_map(|item| match item {
                TrayItem::Action { id, label, .. } if id == MENU_VAULT => Some(label),
                _ => None,
            })
        };
        assert_eq!(label(true).as_deref(), Some("Lock vault"));
        assert_eq!(label(false).as_deref(), Some("Unlock vault"));
        assert_eq!(tooltip(false), "Dev Console — vault locked");
    }

    #[test]
    fn every_menu_item_has_an_action() {
        for item in menu(true) {
            if let TrayItem::Action { id, enabled, .. } = item {
                assert!(enabled, "{id} is offered disabled");
                assert_ne!(
                    menu_action(&id),
                    MenuAction::Unknown,
                    "{id} has no behaviour"
                );
            }
        }
        assert_eq!(menu_action("something-else"), MenuAction::Unknown);
    }

    #[test]
    fn the_tray_id_is_the_application_identity() {
        assert_eq!(TRAY_ID, crate::APP_ID);
    }
}
