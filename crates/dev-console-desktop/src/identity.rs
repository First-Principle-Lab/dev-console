//! Window identity for the two Rune windows.
//!
//! These values are the contract the desktop shell is expected to satisfy, taken unchanged from
//! the Tauri windows in `src-tauri/src/windows.rs` and from plan §3 ("Finder presentation"): the
//! Finder keeps its exact `dev-console-finder` title because the owner's Hyprland rule matches
//! that title and sets float, size and centering, and the 680×420 size must be exact because
//! anything larger shows as an empty band around the palette.
//!
//! ## What these constants can and cannot prove
//!
//! A test in this module proves the *specification the shell asks Rune for*. It cannot prove what
//! a compositor finally does with it. Activation, title/class as reported by `hyprctl -j clients`,
//! actual mapped size, hover behaviour, dismissal and the layout of neighbouring tiled windows all
//! require a real Wayland session, so they stay UNVERIFIED on a headless host (plan §5, "Treat exit
//! 77 as UNVERIFIED").
//!
//! Two properties the Tauri Finder window had have no equivalent in `rune::WindowOptions` at the
//! pinned revision, and are therefore carried to the desktop check rather than claimed here:
//!
//! - `skip_taskbar`: `WindowOptions` has no such field.
//! - `center`: placement is a request made after mapping (`AppHandle::place`/`set_position`), not a
//!   window option; Hyprland's rule for `dev-console-finder` centers it anyway.

use rune::WindowOptions;

/// Window key for the Library. `dev-console` internal label, never shown.
pub const LIBRARY_LABEL: &str = "library";

/// Window key for the Finder.
pub const FINDER_LABEL: &str = "finder";

/// Visible Library title, matching the Tauri window it replaces.
pub const LIBRARY_TITLE: &str = "Dev Console";

/// The Finder window's title. The owner's Hyprland rule matches this exact string to float, size
/// and center the window, so it is part of the app's desktop identity, not a label.
pub const FINDER_TITLE: &str = "dev-console-finder";

/// Library opening size in logical points (`src-tauri/src/windows.rs`).
pub const LIBRARY_SIZE: (f32, f32) = (1040.0, 700.0);

/// Library minimum size in logical points.
pub const LIBRARY_MIN_SIZE: (f32, f32) = (760.0, 520.0);

/// Finder size in logical points. Exact: a larger surface shows as an empty band around the
/// palette.
pub const FINDER_SIZE: (f32, f32) = (680.0, 420.0);

/// The Library is a normal, resizable, titled window. It opens hidden: the resident process starts
/// with no window and shows one on demand (tray click, control socket, compositor shortcut).
pub fn library_options() -> WindowOptions {
    WindowOptions {
        visible: false,
        min_size: Some(LIBRARY_MIN_SIZE),
        ..WindowOptions::default()
    }
}

/// The Finder's opaque, borderless, fixed-size identity.
///
/// `transparent: false` is deliberate and must not be flipped to win rounded corners: the
/// transparent variant loses its entire paint on pointer hover on the owner's
/// WebKitGTK-2.54/Hyprland/NVIDIA desktop (AGENTS.md, "Finder on Hyprland"). Hyprland rounds the
/// native window; the page background stays opaque and matched to the shell background token.
pub fn finder_options() -> WindowOptions {
    WindowOptions {
        transparent: false,
        borderless: true,
        always_on_top: true,
        resizable: false,
        visible: false,
        ..WindowOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_finder_identity_is_the_string_the_compositor_rule_matches() {
        assert_eq!(FINDER_TITLE, "dev-console-finder");
        assert_eq!(FINDER_LABEL, "finder");
        assert_eq!(LIBRARY_LABEL, "library");
        assert_eq!(LIBRARY_TITLE, "Dev Console");
    }

    #[test]
    fn the_sizes_match_the_windows_the_tauri_shell_opened() {
        assert_eq!(FINDER_SIZE, (680.0, 420.0));
        assert_eq!(LIBRARY_SIZE, (1040.0, 700.0));
        assert_eq!(LIBRARY_MIN_SIZE, (760.0, 520.0));
    }

    /// The opaque-Finder rule from AGENTS.md, as a specification assertion. Whether the mapped
    /// window is really 680×420 and floats on Hyprland is a desktop check, not something a
    /// headless test can decide.
    #[test]
    fn the_finder_asks_for_an_opaque_borderless_fixed_size_window() {
        let options = finder_options();
        assert!(
            !options.transparent,
            "the Finder surface stays opaque on WebKitGTK 2.54 + Hyprland"
        );
        assert!(options.borderless);
        assert!(
            !options.resizable,
            "a resizable Finder would break the 680×420 palette band rule"
        );
        assert!(options.always_on_top);
        assert!(
            !options.visible,
            "the resident app opens the Finder only on demand"
        );
        assert_eq!(
            options.physical_size, None,
            "size comes from the logical AppSpec size, so it scales with the display"
        );
    }

    #[test]
    fn the_library_opens_hidden_and_cannot_be_shrunk_below_the_tauri_minimum() {
        let options = library_options();
        assert!(!options.visible);
        assert!(options.resizable);
        assert!(!options.borderless);
        assert_eq!(options.min_size, Some(LIBRARY_MIN_SIZE));
    }
}
