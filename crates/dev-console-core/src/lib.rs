//! Dev Console's domain layer, free of any desktop runtime.
//!
//! Status: **scaffold**. This crate holds nothing yet, on purpose.
//!
//! Plan section 3 ("Reuse the existing domain behavior") moves the proven code from the Tauri
//! backend into here unchanged in behaviour: models, the `rusqlite` repository, the age vault, the
//! keyring adapter (`dev-console` / `vault-key`), polkit authentication, search, imports, project
//! scanners, history, environment snapshots, recovery and backups. Its home today is
//! `src-tauri/src/{models,repository,vault,keyring,auth,search,import,projects,project_history,
//! environment,recovery,backup,paths,control}.rs`.
//!
//! Deliberately *not* done here yet: re-typing any of that into a "core" shape. Duplicating a
//! storage or security rule in two places during a migration is how the two copies drift, and the
//! existing modules keep their own tests while they move (plan section 3, "Refactor dependencies on
//! desktop runtime types without rewriting established storage or security behavior"). Milestone 2
//! of plan section 4 moves them together with their test set, so the moved tests stay the evidence
//! that nothing was lost; XDG paths, filenames, the database schema, the encrypted archive format,
//! the keyring service/account, manual-lock semantics, approval rules, the 120s execution timeout
//! and the output limits all keep their current behaviour.
//!
//! Until that move happens, the running application reads and writes its data only through the
//! Tauri backend, so there is exactly one implementation of every rule at all times.
