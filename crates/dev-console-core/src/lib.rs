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
//!
//! The UI logic that only ever existed in TypeScript is here too, because the Rune views in
//! `crates/dev-console-desktop` will need it and it has to be testable without a renderer (plan §3:
//! "port TypeScript-only drafts, validation, argument parsing, formatting, and Finder selection
//! logic into Rust"): [`validation`] (the editor's Save check), [`argv`] (the terminal-command
//! word splitting), [`time_format`] (the row timestamps) and [`finder_state`] (palette selection and
//! completion). Each ports one file in `src/lib`, and its test names are the ported vitest `it`
//! titles from `tests/unit/*.test.ts`, so a removed case is visible in the test list. The
//! TypeScript stays until the cutover (plan §4.6): both apps are alive, and this is how they are
//! kept from growing two answers to one question.

pub mod argv;
pub mod auth;
pub mod backup;
pub mod clipboard;
pub mod context;
pub mod environment;
pub mod error;
pub mod execution;
pub mod finder_state;
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
pub mod time_format;
pub mod validation;
pub mod vault;

pub use error::{AppError, AppResult};
pub use state::AppState;
