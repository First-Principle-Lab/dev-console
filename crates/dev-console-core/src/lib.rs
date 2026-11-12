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
//!
//! Plan §3's third requirement - "retain request IDs, revision checks, and vault generations so
//! obsolete results cannot restore stale or sensitive state" - is [`freshness`]: the ledger of which
//! request is still current, and the three guards a background answer has to pass before a window may
//! apply it. Its rules are the ones `src/windows/Finder.svelte` has applied since before this crate
//! existed, ported rather than reinvented, so the two apps cannot disagree about what "stale" means.
//!
//! The last piece of plan §3 is the [`services`] module: [`LibraryActions`] and [`FinderActions`],
//! the two surfaces a window is allowed to talk to. They are separate types rather than a mode flag,
//! so "the palette can decrypt nothing" is a property of the program; each surface declares its
//! operations as a const, and a compile-time check refuses a privileged name on the palette's list.

pub mod argv;
pub mod auth;
pub mod autostart;
pub mod backup;
pub mod clipboard;
pub mod context;
pub mod environment;
pub mod error;
pub mod execution;
pub mod finder_state;
pub mod freshness;
pub mod history;
pub mod import;
pub mod keybindings;
pub mod keyring;
pub mod models;
pub mod paths;
pub mod project_history;
pub mod projects;
pub mod recovery;
pub mod repository;
pub mod search;
pub mod services;
pub mod settings;
pub mod state;
#[doc(hidden)]
pub mod testing;
pub mod theme;
pub mod time_format;
pub mod validation;
pub mod vault;

pub use error::{AppError, AppResult};
pub use services::{AppEvent, Effects, FinderActions, LibraryActions, Surface, WindowCommand};
pub use state::AppState;
pub use theme::{Appearance, Palette, Theme};

pub mod cliproxy;
