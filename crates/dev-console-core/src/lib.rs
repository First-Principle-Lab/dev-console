//! Dev Console's domain layer, free of any desktop runtime.
//!
//! Plan section 3 ("Reuse the existing domain behavior") moves the proven code out of the Tauri
//! backend here, unchanged in behaviour: models, the `rusqlite` repository, the age vault, the
//! keyring adapter (`dev-console` / `vault-key`), polkit authentication, search, imports, project
//! scanners, shell history, environment snapshots, recovery, backups, the clipboard owner, and the
//! [`state::AppState`] container that owns them all. Each module kept its own tests during the
//! move, so the same test set is the evidence that nothing was lost (plan section 3: "Refactor
//! dependencies on desktop runtime types without rewriting established storage or security
//! behavior").
//!
//! Unchanged on purpose, and asserted by the tests that came with this code:
//!
//! - XDG data/config locations and filenames ([`paths`]) and the database schema ([`repository`]).
//! - The encrypted archive format ([`backup`]) and the keyring service `dev-console` with account
//!   `vault-key` ([`keyring`], [`vault`]).
//! - Persistent manual lock, authenticated unlock, and the vault generation checks ([`vault`],
//!   [`state`]).
//! - Command approval rules, the 120-second execution timeout, and the output limits
//!   ([`execution`]).
//! - The shell hook protocol installed into the user's zsh ([`project_history`]).
//!
//! This crate depends on no desktop runtime: no Tauri, no windowing, no renderer, no tray. The
//! crates that do touch a window - the Tauri app still in `src-tauri` and, later,
//! `crates/dev-console-desktop` - reach the domain only through [`state::AppState`], which owns the
//! repository, the vault, settings, the clipboard owner, backups, and the recovery journal. The
//! control socket, the tray, the CLI, and window handling stay with the desktop runtime because
//! they wake or move a window; their request grammar and exit codes are unchanged.

pub mod auth;
pub mod backup;
pub mod clipboard;
pub mod context;
pub mod environment;
pub mod error;
pub mod execution;
pub mod history;
pub mod import;
pub mod keyring;
pub mod models;
pub mod paths;
pub mod project_history;
pub mod projects;
pub mod recovery;
pub mod repository;
pub mod search;
pub mod settings;
pub mod state;
pub mod vault;

pub use error::{AppError, AppResult};
pub use state::AppState;
