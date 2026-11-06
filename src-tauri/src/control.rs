//! Control channel between a second `dev-console` process and the resident one.
//!
//! The single-instance callback forwards launch intent but is fire-and-forget, so it cannot
//! answer "did the lock actually complete?". The resident app therefore listens on a private
//! Unix socket in the user's runtime directory and answers one JSON request per connection
//! with one JSON reply.
//!
//! Why a private socket instead of a session-bus service: it gives the synchronous
//! request/acknowledge pair the CLI needs with no dependency on a bus daemon, no well-known
//! name to collide with, a hard timeout, and access control that is simply the filesystem —
//! the socket lives in a 0700 directory with 0600 permissions, so only this user can connect.
//! Nothing but commands, non-sensitive vault state and the finder's detected project folder
//! crosses it; secret values, secret metadata and the vault key never do.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::cli::VaultCommand;
use crate::error::{AppError, AppResult};
use crate::models::VaultState;
use crate::state::AppState;
use crate::{tray, windows};

pub const EXIT_OK: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_LOCKED: i32 = 10;
pub const EXIT_UNAVAILABLE: i32 = 20;
pub const EXIT_NOT_RUNNING: i32 = 30;

const MAX_REQUEST_BYTES: usize = 4096;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub ok: bool,
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<VaultState>,
}

fn runtime_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("DEV_CONSOLE_RUNTIME_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir).join("dev-console");
        }
    }
    // Without XDG_RUNTIME_DIR there is no user-session directory to hide the socket in, so
    // fall back to a per-uid directory in /tmp that only this user may enter.
    let uid = unsafe { libc::getuid() };
    std::env::temp_dir().join(format!("dev-console-{uid}"))
}

pub fn socket_path() -> PathBuf {
    runtime_dir().join("control.sock")
}

fn code_for(state: &VaultState) -> i32 {
    if state.unlocked {
        EXIT_OK
    } else if !state.vault_available {
        EXIT_UNAVAILABLE
    } else {
        EXIT_LOCKED
    }
}

/// Starts the accept loop. A failure to listen must never stop the app from running: the
/// GUI keeps working and the CLI falls back to its offline behaviour.
pub fn serve(app: &AppHandle, state: Arc<AppState>) {
    let path = socket_path();
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            eprintln!("dev-console: control channel disabled ({error})");
            return;
        }
        crate::paths::set_dir_private(parent).ok();
    }
    // A socket left behind by a killed instance is safe to replace: nothing is listening on it.
    if path.exists() && UnixStream::connect(&path).is_err() {
        std::fs::remove_file(&path).ok();
    }
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("dev-console: control channel already running or unavailable ({error})");
            return;
        }
    };
    crate::paths::set_file_private(&path).ok();
    let handle = app.clone();
    std::thread::Builder::new()
        .name("control-channel".into())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let handle = handle.clone();
                        let state = Arc::clone(&state);
                        std::thread::spawn(move || {
                            if let Err(error) = handle_connection(stream, &handle, &state) {
                                eprintln!("dev-console: control request failed: {error}");
                            }
                        });
                    }
                    Err(error) => eprintln!("dev-console: control accept failed: {error}"),
                }
            }
        })
        .expect("control channel thread");
}

pub fn remove_socket() {
    std::fs::remove_file(socket_path()).ok();
}

fn handle_connection(stream: UnixStream, app: &AppHandle, state: &Arc<AppState>) -> AppResult<()> {
    stream.set_read_timeout(Some(REQUEST_TIMEOUT)).ok();
    stream.set_write_timeout(Some(REQUEST_TIMEOUT)).ok();
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|e| AppError::Other(e.to_string()))?,
    );
    let mut line = String::new();
    let read = reader
        .read_line(&mut line)
        .map_err(|e| AppError::Other(e.to_string()))?;
    if read == 0 || read > MAX_REQUEST_BYTES {
        return Err(AppError::Validation(
            "control request was empty or too large".into(),
        ));
    }
    let request: Request = serde_json::from_str(line.trim())
        .map_err(|_| AppError::Validation("control request was not valid JSON".into()))?;
    let reply = dispatch(&request.command, request.path.as_deref(), app, state);
    let mut stream = stream;
    stream
        .write_all(serde_json::to_vec(&reply)?.as_slice())
        .map_err(|e| AppError::Other(e.to_string()))?;
    stream.write_all(b"\n").ok();
    stream.flush().ok();
    Ok(())
}

/// The authoritative implementation, shared by the socket server and the in-process paths.
pub fn dispatch(
    command: &str,
    path: Option<&str>,
    app: &AppHandle,
    state: &Arc<AppState>,
) -> Reply {
    match command {
        "import.bitwarden" => {
            let result = match path {
                Some(path) => crate::import::bitwarden(state, path),
                None => Err(AppError::Validation("a file path is required".into())),
            };
            match result {
                Ok(outcome) => {
                    if outcome.imported + outcome.renamed > 0 {
                        let _ = app.emit("vault-state-changed", state.vault.state());
                    }
                    let mut message = format!(
                        "imported {} secrets, renamed {}, {} already existed",
                        outcome.imported, outcome.renamed, outcome.duplicates
                    );
                    for skipped in &outcome.skipped {
                        message.push_str(&format!("\nskipped: {skipped}"));
                    }
                    Reply {
                        ok: true,
                        code: EXIT_OK,
                        message,
                        vault: None,
                    }
                }
                Err(error) => Reply {
                    ok: false,
                    code: EXIT_ERROR,
                    message: error.to_string(),
                    vault: Some(state.vault.state()),
                },
            }
        }
        "vault.lock" => {
            // Order matters: secrets stop being readable first, then any app-owned copy in the
            // clipboard is cleared while it is still the app's value.
            match state.vault.lock() {
                Ok(()) => {
                    state.clipboard.clear_owned();
                    let vault = state.vault.state();
                    let _ = app.emit("vault-state-changed", vault.clone());
                    let _ = tray::refresh_menu(app);
                    Reply {
                        ok: true,
                        code: code_for(&vault),
                        message: "vault manually locked; automatic unlock is disabled".into(),
                        vault: Some(vault),
                    }
                }
                Err(error) => Reply {
                    ok: false,
                    code: EXIT_ERROR,
                    message: error.to_string(),
                    vault: Some(state.vault.state()),
                },
            }
        }
        "vault.unlock" => match state.vault.unlock() {
            Ok(()) => {
                let vault = state.vault.state();
                let _ = app.emit("vault-state-changed", vault.clone());
                let _ = tray::refresh_menu(app);
                Reply {
                    ok: vault.unlocked,
                    code: code_for(&vault),
                    message: if vault.unlocked {
                        "vault unlocked".into()
                    } else {
                        "vault is still locked".into()
                    },
                    vault: Some(vault),
                }
            }
            Err(error) => Reply {
                ok: false,
                code: EXIT_ERROR,
                message: error.to_string(),
                vault: Some(state.vault.state()),
            },
        },
        "vault.status" => {
            let vault = state.vault.state();
            Reply {
                ok: true,
                code: code_for(&vault),
                message: describe(&vault),
                vault: Some(vault),
            }
        }
        "window.finder" => Reply {
            ok: windows::toggle_finder(app).is_ok(),
            code: EXIT_OK,
            message: "finder toggled".into(),
            vault: None,
        },
        // For the live-session harness (`scripts/test-project-context.sh`): what the palette is
        // ranking by right now. A folder name and path the user is sitting in, never entry data.
        "finder.context" => Reply {
            ok: true,
            code: EXIT_OK,
            message: serde_json::json!({
                "enabled": state.settings.get().project_context_enabled,
                "visible": windows::finder_visible(app),
                "context": state.active_finder_context(),
            })
            .to_string(),
            vault: None,
        },
        "window.library" => Reply {
            ok: windows::show_library(app).is_ok(),
            code: EXIT_OK,
            message: "library shown".into(),
            vault: None,
        },
        "app.quit" => {
            let _ = windows::request_quit(app);
            Reply {
                ok: true,
                code: EXIT_OK,
                message: "quit requested".into(),
                vault: None,
            }
        }
        other => Reply {
            ok: false,
            code: EXIT_ERROR,
            message: format!("unknown control command {other}"),
            vault: None,
        },
    }
}

pub fn describe(vault: &VaultState) -> String {
    if vault.unlocked {
        "unlocked".into()
    } else if vault.manual_lock {
        "locked (manual)".into()
    } else if !vault.vault_available {
        format!("unavailable: {}", vault.reason.clone().unwrap_or_default())
    } else {
        "locked".into()
    }
}

fn send(request: &Request) -> Option<Reply> {
    let path = socket_path();
    let mut stream = UnixStream::connect(&path).ok()?;
    stream.set_read_timeout(Some(REQUEST_TIMEOUT)).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
    stream
        .write_all(serde_json::to_vec(request).ok()?.as_slice())
        .ok()?;
    stream.write_all(b"\n").ok()?;
    stream.shutdown(std::net::Shutdown::Write).ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    serde_json::from_str(line.trim()).ok()
}

/// True when a resident instance answers on the control socket.
pub fn instance_running() -> bool {
    send(&Request {
        command: "vault.status".into(),
        path: None,
    })
    .is_some()
}

/// CLI entry point. Returns the process exit code; output never contains secret data.
pub fn run(command: VaultCommand) -> AppResult<i32> {
    let name = match command {
        VaultCommand::Lock => "vault.lock",
        VaultCommand::Unlock => "vault.unlock",
        VaultCommand::Status => "vault.status",
    };
    if let Some(reply) = send(&Request {
        command: name.into(),
        path: None,
    }) {
        if reply.ok {
            println!("{}", reply.message);
        } else {
            eprintln!("dev-console: {}", reply.message);
        }
        return Ok(if reply.ok {
            reply.code
        } else {
            reply.code.max(EXIT_ERROR)
        });
    }

    // No resident instance. Lock and status still work truthfully against the persistent
    // manual-lock flag; unlock cannot be completed because the authenticated flow needs the
    // running backend.
    match command {
        VaultCommand::Lock => {
            let flag = crate::paths::lock_flag_path()?;
            if let Some(parent) = flag.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let tmp = flag.with_extension("tmp");
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&tmp)
                .map_err(|e| AppError::Other(e.to_string()))?;
            file.write_all(b"locked\n")
                .map_err(|e| AppError::Other(e.to_string()))?;
            file.flush().map_err(|e| AppError::Other(e.to_string()))?;
            crate::paths::set_file_private(&tmp)?;
            std::fs::rename(&tmp, &flag).map_err(|e| AppError::Other(e.to_string()))?;
            println!("vault manually locked (no running instance; the flag persists until an authenticated unlock)");
            Ok(EXIT_NOT_RUNNING)
        }
        VaultCommand::Status => {
            let service = crate::vault::VaultService::new(
                crate::paths::vault_path()?,
                crate::paths::lock_flag_path()?,
                crate::vault::Credential::FromKeyring,
            );
            service.bootstrap();
            let vault = service.state();
            println!("{}", describe(&vault));
            Ok(if vault.manual_lock {
                EXIT_LOCKED
            } else {
                code_for(&vault)
            })
        }
        VaultCommand::Unlock => {
            let flag = crate::paths::lock_flag_path()?;
            if !flag.exists() {
                println!("vault is not manually locked (no running instance)");
                return Ok(EXIT_NOT_RUNNING);
            }
            eprintln!(
                "dev-console: no running instance to authenticate the unlock; start Dev Console \
                 and unlock from the tray or the library"
            );
            Ok(EXIT_NOT_RUNNING)
        }
    }
}

/// CLI entry point for `import bitwarden`. Needs the resident app, which owns the unlocked vault.
pub fn run_import(file: &str) -> AppResult<i32> {
    let path = std::fs::canonicalize(file)
        .map_err(|e| AppError::Validation(format!("cannot open {file}: {e}")))?;
    let Some(reply) = send(&Request {
        command: "import.bitwarden".into(),
        path: Some(path.to_string_lossy().into_owned()),
    }) else {
        eprintln!("dev-console: no running instance; start Dev Console and unlock the vault first");
        return Ok(EXIT_NOT_RUNNING);
    };
    if reply.ok {
        println!("{}", reply.message);
        Ok(EXIT_OK)
    } else {
        eprintln!("dev-console: {}", reply.message);
        Ok(reply.code.max(EXIT_ERROR))
    }
}
