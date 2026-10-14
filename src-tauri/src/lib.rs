pub mod auth;
pub mod autostart;
pub mod cli;
pub mod clipboard;
pub mod control;
pub mod error;
pub mod execution;
pub mod ipc;
pub mod keyring;
pub mod models;
pub mod paths;
pub mod repository;
pub mod search;
pub mod settings;
pub mod state;
pub mod tray;
pub mod vault;
pub mod windows;

use std::sync::Arc;

use tauri::{Emitter, Manager, RunEvent, WindowEvent};

use crate::cli::Intent;
use crate::state::AppState;

pub fn run() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(argv.iter()) {
        Intent::Help => {
            print!("{}", cli::USAGE);
            std::process::exit(0);
        }
        Intent::Version => {
            println!("dev-console {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }
        Intent::Vault(command) => match control::run(command) {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("dev-console: {error}");
                std::process::exit(control::EXIT_ERROR);
            }
        },
        Intent::Library | Intent::Background | Intent::Finder => build(argv),
    }
}

fn dispatch_second_launch(app: &tauri::AppHandle, argv: &[String]) {
    match cli::parse(argv.iter().skip(1)) {
        Intent::Finder => {
            let _ = windows::toggle_finder(app);
        }
        Intent::Library => {
            let _ = windows::show_library(app);
        }
        // `--background` on an already running app must not raise anything.
        Intent::Background => {}
        _ => {}
    }
}

fn build(argv: Vec<String>) {
    let intent = cli::parse(argv.iter());

    tauri::Builder::default()
        // First plugin: routes every later launch to the running instance.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            dispatch_second_launch(app, &args);
        }))
        .setup(move |app| {
            let state =
                AppState::bootstrap().map_err(|e| format!("backend startup failed: {e}"))?;
            app.manage(state.clone());
            // A desktop without a usable status notifier (or a hidden tray) must still be able
            // to run: the desktop entry and the CLI keep working without the tray icon.
            if let Err(error) = tray::build(app.handle()) {
                eprintln!("dev-console: tray unavailable ({error}); continuing without the icon");
            }
            clipboard::spawn_expiry_worker(Arc::clone(&state));
            control::serve(app.handle(), Arc::clone(&state));
            match intent {
                Intent::Finder => windows::show_finder(app.handle())?,
                Intent::Background => {}
                _ => windows::show_library(app.handle())?,
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::app_state,
            ipc::search_entries,
            ipc::list_entries,
            ipc::types_for,
            ipc::get_entry,
            ipc::reveal_secret,
            ipc::save_entry,
            ipc::delete_entry,
            ipc::copy_entry,
            ipc::run_command,
            ipc::lock_vault,
            ipc::unlock_vault,
            ipc::open_library,
            ipc::hide_self,
            ipc::allow_close,
            ipc::cancel_quit,
            ipc::quit_app,
            ipc::get_settings,
            ipc::save_settings,
            ipc::open_settings
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Closing either window only hides it; the tray app stays resident.
                api.prevent_close();
                let quitting = window
                    .app_handle()
                    .state::<Arc<AppState>>()
                    .quitting
                    .load(std::sync::atomic::Ordering::SeqCst);
                match window.label() {
                    windows::FINDER => {
                        let _ = window.hide();
                    }
                    windows::LIBRARY if quitting => {
                        let _ = windows::finish_quit(window.app_handle());
                    }
                    _ => {
                        let _ = window.emit("close-requested", ());
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build dev-console")
        .run(|app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app.try_state::<Arc<AppState>>() {
                    state.vault.clear_decrypted();
                    state.clipboard.clear_owned();
                }
                control::remove_socket();
                windows::hide_all(app);
            }
        });
}
