//! The native Rune shell: two windows, a tray, one instance, and a control socket that wakes it.
//!
//! This crate is the desktop half of the plan in `docs/plans/rune-migration.md`. It owns windows,
//! tray, CLI, single-instance and wakeups. It deliberately owns **no** domain logic: models, the
//! SQLite repository, the age vault, the keyring adapter, polkit, search, imports, project scanners,
//! history, environment snapshots, recovery and backups belong to `dev-console-core` (plan §1), and
//! none of them has moved yet - milestone 2 moves them out of `src-tauri/src` with their tests. Until
//! then this shell shows placeholder views with no real data, and nothing here reads or writes a
//! profile, a database or a keyring, so it cannot disagree with the running Tauri app about user
//! data.
//!
//! ## What this crate can prove on a headless host, and what it cannot
//!
//! Unit tests here cover the *specifications* the shell asks Rune for and the parts that need no
//! display: window titles and sizes, the opaque/borderless option set, the single-instance lock, the
//! control-socket round trip, the tray menu's shape and the icon buffer's layout. Rune's headless
//! tooling can also drive the views over their element trees (see `dev-console-ui-tests`).
//!
//! None of that establishes what plan §5 reserves for a real session: activation, the title/class
//! `hyprctl -j clients` reports for `dev-console-finder`, the mapped 680×420 size, hover, dismissal,
//! whether neighbouring tiled windows keep their layout, tray rendering, GPU/X11 error logs, and
//! "verified on Hyprland/NVIDIA". Those stay UNVERIFIED here - by the plan's own rule ("Treat exit 77
//! as UNVERIFIED"), which is what [`app::EXIT_NO_DISPLAY`] encodes for this binary - and are carried
//! to the owner's desktop check.

pub mod app;
pub mod identity;
pub mod instance;
pub mod tray;
pub mod ui;

/// Application identity: the Wayland `app_id`, the StatusNotifierItem id, and the name the owner's
/// Hyprland rules and `.desktop` file use. The window *title* is separate, and for the Finder it is
/// `dev-console-finder` (see [`identity`]).
pub const APP_ID: &str = "dev-console";

/// Runs the shell. Returns the process exit code; `main` only forwards it.
///
/// Kept out of `main` so the integration tests in `dev-console-ui-tests` can call it without
/// replacing the real binary's process.
pub fn run(args: &[String]) -> i32 {
    app::run(args)
}

/// The command-line help text, exposed so a test can assert the accepted flags stay two.
pub const USAGE: &str = "usage: dev-console [--finder | --library]";

#[cfg(test)]
mod tests {
    #[test]
    fn the_app_id_is_the_tray_id() {
        assert_eq!(crate::APP_ID, crate::tray::TRAY_ID);
    }
}
