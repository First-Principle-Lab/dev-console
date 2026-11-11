//! Process wiring: the single-instance decision, the core services, both windows and the tray.
//!
//! The resident process owns both windows and starts with neither mapped, as the current app does:
//! the tray, the compositor shortcut and a second launch are what ask for a window. A later launch
//! forwards its intent and exits, so exactly one process ever holds the vault and the windows.

use std::sync::Arc;

use dev_console_core::state::AppState;
use rune::{App, AppHandle, WindowSpec};

use crate::finder::{finder_view, Finder};
use crate::hub::{Exec, Hub, Windows};
use crate::identity::{
    finder_options, library_options, FINDER_LABEL, FINDER_SIZE, FINDER_TITLE, LIBRARY_MIN_SIZE,
    LIBRARY_SIZE, LIBRARY_TITLE,
};
use crate::instance::{self, Intent};
use crate::library::{library_view, Library};
use crate::tray;

/// Success.
pub const EXIT_OK: i32 = 0;
/// A usage error, or the resident side failed to come up.
pub const EXIT_ERROR: i32 = 1;
/// An unknown command-line flag.
pub const EXIT_USAGE: i32 = 2;
/// The GUI could not start because there is no display to draw on. 77 is what
/// `scripts/verify.sh` reads as UNVERIFIED rather than as a pass or a failure.
pub const EXIT_NO_DISPLAY: i32 = 77;

/// What `dev-console [FLAG]` accepts. Anything else is a usage error, because a mistyped flag must
/// not silently open a window the caller did not ask for.
pub fn parse_args(args: &[String]) -> Result<Option<Intent>, String> {
    let mut intent = None;
    for arg in args {
        if arg == "--help" || arg == "-h" {
            return Err(USAGE.into());
        }
        match Intent::from_flag(arg) {
            Some(found) => {
                if intent.replace(found).is_some() {
                    return Err("choose at most one window flag".into());
                }
            }
            None => return Err(format!("unknown argument {arg:?}")),
        }
    }
    Ok(intent)
}

const USAGE: &str = "usage: dev-console [--finder | --library]\n\
     \n\
     With no flag, the resident instance is asked to show the Library, or a new resident\n\
     instance starts with no window mapped and waits for the tray, a shortcut or a second launch.";

/// The Library's spec: title, size, minimum and the window-level key router.
pub fn library_spec() -> rune::AppSpec<Library> {
    rune::AppSpec::<Library> {
        min_size: Some(LIBRARY_MIN_SIZE),
        on_key: Some(Library::route_key),
        poll: Some(Library::poll),
        ..rune::AppSpec::new(
            LIBRARY_TITLE,
            LIBRARY_SIZE,
            || unreachable!("built by App::init"),
            library_view,
        )
    }
}

/// The Finder's spec. The query field routes the palette's keys first; the window routes the same
/// table for keys that arrive while a row, not the field, holds focus (a click on a row moves it).
pub fn finder_spec() -> rune::AppSpec<Finder> {
    rune::AppSpec::<Finder> {
        on_key: Some(Finder::route_key),
        ..rune::AppSpec::new(
            FINDER_TITLE,
            FINDER_SIZE,
            || unreachable!("built by App::init"),
            finder_view,
        )
    }
}

/// A close request on the Library: with an unsaved draft, the prompt goes up and the window comes
/// back to show it, because Rune's close cannot be vetoed; otherwise decrypted state is dropped.
fn library_closed(library: &mut Library, _tx: &mut rune::Tx) {
    if library.draft_dirty() {
        library.pending_close = true;
        if let Some(windows) = library.hub().windows() {
            windows.library.open();
        }
    } else {
        library.clear_sensitive();
    }
}

/// The Finder never closes for good: closing it is dismissing it.
fn finder_closed(finder: &mut Finder, _tx: &mut rune::Tx) {
    finder.query.clear();
}

/// Starts, or talks to, the resident instance. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let intent = match parse_args(args) {
        Ok(intent) => intent,
        Err(message) => {
            eprintln!("{message}");
            return if message.starts_with("usage:") {
                EXIT_OK
            } else {
                EXIT_USAGE
            };
        }
    };

    let socket = instance::socket_path();

    // Second launch: hand the intent to the resident process and leave.
    if instance::send_intent(&socket, intent.unwrap_or(Intent::Library)).is_ok() {
        return EXIT_OK;
    }

    let Some(_lock) = take_instance_lock() else {
        eprintln!(
            "dev-console: another instance holds the lock but is not answering on {}; exiting",
            socket.display()
        );
        return EXIT_ERROR;
    };

    let server = match instance::Server::bind(&socket) {
        Ok(server) => server,
        Err(error) => {
            eprintln!(
                "dev-console: cannot listen on {}: {error}",
                socket.display()
            );
            return EXIT_ERROR;
        }
    };

    // Nothing is shown until the profile is open and any interrupted replacement is settled.
    let state: Arc<AppState> = match AppState::bootstrap() {
        Ok(state) => state,
        Err(error) => {
            eprintln!("dev-console: cannot open the profile: {error}");
            return EXIT_ERROR;
        }
    };
    start_background_work(&state);
    let hub = Hub::new(state.clone(), env!("CARGO_PKG_VERSION"));

    rune::set_app_id(crate::APP_ID);

    let library_app = App::new(library_spec());
    let library: AppHandle<Library> = library_app.handle();
    let library_hub = hub.clone();
    let library_exec = library.clone();
    let mut library_app = library_app
        .init(move || Library::new(library_hub, Exec::Thread(library_exec)))
        .window_options(library_options())
        .on_close(library_closed)
        .on_focus(|library, focused, _| {
            if focused {
                library.check_updates_if_due();
            }
        })
        .windowless()
        .keep_running();

    let finder_app = App::new(finder_spec());
    let finder: AppHandle<Finder> = finder_app.handle();
    let finder_hub = hub.clone();
    let finder_exec = finder.clone();
    let finder_app = finder_app
        .init(move || Finder::new(finder_hub, Exec::Thread(finder_exec)))
        .on_close(finder_closed);
    let (_, _finder_window) = library_app.window(
        WindowSpec::from_app(FINDER_LABEL, finder_app)
            .options(finder_options())
            .on_created(|window| {
                // Tiling compositors can settle a borderless window's size after mapping; asking again
                // once it is mapped is honoured for floating windows.
                window.resize(FINDER_SIZE);
            }),
    );

    // The tray must be built before the loop starts and kept alive for the process's life.
    let tray_handle = tray::attach(&hub);
    let tray_refresh: Option<Arc<dyn Fn() + Send + Sync>> = tray_handle.clone().map(|tray| {
        let state = hub.state.clone();
        Arc::new(move || tray::refresh(&tray, state.vault.state().unlocked))
            as Arc<dyn Fn() + Send + Sync>
    });
    hub.connect(Windows {
        library: library.clone(),
        finder: finder.clone(),
        tray: tray_refresh,
    });

    let owns_control = crate::control::serve(&hub, state.clone());
    if let Some(intent) = intent {
        open(&hub, intent);
    }

    // The wakeup path runs on its own thread; `AppHandle` calls are what wake the UI loop, so no
    // socket read ever happens on the UI thread.
    let wakeup_hub = hub.clone();
    std::thread::spawn(move || {
        if let Err(error) = server.serve(|intent| open(&wakeup_hub, intent)) {
            eprintln!("dev-console: control socket stopped: {error}");
        }
    });

    let outcome = library_app.try_run();
    drop(tray_handle);
    // Another build of the app may have been listening first; its socket is not ours to remove.
    if owns_control {
        crate::control::remove_socket();
    }
    if outcome.is_ok()
        && hub
            .restart_requested
            .load(std::sync::atomic::Ordering::SeqCst)
    {
        let executable = std::env::var_os("APPIMAGE")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_exe().ok());
        // The wakeup thread still answers on the instance socket until this process is gone.
        // Unlink it first, so the replacement starts as the resident instance and does not hand
        // its launch to a process that is about to exit.
        std::fs::remove_file(&socket).ok();
        drop(_lock);
        if let Some(executable) = executable {
            let mut command = std::process::Command::new(executable);
            command.arg("--library");
            if std::env::var_os("APPIMAGE").is_some() {
                for key in ["APPIMAGE", "APPDIR", "ARGV0", "OWD", "LD_LIBRARY_PATH"] {
                    command.env_remove(key);
                }
            }
            if command.spawn().is_ok() {
                return EXIT_OK;
            }
        }
        eprintln!("dev-console: restart failed; the previous AppImage is retained beside the installed image");
        return EXIT_ERROR;
    }
    match outcome {
        Ok(()) => EXIT_OK,
        Err(message) => {
            eprintln!("dev-console: the native shell could not start: {message}");
            EXIT_NO_DISPLAY
        }
    }
}

/// The resident process's duties that need no window, as `src-tauri/src/lib.rs` starts them: a
/// sealed backup at startup, the clipboard expiry worker, environment snapshot sync and the shell
/// hook refresh.
fn start_background_work(state: &Arc<AppState>) {
    // A locked or unavailable vault cannot be backed up, and that is not an error.
    if state.vault.is_unlocked() {
        match state
            .build_archive()
            .and_then(|archive| state.backups.create(&archive, &state.vault))
        {
            Ok(name) => eprintln!("dev-console: sealed backup written ({name})"),
            Err(error) => eprintln!("dev-console: startup backup skipped ({error})"),
        }
    }
    dev_console_core::clipboard::spawn_expiry_worker(Arc::clone(state));
    dev_console_core::environment::spawn_sync_worker(Arc::clone(state));
    if dev_console_core::project_history::refresh_installed_zsh_hook().is_err() {
        eprintln!("dev-console: could not refresh the installed shell hook");
    }
    // The Tauri build wrote its login entry with the AppImage's temporary mount path, which is gone
    // after a reboot. Rewriting it at every start repairs an upgraded install without a visit to
    // Settings, and keeps the entry on the installed image if that ever moves.
    if state.settings.get().autostart {
        if let Err(error) = dev_console_core::autostart::apply(true) {
            eprintln!("dev-console: could not refresh the login entry ({error})");
        }
    }
}

fn take_instance_lock() -> Option<instance::InstanceLock> {
    let path = instance::lock_path();
    match instance::InstanceLock::acquire(&path) {
        Ok(lock) => lock,
        Err(error) => {
            eprintln!(
                "dev-console: cannot take the instance lock at {}: {error}",
                path.display()
            );
            None
        }
    }
}

/// Shows the window an intent names. `--finder` is Super+F, which toggles.
fn open(hub: &Hub, intent: Intent) {
    match intent {
        Intent::Library => {
            if let Some(windows) = hub.windows() {
                windows.library.open();
            }
        }
        Intent::Finder => hub.toggle_finder(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_parse_into_at_most_one_intent() {
        assert_eq!(parse_args(&[]).unwrap(), None);
        assert_eq!(
            parse_args(&["--finder".into()]).unwrap(),
            Some(Intent::Finder)
        );
        assert_eq!(
            parse_args(&["--library".into()]).unwrap(),
            Some(Intent::Library)
        );
        assert!(parse_args(&["--finder".into(), "--library".into()])
            .unwrap_err()
            .contains("at most one"));
        assert!(parse_args(&["--reveal".into()])
            .unwrap_err()
            .starts_with("unknown argument"));
        assert!(parse_args(&["--help".into()])
            .unwrap_err()
            .starts_with("usage:"));
    }

    #[test]
    fn the_two_window_specs_match_the_tauri_shell() {
        let library = library_spec();
        assert_eq!(library.title, LIBRARY_TITLE);
        assert_eq!(library.size, LIBRARY_SIZE);
        assert_eq!(library.min_size, Some(LIBRARY_MIN_SIZE));
        assert!(
            library.on_key.is_some(),
            "the Library window carries the app-level key router"
        );
        let finder = finder_spec();
        assert_eq!(finder.title, FINDER_TITLE);
        assert_eq!(finder.size, FINDER_SIZE);
        assert!(
            finder.on_key.is_some(),
            "the Finder routes its keys from the window too, for a row that took focus"
        );
        let options = finder_options();
        assert!(
            !options.transparent && options.borderless && !options.resizable && !options.visible
        );
        assert!(
            !library_options().visible,
            "the resident app starts with no window mapped"
        );
    }

    #[test]
    fn exit_codes_do_not_collide_with_the_repos_unverified_convention() {
        assert_eq!(
            (EXIT_OK, EXIT_ERROR, EXIT_USAGE, EXIT_NO_DISPLAY),
            (0, 1, 2, 77)
        );
    }

    #[test]
    fn an_intent_with_no_running_instance_is_reported_rather_than_retried_forever() {
        let dir = tempfile::tempdir().unwrap();
        let dead = dir.path().join("nothing-here.sock");
        drop(std::os::unix::net::UnixListener::bind(&dead).unwrap());
        let error = instance::send_intent(&dead, Intent::Finder).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionRefused);
        assert!(!instance::instance_running(&dead));
    }
}
