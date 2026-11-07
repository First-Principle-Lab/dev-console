use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

use crate::context::FinderContext;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const LIBRARY: &str = "library";
pub const FINDER: &str = "finder";

/// The palette's size. The finder window must be exactly this big: anything larger shows as an
/// empty band around the palette.
const FINDER_SIZE: (f64, f64) = (680.0, 420.0);

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
            .inner_size(FINDER_SIZE.0, FINDER_SIZE.1)
            .resizable(false)
            .decorations(false)
            // Keep the native surface opaque. On WebKitGTK 2.54 + Hyprland/NVIDIA,
            // transparent windows can lose their entire paint on pointer hover while staying
            // mapped and focused. Hyprland supplies the rounded window corners; see AGENTS.md.
            .transparent(false)
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
    // Before presenting: asking which window is focused is only meaningful while the terminal
    // still is, and `present` hands focus to the palette. The focused window is read here, in
    // line; only the `/proc` walk is left to a thread.
    detect_context(app);
    present(&window)?;
    // Tiling compositors ignore the creation size: Hyprland mapped the finder at 950x1008.
    // Asking again once the window is mapped is honoured for floating windows.
    let _ = window.set_size(tauri::LogicalSize::new(FINDER_SIZE.0, FINDER_SIZE.1));
    // The frontend clears its query and refocuses the input on every appearance.
    window
        .emit("finder-shown", ())
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

/// Works out which project the terminal belongs to.
///
/// Two halves. The focused window is captured here, synchronously, over the compositor socket
/// (well under a millisecond on Hyprland, bounded at 50 ms): it has to be, because asking after the
/// palette is shown would only ever name the palette. Everything else - `hyprctl` for a session
/// without the socket, the `/proc` walk, the repository walk-up - runs on a thread of its own, and
/// the palette is presented without waiting for it. The result arrives as a `finder-context`
/// event; the chip appears when it does.
fn detect_context<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let enabled = state.settings.get().project_context_enabled;
    // Whatever the last open decided is wrong for this one, so it goes before anything else: the
    // palette must never be shown ranked against a project it did not come from. The token is what
    // makes a reply that arrives after this open is history get dropped instead of applied.
    let token = state.begin_finder_detection();
    let _ = app.emit("finder-context", Option::<FinderContext>::None);
    if !enabled {
        // Off means off: no compositor query and no `/proc` walk at all.
        return;
    }
    let focused = crate::context::focused_now();
    let handle = app.clone();
    let spawned = std::thread::Builder::new()
        .name("finder-context".into())
        .spawn(move || {
            let context = focused
                .or_else(crate::context::focused_via_hyprctl)
                .and_then(|window| crate::context::resolve(&window));
            let applied = handle
                .try_state::<Arc<AppState>>()
                .is_some_and(|state| state.finish_finder_detection(token, context.clone()));
            if applied {
                let _ = handle.emit("finder-context", context);
            }
        });
    if spawned.is_err() {
        // A machine that cannot spare one more thread still gets a finder, just an unordered one.
        eprintln!("dev-console: project detection could not start");
    }
}

pub fn hide<R: Runtime>(window: &WebviewWindow<R>) -> AppResult<()> {
    window.hide().map_err(|e| AppError::Other(e.to_string()))
}

/// Whether the palette is on screen; what the `finder.context` control query reports.
pub fn finder_visible<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.get_webview_window(FINDER)
        .is_some_and(|window| window.is_visible().unwrap_or(false))
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
