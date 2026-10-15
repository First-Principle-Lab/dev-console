use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const LIBRARY: &str = "library";
pub const FINDER: &str = "finder";

/// Creates a window once and reuses it afterwards, so repeated launches never duplicate a
/// window or a process.
fn ensure<R: Runtime>(app: &AppHandle<R>, label: &str, url: &str) -> AppResult<WebviewWindow<R>> {
    if let Some(window) = app.get_webview_window(label) {
        return Ok(window);
    }
    let builder = match label {
        LIBRARY => {
            tauri::WebviewWindowBuilder::new(app, LIBRARY, tauri::WebviewUrl::App(url.into()))
                .title("Dev Console")
                .inner_size(1040.0, 700.0)
                .min_inner_size(760.0, 520.0)
                .resizable(true)
                .visible(false)
        }
        _ => tauri::WebviewWindowBuilder::new(app, FINDER, tauri::WebviewUrl::App(url.into()))
            .title("dev-console-finder")
            .inner_size(680.0, 420.0)
            .resizable(false)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .center(),
    };
    builder
        .build()
        .map_err(|error| AppError::Other(error.to_string()))
}

fn present<R: Runtime>(window: &WebviewWindow<R>) -> AppResult<()> {
    let _ = window.unminimize();
    window.show().map_err(|e| AppError::Other(e.to_string()))?;
    window
        .set_focus()
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

pub fn show_library<R: Runtime>(app: &AppHandle<R>) -> AppResult<()> {
    let window = ensure(app, LIBRARY, "index.html")?;
    present(&window)
}

pub fn show_library_with<R: Runtime>(
    app: &AppHandle<R>,
    kind: crate::models::Kind,
    entry_id: Option<String>,
) -> AppResult<()> {
    show_library(app)?;
    if let Some(window) = app.get_webview_window(LIBRARY) {
        let intent = crate::models::WindowIntent {
            action: if entry_id.is_some() {
                "open-entry"
            } else {
                "new-entry"
            },
            kind,
            entry_id,
        };
        window
            .emit("window-intent", intent)
            .map_err(|e| AppError::Other(e.to_string()))?;
    }
    Ok(())
}

pub fn show_finder<R: Runtime>(app: &AppHandle<R>) -> AppResult<()> {
    let window = ensure(app, FINDER, "index.html#view=finder")?;
    present(&window)?;
    // The frontend clears its query and refocuses the input on every appearance.
    window
        .emit("finder-shown", ())
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

pub fn hide<R: Runtime>(window: &WebviewWindow<R>) -> AppResult<()> {
    window.hide().map_err(|e| AppError::Other(e.to_string()))
}

/// Super+F semantics: a visible finder hides, anything else shows it.
pub fn toggle_finder<R: Runtime>(app: &AppHandle<R>) -> AppResult<()> {
    if let Some(window) = app.get_webview_window(FINDER) {
        if window.is_visible().unwrap_or(false) {
            return hide(&window);
        }
    }
    show_finder(app)
}

pub fn hide_all<R: Runtime>(app: &AppHandle<R>) {
    for label in [LIBRARY, FINDER] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.hide();
        }
    }
}

/// Quit is the only normal action that ends the resident app. The library gets a chance to
/// resolve unsaved edits first; Quit still completes after a grace period so a dead webview
/// cannot make the app unquittable.
pub fn request_quit<R: Runtime>(app: &AppHandle<R>) -> AppResult<()> {
    app.state::<Arc<AppState>>()
        .quitting
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let Some(window) = app.get_webview_window(LIBRARY) else {
        return finish_quit(app);
    };
    let _ = window.emit("close-requested", ());
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(5000));
        let state = handle.state::<Arc<AppState>>();
        if state.quitting.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = finish_quit(&handle);
        }
    });
    Ok(())
}

pub fn cancel_quit<R: Runtime>(app: &AppHandle<R>) {
    app.state::<Arc<AppState>>()
        .quitting
        .store(false, std::sync::atomic::Ordering::SeqCst);
}

/// Flushes completed writes, clears decrypted vault state and app-owned clipboard content,
/// then exits. Never opens or writes the vault.
pub fn finish_quit<R: Runtime>(app: &AppHandle<R>) -> AppResult<()> {
    {
        let state = app.state::<Arc<AppState>>();
        state.vault.clear_decrypted();
        state.clipboard.clear_owned();
    }
    hide_all(app);
    app.exit(0);
    Ok(())
}

pub fn is_quitting<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.state::<Arc<AppState>>()
        .quitting
        .load(std::sync::atomic::Ordering::SeqCst)
}
