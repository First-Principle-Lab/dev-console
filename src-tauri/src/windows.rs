use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

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
            // The palette paints its own rounded shell; whatever a compositor adds around it
            // stays see-through instead of showing as a dark slab.
            .transparent(true)
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
    // Any context from the previous showing describes a window that is no longer focused;
    // drop it synchronously so a search racing the new detection cannot rank against it.
    app.state::<Arc<AppState>>().set_finder_context(None);
    let window = ensure(app, FINDER, "index.html#view=finder")?;
    present(&window)?;
    // Tiling compositors ignore the creation size: Hyprland mapped the finder at 950x1008.
    // Asking again once the window is mapped is honoured for floating windows.
    let _ = window.set_size(tauri::LogicalSize::new(FINDER_SIZE.0, FINDER_SIZE.1));
    // The frontend clears its query and refocuses the input on every appearance.
    window
        .emit("finder-shown", ())
        .map_err(|e| AppError::Other(e.to_string()))?;
    spawn_context_detection(app.clone());
    Ok(())
}

/// Detects the focused project off the main path: the Finder is already visible while the
/// compositor round-trip (bounded at 200 ms) and the `/proc` walk run, and the frontend
/// learns the outcome through `finder-context` plus the `context` field of the next search
/// response. The stale window between show and detection resolving ranks without context,
/// which is exactly the pre-feature behaviour.
pub fn spawn_context_detection<R: Runtime>(app: AppHandle<R>) {
    let for_thread = app.clone();
    let spawned = std::thread::Builder::new()
        .name("finder-context".into())
        .spawn(move || {
            let found = detect_finder_context(&for_thread);
            for_thread
                .state::<Arc<AppState>>()
                .set_finder_context(found.clone());
            let _ = for_thread.emit("finder-context", &found);
        });
    if spawned.is_err() {
        // Detection is an enhancement; a thread-starved machine simply shows no context.
        let _ = app.emit(
            "finder-context",
            &Option::<crate::context::FinderContext>::None,
        );
    }
}

/// The detection pass proper, split out so the disabled case provably never touches /proc
/// or the compositor socket. Reads settings from the cache: no I/O beyond detection itself.
fn detect_finder_context<R: Runtime>(app: &AppHandle<R>) -> Option<crate::context::FinderContext> {
    if !app
        .state::<Arc<AppState>>()
        .settings
        .get()
        .project_context_enabled
    {
        return None;
    }
    crate::context::detect()
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
