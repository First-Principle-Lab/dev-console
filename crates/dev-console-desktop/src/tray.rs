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

/// The app logo at 32 px: the PNG the desktop entry's icon theme installs, so the tray shows the same
/// mark as the launcher and the dock. It lives with the Tauri icons until the cutover moves them,
/// as `scripts/build-appimage.sh` reads them from there too.
const LOGO_PNG: &[u8] = include_bytes!("../../../src-tauri/icons/32x32.png");

/// The logo as straight (not premultiplied) RGBA rows, with its edge length.
///
/// The tray host scales it to its own size. The shipped Tauri tray drew the app icon; an earlier
/// native build drew a plain grey disc here instead, which a tray of other apps' logos showed as a
/// blank dot.
pub fn logo_rgba() -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(LOGO_PNG));
    let mut reader = decoder.read_info().expect("the embedded logo is a PNG");
    let mut buffer = vec![
        0;
        reader
            .output_buffer_size()
            .expect("the logo fits in memory")
    ];
    let frame = reader
        .next_frame(&mut buffer)
        .expect("the embedded logo decodes");
    buffer.truncate(frame.buffer_size());
    // The asset is 8-bit RGBA; the test below would fail first if it were ever re-exported otherwise.
    assert_eq!(frame.color_type, png::ColorType::Rgba, "the logo is RGBA");
    (frame.width, frame.height, buffer)
}

pub fn tray_icon() -> TrayIcon {
    let (width, height, rgba) = logo_rgba();
    TrayIcon::rgba(width, height, rgba)
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
    fn the_tray_shows_the_launcher_logo() {
        let (width, height, pixels) = logo_rgba();
        assert_eq!((width, height), (32, 32));
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        let at = |x: u32, y: u32| {
            let i = ((y * width + x) * 4) as usize;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
        assert_eq!(at(0, 0)[3], 0, "the rounded corner is transparent");
        let tile = at(4, 16);
        assert_eq!(tile[3], 255, "the tile is opaque");
        assert!(
            tile[..3].iter().all(|&channel| channel < 80),
            "the tile is the logo's dark ground, not a light disc: {tile:?}"
        );
        // The `>` stroke is the one saturated colour in the mark: blue well above red and green.
        let accent = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| at(x, y))
            .any(|[r, g, b, a]| a == 255 && b > 200 && b > r + 40 && b > g + 40);
        assert!(accent, "the blue chevron is drawn");
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
