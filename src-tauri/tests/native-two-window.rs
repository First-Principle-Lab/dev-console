//! Real IPC dispatch across both windows, driven through Tauri's own invoke path.
//!
//! The production `#[tauri::command]` handlers are registered on a mock-runtime app, two real
//! `WebviewWindow`s labelled `library` and `finder` are created, and each call goes through
//! `tauri::test::get_ipc_response` - the same dispatch a webview's `invoke()` uses, including
//! argument deserialisation, the `WebviewWindow` extractor and the error serialisation the
//! frontend receives.
//!
//! What this proves:
//!   * both windows read the same records: a create/update/delete invoked from the library is
//!     visible through the finder's own `search_entries`/`list` call and both windows receive
//!     the same `entries-changed` payload (C08),
//!   * `open_library` (Ctrl+O) hands the entry to the library window as a `window-intent` event
//!     and is refused from the library itself (C07 handoff, C14),
//!   * secret operations are authorised per calling window and refused outright while the vault
//!     is locked, and no command response, event or search row ever carries a secret value
//!     (C05, C14),
//!   * a failed unlock leaves the manual lock in place all the way up to the IPC surface (C13).
//!
//! What it does not prove: that a WebKit webview's JavaScript `invoke()` reaches these handlers
//! (`scripts/smoke-native.sh` drives the compiled binary for that), nor anything about the tray,
//! a real display, or a real polkit prompt.
//!
//! Run through `scripts/test-native-ipc.sh`, which also starts the Secret Service provider that
//! the ignored test needs and passes `--test-threads=1` (the state profile is process-global).

use std::sync::{Arc, Mutex, MutexGuard};

use dev_console_lib::clipboard::{read_clipboard, write_clipboard};
use dev_console_lib::models::Kind;
use dev_console_lib::paths;
use dev_console_lib::state::AppState;
use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::webview::InvokeRequest;
use tauri::{Listener, Manager, WebviewWindow};

type Window = WebviewWindow<MockRuntime>;

/// The profile switch touches process-global environment variables, so the tests in this file
/// take turns even when cargo runs them in parallel threads.
fn profile_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    match LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Records every event a single window receives, so "both windows saw it" is a real assertion
/// about two separate listeners rather than one global one.
#[derive(Clone, Default)]
struct Received(Arc<Mutex<Vec<(String, Value)>>>);

impl Received {
    fn handler(&self, event: &'static str) -> impl Fn(tauri::Event) + Send + 'static {
        let sink = Arc::clone(&self.0);
        move |e: tauri::Event| {
            let parsed = serde_json::from_str(e.payload()).unwrap_or(Value::Null);
            sink.lock()
                .expect("event sink poisoned")
                .push((event.to_string(), parsed));
        }
    }

    fn of(&self, event: &str) -> Vec<Value> {
        self.0
            .lock()
            .expect("event sink poisoned")
            .iter()
            .filter(|(name, _)| name == event)
            .map(|(_, payload)| payload.clone())
            .collect()
    }

    fn clear(&self) {
        self.0.lock().expect("event sink poisoned").clear();
    }
}

struct Harness {
    app: tauri::App<MockRuntime>,
    library: Window,
    finder: Window,
    library_events: Received,
    finder_events: Received,
    state: Arc<AppState>,
    profile: std::path::PathBuf,
    _lock: MutexGuard<'static, ()>,
}

fn invoke(window: &Window, cmd: &str, body: Value) -> Result<Value, String> {
    let response = get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("test url"),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    );
    match response {
        Ok(payload) => Ok(payload
            .deserialize::<Value>()
            .expect("a JSON response body")),
        Err(error) => Err(error.as_str().unwrap_or_default().to_string()),
    }
}

fn draft(kind: Kind, name: &str, command: &str, value: &str) -> Value {
    json!({
        "kind": kind.as_str(),
        "id": null,
        "expectedRevision": 0,
        "name": name,
        "type": if kind == Kind::Command { "Shell" } else { "API key" },
        "tags": ["work"],
        "description": "",
        "notes": "two-window-notes",
        "fields": [{"id": "f1", "label": "Project", "value": "ipc-probe", "display": "text"}],
        "pinned": false,
        "command": command,
        "workingDirectory": "",
        "value": value,
    })
}

#[test]
fn approved_command_returns_output_through_native_ipc() {
    let h = Harness::new("inline-command-output");
    let saved = h
        .save(
            &h.library,
            draft(
                Kind::Command,
                "List",
                "printf files; printf warning >&2",
                "",
            ),
        )
        .expect("save command");
    let result = invoke(
        &h.library,
        "run_command",
        json!({ "request": {
        "id": saved["id"], "expectedRevision": saved["revision"], "approved": true
    } }),
    )
    .expect("run command");
    assert_eq!(result["mode"], json!("inline"));
    assert_eq!(result["stdout"], json!("files"));
    assert_eq!(result["stderr"], json!("warning"));
    assert_eq!(result["exitCode"], json!(0));
}

#[test]
fn pin_changes_only_the_requested_command_and_rejects_stale_or_finder_writes() {
    let h = Harness::new("pin-command");
    let saved = h
        .save(
            &h.library,
            draft(Kind::Command, "keep-command", "printf intact", ""),
        )
        .expect("save command");
    let request = json!({ "request": {
        "kind": "command", "id": saved["id"], "expectedRevision": saved["revision"], "pinned": true
    } });
    let denied = invoke(&h.finder, "set_pinned", request.clone()).expect_err("finder cannot pin");
    assert!(denied.contains("not allowed"), "{denied}");
    let changed = invoke(&h.library, "set_pinned", request.clone()).expect("pin command");
    assert_eq!(changed["revision"], json!(2));
    let details = invoke(
        &h.library,
        "get_entry",
        json!({ "request": { "kind": "command", "id": saved["id"] } }),
    )
    .expect("get command");
    assert_eq!(details["meta"]["pinned"], json!(true));
    assert_eq!(details["command"], json!("printf intact"));
    assert!(invoke(&h.library, "set_pinned", request).is_err());
    h.drop_profile();
}

impl Harness {
    fn new(tag: &str) -> Self {
        let lock = profile_lock();
        Self::over_profile(paths::set_disposable_profile(tag), lock)
    }

    /// A second resident process over the *same* profile files: new backend state and new
    /// windows, reading the database, vault and lock flag the previous one left behind.
    fn restart_from(profile: &std::path::Path) -> Self {
        let lock = profile_lock();
        std::env::set_var("DEV_CONSOLE_DATA_DIR", profile.join("data"));
        std::env::set_var("DEV_CONSOLE_CONFIG_DIR", profile.join("config"));
        Self::over_profile(profile.to_path_buf(), lock)
    }

    fn over_profile(profile: std::path::PathBuf, lock: MutexGuard<'static, ()>) -> Self {
        let state = AppState::bootstrap().expect("backend bootstrap");

        let managed = Arc::clone(&state);
        let app = mock_builder()
            .manage(managed)
            .invoke_handler(tauri::generate_handler![
                dev_console_lib::ipc::app_state,
                dev_console_lib::ipc::search_entries,
                dev_console_lib::ipc::list_entries,
                dev_console_lib::ipc::get_entry,
                dev_console_lib::ipc::reveal_secret,
                dev_console_lib::ipc::save_entry,
                dev_console_lib::ipc::set_pinned,
                dev_console_lib::ipc::delete_entry,
                dev_console_lib::ipc::copy_entry,
                dev_console_lib::ipc::copy_command_output,
                dev_console_lib::ipc::run_command,
                dev_console_lib::ipc::lock_vault,
                dev_console_lib::ipc::unlock_vault,
                dev_console_lib::ipc::recovery_key,
                dev_console_lib::ipc::copy_recovery_key,
                dev_console_lib::ipc::open_library,
                dev_console_lib::ipc::get_settings,
                dev_console_lib::ipc::create_backup,
                dev_console_lib::ipc::scan_project,
                dev_console_lib::ipc::import_project,
            ])
            .build(mock_context(noop_assets()))
            .expect("mock app");

        let library = tauri::WebviewWindowBuilder::new(
            &app,
            dev_console_lib::windows::LIBRARY,
            tauri::WebviewUrl::App("index.html".into()),
        )
        .build()
        .expect("library window");
        let finder = tauri::WebviewWindowBuilder::new(
            &app,
            dev_console_lib::windows::FINDER,
            tauri::WebviewUrl::App("index.html#view=finder".into()),
        )
        .build()
        .expect("finder window");

        let library_events = Received::default();
        let finder_events = Received::default();
        for (window, sink) in [(&library, &library_events), (&finder, &finder_events)] {
            window.listen("entries-changed", sink.handler("entries-changed"));
            window.listen("vault-state-changed", sink.handler("vault-state-changed"));
            window.listen("window-intent", sink.handler("window-intent"));
        }

        assert_eq!(
            library.label(),
            dev_console_lib::windows::LIBRARY,
            "the test window must carry the production label, or the authorisation checks prove nothing"
        );

        Harness {
            app,
            library,
            finder,
            library_events,
            finder_events,
            state,
            profile,
            _lock: lock,
        }
    }

    /// The windows the resident process actually has open right now.
    fn open_windows(&self) -> Vec<String> {
        let mut labels: Vec<String> = self.app.webview_windows().keys().cloned().collect();
        labels.sort();
        labels
    }

    fn save(&self, window: &Window, body: Value) -> Result<Value, String> {
        invoke(
            window,
            "save_entry",
            json!({ "request": { "draft": body } }),
        )
    }

    fn drop_profile(&self) {
        paths::remove_disposable_profile(&self.profile);
    }
}

/// C08 + C04: one record store behind two windows, driven entirely through real `invoke()`s.
#[test]
fn both_windows_see_the_same_commands_and_the_same_events() {
    let h = Harness::new("two-window-commands");
    assert_eq!(
        h.open_windows(),
        vec!["finder".to_string(), "library".to_string()],
        "the two production window labels are the ones under test"
    );

    // ---- create from the library ----
    let created = h
        .save(
            &h.library,
            draft(Kind::Command, "Deploy", "make deploy", ""),
        )
        .expect("save_entry from the library window");
    let id = created["id"].as_str().expect("saved id").to_string();
    assert_eq!(created["revision"], json!(1));

    // Both windows were notified, with the same payload, exactly once each.
    for (label, events) in [("library", &h.library_events), ("finder", &h.finder_events)] {
        let notes = events.of("entries-changed");
        assert_eq!(
            notes.len(),
            1,
            "{label} must receive exactly one entries-changed"
        );
        assert_eq!(notes[0]["id"], json!(id), "{label} sees the same record");
        assert_eq!(notes[0]["operation"], json!("created"), "{label}");
        assert_eq!(notes[0]["kind"], json!("command"), "{label}");
    }

    // ---- the other window reads it back without any restart or refresh ----
    let listed =
        invoke(&h.finder, "app_state", json!({})).expect("app_state is allowed from the finder");
    assert!(listed["vault"]["unlocked"].is_boolean());

    let found = invoke(
        &h.finder,
        "search_entries",
        json!({ "request": { "query": "Dep", "requestId": 1 } }),
    )
    .expect("search from the finder");
    assert_eq!(
        found["results"].as_array().expect("results").len(),
        1,
        "the finder finds the record the library just created"
    );
    assert_eq!(found["results"][0]["id"], json!(id));

    // ---- pin + edit from the library, delete from the library, seen by both ----
    h.library_events.clear();
    h.finder_events.clear();
    let mut pinned = draft(Kind::Command, "Deploy", "make deploy --force", "");
    pinned["id"] = json!(id);
    pinned["expectedRevision"] = json!(1);
    pinned["pinned"] = json!(true);
    let updated = h
        .save(&h.library, pinned)
        .expect("edit through the same window");
    assert_eq!(updated["revision"], json!(2));
    assert_eq!(
        h.library_events.of("entries-changed")[0]["operation"],
        json!("updated")
    );
    assert_eq!(
        h.finder_events.of("entries-changed")[0]["operation"],
        json!("updated"),
        "the finder is told about the edit it did not perform"
    );

    let detail = invoke(
        &h.library,
        "get_entry",
        json!({ "request": { "kind": "command", "id": id } }),
    )
    .expect("details from the library");
    assert_eq!(
        detail["command"],
        json!("make deploy --force"),
        "the edit is what the list is now showing"
    );

    invoke(
        &h.library,
        "delete_entry",
        json!({ "request": { "kind": "command", "id": id, "expectedRevision": 2 } }),
    )
    .expect("delete through the library");
    assert_eq!(
        h.finder_events.of("entries-changed").last().expect("event")["operation"],
        json!("deleted"),
        "both windows are told about the delete"
    );
    assert_eq!(
        h.library_events
            .of("entries-changed")
            .last()
            .expect("event")["id"],
        json!(id)
    );

    // The deleted record is gone for both windows, not just for the caller. `get_entry` is
    // library-only, so the second window proves it through the search it is allowed to run.
    match invoke(
        &h.library,
        "get_entry",
        json!({ "request": { "kind": "command", "id": id } }),
    ) {
        Ok(value) => panic!("a deleted record must not be readable after the delete: {value}"),
        Err(message) => assert!(
            message.contains("not found"),
            "expected a not-found rejection, got {message}"
        ),
    }

    let after = invoke(
        &h.finder,
        "search_entries",
        json!({ "request": { "query": "Deploy", "requestId": 2 } }),
    )
    .expect("search");
    assert!(
        after["results"].as_array().expect("results").is_empty(),
        "the finder stops offering what the library deleted"
    );

    h.drop_profile();
}

/// C07 handoff + C14: the Finder has no editor, and the library has no Finder handoff.
#[test]
fn privileged_operations_are_authorised_by_the_calling_window() {
    let h = Harness::new("two-window-permissions");
    let created = h
        .save(&h.library, draft(Kind::Command, "Probe", "echo probe", ""))
        .expect("create through the library");
    let id = created["id"].as_str().expect("id").to_string();
    h.library_events.clear();
    h.finder_events.clear();

    // Everything that can change data, reveal, execute or export is library-only. Trying them
    // from the finder's calling context must be refused before the backend is touched.
    let refused: Vec<(&str, Value)> = vec![
        ("list_entries", json!({ "request": { "kind": "command" } })),
        (
            "get_entry",
            json!({ "request": { "kind": "command", "id": id } }),
        ),
        (
            "save_entry",
            json!({ "request": { "draft": draft(Kind::Command, "Sneak", "echo sneak", "") } }),
        ),
        (
            "delete_entry",
            json!({ "request": { "kind": "command", "id": id, "expectedRevision": 1 } }),
        ),
        (
            "reveal_secret",
            json!({ "request": { "kind": "secret", "id": id } }),
        ),
        (
            "run_command",
            json!({ "request": { "id": id, "expectedRevision": 1, "approved": true } }),
        ),
        ("get_settings", json!({})),
        ("create_backup", json!({})),
    ];
    for (command, body) in refused {
        match invoke(&h.finder, command, body) {
            Err(message) => assert!(
                message.contains("not allowed to call finder"),
                "{command} from the finder must be a window-permission rejection, got {message}"
            ),
            Ok(value) => panic!("{command} must not be callable from the finder, got {value}"),
        }
    }
    assert!(
        h.library_events.of("entries-changed").is_empty()
            && h.finder_events.of("entries-changed").is_empty(),
        "a refused call must not change anything either window can see"
    );

    // Permissions are not simply switched off globally: the same calls work from the library.
    invoke(
        &h.library,
        "list_entries",
        json!({ "request": { "kind": "command" } }),
    )
    .expect("the library may list");
    invoke(&h.library, "get_settings", json!({})).expect("the library may read settings");

    // Copying is deliberately allowed from the finder. With no clipboard provider on the
    // session the copy itself fails, and that must be a clipboard failure rather than the
    // window-permission rejection the calls above got - the authorisation is what is under
    // test here, not the desktop's clipboard.
    match invoke(
        &h.finder,
        "copy_entry",
        json!({ "request": { "kind": "command", "id": id } }),
    ) {
        Ok(result) => assert_eq!(result["kind"], json!("command")),
        Err(message) => assert!(
            message.contains("clipboard"),
            "the finder is authorised to copy, so only the clipboard itself may refuse: {message}"
        ),
    }
    invoke(
        &h.finder,
        "open_library",
        json!({ "request": { "kind": "command", "entryId": id } }),
    )
    .expect("the finder may hand an entry to the library");
    let intents = h.library_events.of("window-intent");
    assert_eq!(intents.len(), 1, "the library window receives the handoff");
    assert_eq!(intents[0]["entryId"], json!(id));
    assert_eq!(intents[0]["action"], json!("open-entry"));
    match invoke(
        &h.library,
        "open_library",
        json!({ "request": { "kind": "command", "entryId": id } }),
    ) {
        Err(message) => assert!(message.contains("not allowed to call library")),
        Ok(_) => panic!("the library must not be able to call the finder-only handoff"),
    }

    h.drop_profile();
}

/// C05 + C08 + C13 + C14, with a real keyring-backed vault: secret values never cross the IPC
/// boundary except through an explicit reveal, and both windows track the vault state.
#[test]
#[ignore = "needs a real Secret Service provider, started by scripts/test-native-ipc.sh"]
fn both_windows_track_the_vault_and_secret_values_stay_sealed() {
    let h = Harness::new("two-window-secrets");
    if !h.state.vault.is_unlocked() {
        h.drop_profile();
        panic!("the harness did not open the vault through the real provider");
    }

    let marker = "synthetic-two-window-value";
    let created = h
        .save(
            &h.library,
            draft(Kind::Secret, "Registry token", "", marker),
        )
        .expect("create a secret through the library window");
    let id = created["id"].as_str().expect("id").to_string();

    // Both windows know a record changed; the event carries metadata only.
    for (label, events) in [("library", &h.library_events), ("finder", &h.finder_events)] {
        let notes = events.of("entries-changed");
        assert_eq!(notes.len(), 1, "{label} sees the secret being created");
        assert_eq!(notes[0]["kind"], json!("secret"));
        let serialised = notes[0].to_string();
        assert!(
            !serialised.contains(marker) && !serialised.contains("two-window-notes"),
            "{label} must not be told the value or the notes, only the id and revision"
        );
    }

    // The finder searches commands and secrets together, but only by metadata.
    let found = invoke(
        &h.finder,
        "search_entries",
        json!({ "request": { "query": "Registry", "requestId": 1 } }),
    )
    .expect("search");
    assert_eq!(found["results"].as_array().expect("results").len(), 1);
    let value = invoke(
        &h.finder,
        "search_entries",
        json!({ "request": { "query": marker, "requestId": 2 } }),
    )
    .expect("search for the value");
    assert!(
        value["results"].as_array().expect("results").is_empty(),
        "searching a secret value must never find the secret"
    );

    // Details are masked; only an explicit reveal from the library returns the value.
    let details = invoke(
        &h.library,
        "get_entry",
        json!({ "request": { "kind": "secret", "id": id } }),
    )
    .expect("secret details");
    assert_eq!(
        details["value"],
        Value::Null,
        "a normal inspection must not carry the value"
    );
    let revealed = invoke(
        &h.library,
        "reveal_secret",
        json!({ "request": { "kind": "secret", "id": id } }),
    )
    .expect("explicit reveal from the library");
    assert_eq!(revealed["value"], json!(marker));

    h.library_events.clear();
    h.finder_events.clear();

    // ---- lock: both windows are told, and every secret path shuts ----
    let locked = invoke(&h.library, "lock_vault", json!({})).expect("lock through the library");
    assert_eq!(locked["unlocked"], json!(false));
    assert_eq!(locked["manualLock"], json!(true));
    for (label, events) in [("library", &h.library_events), ("finder", &h.finder_events)] {
        let states = events.of("vault-state-changed");
        assert_eq!(states.len(), 1, "{label} is told about the lock");
        assert_eq!(states[0]["unlocked"], json!(false));
        assert_eq!(states[0]["manualLock"], json!(true));
    }

    // Every secret path refuses while locked - the library-only ones called from the library,
    // and the copy called from the finder, which is the window that is allowed to call it.
    let locked_calls: Vec<(&str, &Window, Value)> = vec![
        (
            "list_entries",
            &h.library,
            json!({ "request": { "kind": "secret" } }),
        ),
        (
            "get_entry",
            &h.library,
            json!({ "request": { "kind": "secret", "id": id } }),
        ),
        (
            "reveal_secret",
            &h.library,
            json!({ "request": { "kind": "secret", "id": id } }),
        ),
        (
            "copy_entry",
            &h.finder,
            json!({ "request": { "kind": "secret", "id": id } }),
        ),
        ("create_backup", &h.library, json!({})),
    ];
    for (command, window, body) in locked_calls {
        match invoke(window, command, body) {
            Err(message) => assert!(
                message.contains("locked"),
                "{command} while locked must be refused as locked, got {message}"
            ),
            Ok(value) => panic!("{command} succeeded while the vault is locked: {value}"),
        }
    }

    // Secret metadata disappears from the combined search for *both* windows.
    for (label, window) in [("library", &h.library), ("finder", &h.finder)] {
        let after = invoke(
            window,
            "search_entries",
            json!({ "request": { "query": "Registry", "requestId": 3 } }),
        )
        .expect("search while locked");
        assert!(
            after["results"].as_array().expect("results").is_empty(),
            "{label} must not see secret metadata while the vault is locked"
        );
    }

    // Commands stay fully usable while the secrets are sealed.
    let while_locked = invoke(&h.finder, "app_state", json!({})).expect("app state");
    assert_eq!(while_locked["vault"]["unlocked"], json!(false));
    let usable = invoke(
        &h.library,
        "list_entries",
        json!({ "request": { "kind": "command" } }),
    )
    .expect("commands must stay usable while the vault is locked");
    assert!(usable.is_array());

    // ---- a failed unlock must not undo the manual lock, at the IPC surface too ----
    // This is the ordering regression: authentication may succeed, but if the vault does not
    // open, the flag file and the in-memory lock have to survive.
    h.library_events.clear();
    h.finder_events.clear();
    match invoke(&h.library, "unlock_vault", json!({})) {
        // On a desktop with a working polkit agent this could legitimately succeed.
        Ok(state) => assert_eq!(
            state["unlocked"],
            json!(true),
            "if unlock reports success the vault really has to be open"
        ),
        Err(message) => {
            // No mechanism installed, a dismissed prompt, or a vault that will not decrypt: in
            // every one of those cases the manual lock has to survive untouched.
            assert!(!h.state.vault.is_unlocked(), "refused unlock: {message}");
            assert!(
                h.state.vault.manual_lock(),
                "a refused unlock must keep the persistent manual lock ({message})"
            );
            assert!(
                h.state.vault.list_secrets().is_err(),
                "a refused unlock must not make secrets readable ({message})"
            );
            for (label, events) in [("library", &h.library_events), ("finder", &h.finder_events)] {
                for state in events.of("vault-state-changed") {
                    assert_eq!(
                        state["unlocked"],
                        json!(false),
                        "{label} was told the vault opened after a refused unlock"
                    );
                    assert_eq!(state["manualLock"], json!(true), "{label}");
                }
            }
            for (label, window) in [("library", &h.library), ("finder", &h.finder)] {
                let seen = invoke(window, "app_state", json!({})).expect("app state");
                assert_eq!(
                    seen["vault"]["unlocked"],
                    json!(false),
                    "{label} still sees a locked vault after the refused unlock"
                );
                assert_eq!(seen["vault"]["manualLock"], json!(true), "{label}");
            }
            match invoke(
                &h.library,
                "reveal_secret",
                json!({ "request": { "kind": "secret", "id": id } }),
            ) {
                Err(m) => assert!(m.contains("locked"), "reveal after refusal: {m}"),
                Ok(_) => panic!("a refused unlock made a secret revealable"),
            }
        }
    }

    // The successful path, exercised without a polkit prompt: opening the vault is only allowed
    // to broadcast `unlocked` once the vault has actually decrypted.
    h.state
        .vault
        .unlock_with(|| Ok(()))
        .expect("unlock with authentication satisfied");
    h.library_events.clear();
    h.finder_events.clear();
    let unlocked = invoke(&h.library, "app_state", json!({})).expect("app state after unlock");
    assert_eq!(unlocked["vault"]["unlocked"], json!(true));
    assert_eq!(
        unlocked["vault"]["manualLock"],
        json!(false),
        "a completed open is the only thing that clears the manual lock"
    );
    let readable = invoke(
        &h.finder,
        "search_entries",
        json!({ "request": { "query": "Registry", "requestId": 4 } }),
    )
    .expect("search after unlock");
    assert_eq!(
        readable["results"].as_array().expect("results").len(),
        1,
        "the finder sees the secret metadata again, still without its value"
    );
    assert!(readable.to_string().contains("Registry"));
    assert!(
        !readable.to_string().contains(marker),
        "the combined search result must never carry the secret value"
    );

    // Commands and secrets are both usable again, and the reveal is the only thing that
    // returns the value.
    let usable = invoke(
        &h.library,
        "list_entries",
        json!({ "request": { "kind": "command" } }),
    )
    .expect("commands after unlock");
    assert!(usable.is_array());
    let revealed_again = invoke(
        &h.library,
        "reveal_secret",
        json!({ "request": { "kind": "secret", "id": id } }),
    )
    .expect("reveal after a completed unlock");
    assert_eq!(revealed_again["value"], json!(marker));

    h.drop_profile();
}

/// The recovery credential is a secret like any other, and it is the one that decrypts every
/// archive this profile has ever sealed. Driven entirely through real IPC dispatch: it must be
/// refused by a locked vault - without leaking anything in the refusal - and it must only ever
/// reach the clipboard through the protected native copy path.
#[test]
#[ignore = "needs a real Secret Service provider, started by scripts/test-native-ipc.sh"]
fn the_recovery_credential_stays_behind_the_vault_and_copies_natively() {
    let h = Harness::new("two-window-recovery");
    if !h.state.vault.is_unlocked() {
        h.drop_profile();
        panic!("the harness did not open the vault through the real provider");
    }
    let display = std::env::var_os("DISPLAY").is_some();

    let revealed = invoke(&h.library, "recovery_key", json!({}))
        .expect("the owner may read the credential while the vault is open");
    let secret = revealed["secret"]
        .as_str()
        .expect("a credential")
        .to_string();
    assert!(
        secret.starts_with("AGE-SECRET-KEY-1"),
        "the reveal returns the age identity"
    );
    assert!(revealed["path"]
        .as_str()
        .expect("path")
        .ends_with("recovery.agekey"));

    // The finder is not allowed to ask at all.
    for command in ["recovery_key", "copy_recovery_key"] {
        match invoke(&h.finder, command, json!({})) {
            Err(message) => assert!(
                message.contains("not allowed to call finder"),
                "{command} from the finder must be a window rejection, got {message}"
            ),
            Ok(value) => panic!("{command} must not be callable from the finder: {value}"),
        }
    }

    // ---- manual lock through the IPC surface: the reveal has to shut with it ----
    let locked = invoke(&h.library, "lock_vault", json!({})).expect("lock through the library");
    assert_eq!(locked["unlocked"], json!(false));
    // The library is the window that is allowed to ask, so this is the real gate: locked means
    // refused, and the refusal must not repeat the credential back to the renderer.
    for command in ["recovery_key", "copy_recovery_key"] {
        match invoke(&h.library, command, json!({})) {
            Err(message) => {
                assert!(
                    message.contains("locked"),
                    "{command} while locked must refuse as locked, got {message}"
                );
                assert!(
                    !message.contains(&secret),
                    "the refusal leaked the credential: {message}"
                );
            }
            Ok(value) => panic!(
                "{command} handed the recovery credential out while the vault is locked: {value}"
            ),
        }
    }
    // The finder had no access before the lock and still has none afterwards.
    for command in ["recovery_key", "copy_recovery_key"] {
        match invoke(&h.finder, command, json!({})) {
            Err(message) => assert!(
                message.contains("not allowed to call finder") && !message.contains(&secret),
                "{command} from the finder must stay a window rejection: {message}"
            ),
            Ok(value) => panic!("{command} reached the finder while locked: {value}"),
        }
    }
    // A restart in this state must refuse too: the manual lock is persistent, not per-window.
    let profile = h.profile.clone();
    drop(h);
    let h = Harness::restart_from(&profile);
    assert!(
        h.state.vault.manual_lock(),
        "the manual lock survived into the restarted process"
    );
    match invoke(&h.library, "recovery_key", json!({})) {
        Err(message) => assert!(
            !message.contains(&secret) && message.contains("locked"),
            "a restarted locked vault must refuse without echoing anything: {message}"
        ),
        Ok(value) => panic!("a restarted locked vault returned the credential: {value}"),
    }

    // ---- an authenticated unlock reopens the path, and the copy is the guarded native one ----
    h.state
        .vault
        .unlock_with(|| Ok(()))
        .expect("unlock with authentication satisfied");
    let again = invoke(&h.library, "recovery_key", json!({}))
        .expect("reveal after an authenticated unlock");
    assert_eq!(
        again["secret"],
        json!(secret),
        "the same credential, reachable only through the vault"
    );

    let copied = match invoke(&h.library, "copy_recovery_key", json!({})) {
        Ok(result) => result,
        Err(message) => {
            // With no clipboard provider the copy may fail, but only as a clipboard failure -
            // the authorisation is not what is in question here.
            h.drop_profile();
            assert!(
                message.contains("clipboard"),
                "only the clipboard itself may refuse an authorised copy: {message}"
            );
            return;
        }
    };
    assert_eq!(copied["id"], json!("recovery-credential"));
    assert_eq!(copied["kind"], json!("secret"));
    assert!(
        copied["expiresAtMs"].as_i64().expect("an expiry") > dev_console_lib::repository::unix_ms(),
        "the copy is expiry-bound like every other secret copy"
    );
    assert!(
        !copied.to_string().contains(&secret),
        "the copy result must never carry the value back to the renderer"
    );

    if display {
        assert_eq!(
            read_clipboard().expect("paste"),
            secret,
            "the credential reached the real selection"
        );

        // Something copied afterwards belongs to the user, not to the app: locking must leave
        // it alone, while the app's own copy of the credential goes away.
        let unrelated = "copied-by-another-application";
        write_clipboard(unrelated).expect("unrelated copy");
        invoke(&h.library, "lock_vault", json!({})).expect("lock again");
        assert_eq!(
            read_clipboard().expect("paste after lock"),
            unrelated,
            "an unrelated clipboard value survives the lock"
        );

        // And with the credential still the owned copy, the lock wipes it.
        h.state.vault.unlock_with(|| Ok(())).expect("unlock again");
        invoke(&h.library, "copy_recovery_key", json!({})).expect("copy again");
        assert_eq!(read_clipboard().expect("paste"), secret);
        invoke(&h.library, "lock_vault", json!({})).expect("lock");
        assert_eq!(
            read_clipboard().expect("paste").trim(),
            "",
            "the app-owned copy of the credential is cleared when the vault locks"
        );
    }

    h.drop_profile();
}

#[test]
fn project_import_round_trips_through_ipc_and_preserves_execution_approval() {
    let h = Harness::new("project-import-ipc");
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(
        folder.path().join("Makefile"),
        "probe:\n\t@pwd\nsecond:\n\t@true\n",
    )
    .unwrap();
    let path = std::fs::canonicalize(folder.path()).unwrap();
    let scan_request = json!({"request": {"path": path}});
    assert!(invoke(&h.finder, "scan_project", scan_request.clone()).is_err());
    assert!(invoke(
        &h.finder,
        "import_project",
        json!({"request": {"path": path, "names": [], "all": true}})
    )
    .is_err());
    let scanned = invoke(&h.library, "scan_project", scan_request.clone()).unwrap();
    assert_eq!(scanned["candidates"].as_array().unwrap().len(), 2);
    assert!(h.state.repo.all_command_records().unwrap().is_empty());
    let names: Vec<_> = scanned["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].clone())
        .collect();
    let request = json!({"request": {"path": path, "names": names}});
    assert_eq!(
        invoke(&h.library, "import_project", request.clone()).unwrap()["imported"],
        json!(2)
    );
    let rescanned = invoke(&h.library, "scan_project", scan_request).unwrap();
    assert!(rescanned["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["alreadyImported"] == true));
    let repeated = invoke(&h.library, "import_project", request).unwrap();
    assert_eq!(repeated["imported"], json!(0));
    assert_eq!(repeated["duplicates"], json!(2));
    let records = h.state.repo.all_command_records().unwrap();
    let record = &records
        .iter()
        .find(|(record, _, _)| record.command == "make probe")
        .unwrap()
        .0;
    assert_eq!(record.working_directory, path.to_str().unwrap());
    assert!(!h.state.repo.approval_state(record).unwrap());
    let denied = invoke(
        &h.library,
        "run_command",
        json!({"request": {"id": record.id, "expectedRevision": record.revision, "approved": false}}),
    );
    assert!(denied.is_err());
    let output = invoke(&h.library, "run_command", json!({"request": {"id": record.id, "expectedRevision": record.revision, "approved": true}})).unwrap();
    assert_eq!(output["stdout"], json!(format!("{}\n", path.display())));
    assert_eq!(output["exitCode"], json!(0));
}

#[test]
fn project_cli_round_trips_over_a_real_control_socket() {
    use dev_console_lib::control;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    let h = Harness::new("project-control-socket");
    let folder = tempfile::tempdir().unwrap();
    let runtime = folder.path().join("runtime");
    let old_runtime = std::env::var_os("DEV_CONSOLE_RUNTIME_DIR");
    std::env::set_var("DEV_CONSOLE_RUNTIME_DIR", &runtime);
    std::fs::write(folder.path().join("Makefile"), "probe:\n\t@true\n").unwrap();
    control::serve(h.app.handle(), Arc::clone(&h.state));
    let mut stream = UnixStream::connect(control::socket_path()).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    writeln!(
        stream,
        "{}",
        json!({"command":"project.scan", "path":folder.path()})
    )
    .unwrap();
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).unwrap();
    let reply: Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply["ok"], json!(true));
    assert!(reply["message"].as_str().unwrap().contains("make probe"));
    assert!(h.state.repo.all_command_records().unwrap().is_empty());
    assert_eq!(
        control::run_project(folder.path().to_str().unwrap(), false).unwrap(),
        0
    );
    assert!(h.state.repo.all_command_records().unwrap().is_empty());
    assert_eq!(
        control::run_project(folder.path().to_str().unwrap(), true).unwrap(),
        0
    );
    assert_eq!(
        control::run_project(folder.path().to_str().unwrap(), true).unwrap(),
        0
    );
    assert_eq!(h.state.repo.all_command_records().unwrap().len(), 1);
    control::remove_socket();
    if let Some(old) = old_runtime {
        std::env::set_var("DEV_CONSOLE_RUNTIME_DIR", old);
    } else {
        std::env::remove_var("DEV_CONSOLE_RUNTIME_DIR");
    }
}
