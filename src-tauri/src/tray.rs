use tauri::menu::{Menu, MenuBuilder, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::error::{AppError, AppResult};
use crate::models::Kind;
use crate::windows;

pub const TRAY_ID: &str = "main-tray";

pub const ID_LIBRARY: &str = "open-library";
pub const ID_FINDER: &str = "open-finder";
pub const ID_ADD_COMMAND: &str = "add-command";
pub const ID_ADD_SECRET: &str = "add-secret";
pub const ID_VAULT: &str = "vault-toggle";
pub const ID_SETTINGS: &str = "settings";
pub const ID_QUIT: &str = "quit";

fn item<R: Runtime>(app: &AppHandle<R>, id: &str, text: &str) -> AppResult<MenuItem<R>> {
    MenuItem::with_id(app, id, text, true, None::<&str>).map_err(|e| AppError::Other(e.to_string()))
}

pub fn build_menu<R: Runtime>(app: &AppHandle<R>, unlocked: bool) -> AppResult<Menu<R>> {
    let vault_text = if unlocked {
        "Lock vault"
    } else {
        "Unlock vault"
    };
    let menu = MenuBuilder::new(app)
        .item(&item(app, ID_LIBRARY, "Open library")?)
        .item(&item(app, ID_FINDER, "Open finder")?)
        .separator()
        .item(&item(app, ID_ADD_COMMAND, "Add command")?)
        .item(&item(app, ID_ADD_SECRET, "Add secret")?)
        .separator()
        .item(&item(app, ID_VAULT, vault_text)?)
        .item(&item(app, ID_SETTINGS, "Settings")?)
        .separator()
        .item(&item(app, ID_QUIT, "Quit")?)
        .build()
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(menu)
}

/// Linux (libappindicator) does not deliver raw tray mouse events through Tauri, so every
/// action has to be reachable from the native context menu.
pub fn build(app: &AppHandle) -> AppResult<()> {
    let unlocked = app
        .state::<std::sync::Arc<crate::state::AppState>>()
        .vault
        .is_unlocked();
    let menu = build_menu(app, unlocked)?;
    let tooltip = tooltip_text(unlocked);
    let icon = app
        .default_window_icon()
        .ok_or_else(|| AppError::Other("application icon is missing".into()))?
        .clone();
    TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip(&tooltip)
        .icon(icon)
        .on_menu_event(|app: &AppHandle, event| {
            let id = event.id().0.as_str();
            match id {
                ID_LIBRARY => {
                    let _ = windows::show_library(app);
                }
                ID_FINDER => {
                    let _ = windows::show_finder(app);
                }
                ID_ADD_COMMAND => {
                    let _ = windows::show_library_with(app, Kind::Command, None);
                }
                ID_ADD_SECRET => {
                    // Unlocking is handled by the library's unlock panel when the vault is locked.
                    let _ = windows::show_library_with(app, Kind::Secret, None);
                }
                ID_VAULT => {
                    let result = {
                        let state = app.state::<std::sync::Arc<crate::state::AppState>>();
                        if state.vault.is_unlocked() {
                            state.vault.lock().map(|()| state.vault.state())
                        } else {
                            state.vault.unlock().map(|()| state.vault.state())
                        }
                    };
                    match result {
                        Ok(vault) => {
                            let _ = app.emit("vault-state-changed", vault);
                            let _ = refresh_menu(app);
                        }
                        Err(error) => {
                            let _ = app.emit("vault-message", error.to_string());
                        }
                    }
                }
                ID_SETTINGS => {
                    if let Ok(()) = windows::show_library(app) {
                        if let Some(window) = app.get_webview_window(windows::LIBRARY) {
                            let _ = window.emit("open-settings", ());
                        }
                    }
                }
                ID_QUIT => {
                    let _ = windows::request_quit(app);
                }
                other => {
                    eprintln!("ignoring unknown tray action {other}");
                }
            }
        })
        .build(app)
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

fn tooltip_text(unlocked: bool) -> String {
    // Only the app name and lock status: never entry names or values.
    format!(
        "Dev Console — vault {}",
        if unlocked { "unlocked" } else { "locked" }
    )
}

pub fn refresh_menu<R: Runtime>(app: &AppHandle<R>) -> AppResult<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let unlocked = app
        .state::<std::sync::Arc<crate::state::AppState>>()
        .vault
        .is_unlocked();
    tray.set_menu(Some(build_menu(app, unlocked)?))
        .map_err(|e| AppError::Other(e.to_string()))?;
    tray.set_tooltip(Some(tooltip_text(unlocked)))
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}
