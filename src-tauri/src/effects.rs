//! The runtime half of the service contract: what an [`Effects`] value *does* in a desktop process.
//!
//! A service call in `dev-console-core` answers with a value plus the events and window actions it
//! asked for. Nothing in the core can name a webview, a tray item or a process, so the turning of
//! those requests into Tauri calls lives here, in one place:
//!
//!   * [`deliver`] is the only `emit` in this crate. Every event the windows hear goes through it,
//!     so a name, a payload shape and a target exist once (plan section 3: typed events rather than
//!     a string repeated at each call site). `grep '\.emit(' src-tauri/src` is empty by design.
//!   * [`perform`] is the only place a window is shown, hidden, or a quit is finished.
//!   * [`dispatch`] applies one [`Effects`] value in the order the service asked for.
//!
//! The same three functions are what the native (Rune) shell will have to provide later: it replaces
//! the window and tray layer underneath them, not the calls above.

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::error::{AppError, AppResult};
use crate::services::{AppEvent, Delivery, Effects, WindowCommand};
use crate::{tray, windows};

/// Sends one event to the window(s) its variant names.
///
/// The `Result` is returned rather than discarded, because the sites that used to `?` a failed
/// `emit` still do; the ones that ignored it carry on ignoring it. A broadcast (`Delivery::All`) is
/// the `AppHandle::emit` the old call site used, and a window-targeted variant is a `WebviewWindow`
/// emit on that label - a window that is not there simply hears nothing, which is what the old
/// `if let Some(window) = ...` sites did.
pub fn deliver<R: Runtime>(app: &AppHandle<R>, event: &AppEvent) -> AppResult<()> {
    let name = event.name();
    let payload = event.payload();
    match event.delivery() {
        Delivery::All => app
            .emit(name, payload)
            .map_err(|error| AppError::Other(format!("{name}: {error}"))),
        Delivery::Library => send_to(app, windows::LIBRARY, name, payload),
        Delivery::Finder => send_to(app, windows::FINDER, name, payload),
    }
}

fn send_to<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    name: &str,
    payload: serde_json::Value,
) -> AppResult<()> {
    let Some(window) = app.get_webview_window(label) else {
        return Ok(());
    };
    window
        .emit(name, payload)
        .map_err(|error| AppError::Other(format!("{name}: {error}")))
}

/// Carries out one window action. The services decide *which* one; only here does it become a call
/// on a window, and only here can a window action fail.
pub fn perform<R: Runtime>(app: &AppHandle<R>, command: WindowCommand) -> AppResult<()> {
    match command {
        WindowCommand::ShowLibrary { kind, entry_id } => {
            windows::show_library_with(app, kind, entry_id)
        }
        WindowCommand::Hide { surface } => {
            let window = app.get_webview_window(surface.label()).ok_or_else(|| {
                AppError::Other(format!("there is no {} window to hide", surface.label()))
            })?;
            windows::hide(&window)
        }
        WindowCommand::FinishQuit => windows::finish_quit(app),
        WindowCommand::CancelQuit => {
            windows::cancel_quit(app);
            Ok(())
        }
        WindowCommand::Quit => windows::request_quit(app),
        WindowCommand::Restart => windows::restart(app),
    }
}

/// Applies everything one call asked for, in the order it asked for it: the events go out first, so
/// a window that reloads because of them sees the state the call also reported, and the tray's lock
/// item is refreshed last, once, however many vault events went out.
///
/// The `Result` is only ever about a window action or a failed relaunch. An `emit` that no webview
/// accepted and a desktop with no status notifier are both survivable, and were ignored before this
/// module existed, so they are ignored here.
pub fn dispatch<R: Runtime>(app: &AppHandle<R>, effects: Effects) -> AppResult<()> {
    let Effects {
        events,
        windows,
        tray_refresh,
    } = effects;
    for event in &events {
        let _ = deliver(app, event);
    }
    for command in windows {
        perform(app, command)?;
    }
    if tray_refresh {
        let _ = tray::refresh_menu(app);
    }
    Ok(())
}

/// A call whose whole answer is one window action: no events, nothing to refresh.
pub fn window_command(command: WindowCommand) -> Effects {
    let mut effects = Effects::default();
    effects.window(command);
    effects
}

/// Chooses what a handler returns when both the call and the effects it produced can fail.
///
/// The refusal the *call* decided on is the answer the window needs - "the vault is locked", "this
/// window is not allowed to call finder" - and it is raised before the effects are applied, so an
/// effect that then failed must not overwrite it. A successful call whose window action failed is
/// the opposite case: the failure is real and gets reported.
pub fn finish<T>(value: AppResult<T>, effects: AppResult<()>) -> AppResult<T> {
    match value {
        Ok(value) => effects.map(|_| value),
        error => error,
    }
}
