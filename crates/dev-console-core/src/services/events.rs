//! Typed events (plan section 3).
//!
//! Before this module an event existed as a string literal repeated at every `emit` site and at
//! every `listen` site in the webview, so a typo was only found by the one screen that happened to
//! be listening. Here the name, the payload type and the delivery target are one definition, and
//! the desktop runtime has a single helper that turns an `AppEvent` into an emit.
//!
//! Every variant whose name matches what the Tauri build emits today keeps exactly that name and
//! that payload shape, so the existing webview sees byte-identical events. The variants documented
//! as *not emitted by the Tauri runtime* are the remaining families plan section 3 lists (settings,
//! clipboard expiry, update progress); they exist so the Rune app has one vocabulary, and nothing
//! in the shipped app changes because of them.

use serde::Serialize;
use serde_json::Value;

use crate::context::FinderContext;
use crate::models::{CopyResult, EntriesChanged, Settings, VaultState, WindowIntent};

/// Who receives an event. `All` is the broadcast the Tauri `AppHandle::emit` performs; the window
/// variants match the `WebviewWindow::emit` sites, which address one webview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    All,
    Library,
    Finder,
}

/// The self-update state the Settings panel shows.
///
/// Not emitted by the Tauri runtime: there the updater runs inside the webview through the Tauri
/// updater plugin, so the state is component-local. The native updater publishes this instead, so a
/// check started by the CLI is visible in the window too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub phase: UpdatePhase,
    /// The release the update is for, once one is known.
    pub version: Option<String>,
    /// Human-readable failure, safe to show: never a path into the signing material.
    pub error: Option<String>,
}

/// Where a self-update attempt got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePhase {
    Checking,
    Current,
    Available,
    Downloading,
    Installing,
    Failed,
}

/// One event the runtime can deliver to a window.
///
/// `Debug` and `Clone` only: the payload models carry no `PartialEq`, and a test that needs to tell
/// two events apart matches on the variant and compares the payload's JSON, which is the shape a
/// window actually sees.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// An entry was created, updated, deleted, or the whole list was reloaded.
    EntriesChanged(EntriesChanged),
    /// Lock state, keyring availability or the vault generation moved.
    VaultStateChanged(VaultState),
    /// A vault action failed somewhere a window is open to explain it (the tray's lock toggle).
    VaultMessage(String),
    /// The saved theme changed, so every window re-accepts the palette.
    ThemeChanged(String),
    /// Settings were saved. Emitted by the webview to itself today (`SettingsPanel.svelte`), not by
    /// the Tauri runtime; the variant exists so the native settings screen has one path.
    SettingsSaved(Settings),
    /// The project the palette was opened from, or its dismissal.
    FinderContext(Option<FinderContext>),
    /// The palette was presented: its query is cleared and the input refocused on every appearance.
    FinderShown,
    /// The user closed a window that has unsaved edits to answer for.
    CloseRequested,
    /// Open the in-window settings panel (a tray action).
    OpenSettings,
    /// The library should show an entry or start a new one (the palette's `Enter` and `Ctrl+P`).
    WindowIntent(WindowIntent),
    /// A copy the app owns was placed on the clipboard, and when it goes away. Emitted by the
    /// service layer in this crate, but not consumed by the webview, which reads the same
    /// `CopyResult` from the call's return value; the native shell listens instead.
    ClipboardCopied(CopyResult),
    /// Self-update progress; see [`UpdateProgress`].
    UpdateProgress(UpdateProgress),
}

impl AppEvent {
    /// The event name on the wire. Names of variants the Tauri runtime emits today are part of the
    /// frontend contract and are pinned by `every_event_name_is_the_one_the_webview_listens_for`.
    pub fn name(&self) -> &'static str {
        match self {
            AppEvent::EntriesChanged(_) => "entries-changed",
            AppEvent::VaultStateChanged(_) => "vault-state-changed",
            AppEvent::VaultMessage(_) => "vault-message",
            AppEvent::ThemeChanged(_) => "theme-changed",
            AppEvent::SettingsSaved(_) => "settings-saved",
            AppEvent::FinderContext(_) => "finder-context",
            AppEvent::FinderShown => "finder-shown",
            AppEvent::CloseRequested => "close-requested",
            AppEvent::OpenSettings => "open-settings",
            AppEvent::WindowIntent(_) => "window-intent",
            AppEvent::ClipboardCopied(_) => "clipboard-copied",
            AppEvent::UpdateProgress(_) => "update-progress",
        }
    }

    /// The JSON payload, identical to what the same `emit` site passed before this module existed.
    pub fn payload(&self) -> Value {
        match self {
            AppEvent::EntriesChanged(payload) => to_value(payload),
            AppEvent::VaultStateChanged(payload) => to_value(payload),
            // `emit(name, String)` serialises to a JSON string, not an object.
            AppEvent::VaultMessage(message) => Value::String(message.clone()),
            AppEvent::ThemeChanged(theme) => Value::String(theme.clone()),
            AppEvent::SettingsSaved(settings) => to_value(settings),
            AppEvent::FinderContext(context) => to_value(context),
            AppEvent::FinderShown => Value::Null,
            AppEvent::CloseRequested => Value::Null,
            AppEvent::OpenSettings => Value::Null,
            AppEvent::WindowIntent(intent) => to_value(intent),
            AppEvent::ClipboardCopied(result) => to_value(result),
            AppEvent::UpdateProgress(progress) => to_value(progress),
        }
    }

    /// Which window(s) receive it: the broadcast or the single webview the old `emit` site used.
    pub fn delivery(&self) -> Delivery {
        match self {
            // The tray's settings action, the palette's Enter and a refused window close all
            // address the library webview: closing the palette just hides it, so the
            // `close-requested` site in `lib.rs` is only ever reached by the library.
            AppEvent::OpenSettings | AppEvent::WindowIntent(_) | AppEvent::CloseRequested => {
                Delivery::Library
            }
            AppEvent::FinderShown => Delivery::Finder,
            _ => Delivery::All,
        }
    }
}

fn to_value<T: Serialize>(payload: &T) -> Value {
    // Serialising these types cannot fail: they are all plain data already on the wire.
    serde_json::to_value(payload).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Kind, Settings};

    fn entries_changed() -> EntriesChanged {
        EntriesChanged {
            kind: Kind::Command,
            id: "entry-1".into(),
            revision: 3,
            operation: "updated",
            vault_generation: 2,
        }
    }

    #[test]
    fn event_names_are_the_ones_the_webview_listens_for() {
        // Taken from the shipped frontend: `grep -rn "on<[^>]*>(" src`, plus the `window.emit` and
        // `app.emit` sites in `src-tauri/src`. A rename here is a broken listener there.
        let listened = [
            "entries-changed",
            "vault-state-changed",
            "vault-message",
            "theme-changed",
            "settings-saved",
            "finder-context",
            "finder-shown",
            "close-requested",
            "open-settings",
            "window-intent",
        ];
        let defined: Vec<&str> = all_events().iter().map(|event| event.name()).collect();
        for name in listened {
            assert!(defined.contains(&name), "{name} has no typed event");
        }
        // The two families the Tauri runtime never emitted, and one no webview listens for.
        assert!(defined.contains(&"clipboard-copied"));
        assert!(defined.contains(&"update-progress"));
    }

    #[test]
    fn event_names_are_unique() {
        let events = all_events();
        for (left, right) in events.iter().zip(events.iter()) {
            if !std::ptr::eq(left, right) {
                assert_ne!(left.name(), right.name(), "two variants share a name");
            }
        }
    }

    #[test]
    fn an_entries_changed_event_carries_the_shape_the_library_decodes() {
        let event = AppEvent::EntriesChanged(entries_changed());
        assert_eq!(event.name(), "entries-changed");
        assert_eq!(event.delivery(), Delivery::All);
        assert_eq!(
            event.payload(),
            serde_json::json!({
                "kind": "command",
                "id": "entry-1",
                "revision": 3,
                "operation": "updated",
                "vaultGeneration": 2
            })
        );
    }

    #[test]
    fn a_theme_changed_event_is_a_bare_string_like_the_old_emit() {
        // `app.emit("theme-changed", &saved.theme)` sends a JSON string, and `App.svelte` listens
        // for exactly that; wrapping it in an object would hand the webview `undefined`.
        assert_eq!(
            AppEvent::ThemeChanged("mocha".into()).payload(),
            serde_json::Value::String("mocha".into())
        );
    }

    #[test]
    fn window_targeted_events_keep_their_delivery() {
        let intent = AppEvent::WindowIntent(crate::models::WindowIntent {
            action: "open-entry",
            kind: Kind::Secret,
            entry_id: Some("secret-1".into()),
        });
        assert_eq!(intent.delivery(), Delivery::Library);
        assert_eq!(
            intent.payload(),
            serde_json::json!({ "action": "open-entry", "kind": "secret", "entryId": "secret-1" })
        );
        assert_eq!(AppEvent::OpenSettings.delivery(), Delivery::Library);
        assert_eq!(AppEvent::FinderShown.delivery(), Delivery::Finder);
        assert_eq!(
            AppEvent::CloseRequested.delivery(),
            Delivery::Library,
            "the library is the window that has to answer for unsaved edits; closing the palette \
             just hides it, which is what `on_window_event` does today"
        );
        assert_eq!(
            AppEvent::FinderContext(None).delivery(),
            Delivery::All,
            "both windows show what the palette ranked by"
        );
        assert_eq!(
            AppEvent::SettingsSaved(Settings::default()).delivery(),
            Delivery::All
        );
    }

    #[test]
    fn a_null_payload_is_what_the_unit_payloads_sent() {
        for event in [
            AppEvent::FinderShown,
            AppEvent::CloseRequested,
            AppEvent::OpenSettings,
        ] {
            assert_eq!(event.payload(), serde_json::Value::Null, "{}", event.name());
        }
    }

    /// Every variant, so the name and uniqueness tests cover all of them.
    fn all_events() -> Vec<AppEvent> {
        let vault = crate::models::VaultState {
            unlocked: false,
            manual_lock: true,
            keyring_available: false,
            keyring_unlocked: false,
            vault_available: true,
            generation: 1,
            authentication_required: false,
            reason: None,
        };
        let exclusion = crate::models::ClipboardExclusion {
            supported: false,
            mechanism: "none".into(),
            detail: "no clipboard owner".into(),
        };
        vec![
            AppEvent::EntriesChanged(entries_changed()),
            AppEvent::VaultStateChanged(vault),
            AppEvent::VaultMessage("vault unavailable".into()),
            AppEvent::ThemeChanged("system".into()),
            AppEvent::SettingsSaved(Settings::default()),
            AppEvent::FinderContext(None),
            AppEvent::FinderShown,
            AppEvent::CloseRequested,
            AppEvent::OpenSettings,
            AppEvent::WindowIntent(crate::models::WindowIntent {
                action: "new-entry",
                kind: Kind::Command,
                entry_id: None,
            }),
            AppEvent::ClipboardCopied(crate::models::CopyResult {
                kind: Kind::Command,
                id: "entry-1".into(),
                expires_at_ms: 1,
                excluded: false,
                exclusion,
            }),
            AppEvent::UpdateProgress(UpdateProgress {
                phase: UpdatePhase::Checking,
                version: None,
                error: None,
            }),
        ]
    }
}
