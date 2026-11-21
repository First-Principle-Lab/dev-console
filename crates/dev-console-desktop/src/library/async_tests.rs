//! Hold completed service answers until after a close, lock or selection change.
use super::*;
use crate::hub::TestQueue;
use dev_console_core::testing::Fixture;

const VALUE: &str = "synthetic-late-answer-value";

fn library() -> (Fixture, Library, TestQueue<Library>) {
    let fixture = Fixture::new("native-late-answers");
    fixture.seed_secret("synthetic-token", VALUE);
    let hub = Hub::new(fixture.state.clone(), "0.0.0-test");
    let mut library = Library::new(hub, Exec::Inline);
    library.select_tab(Kind::Secret);
    let queue = TestQueue::default();
    library.exec = Exec::Deferred(queue.clone());
    (fixture, library, queue)
}

#[test]
fn closing_the_library_refuses_a_secret_reveal_already_in_flight() {
    let (_fixture, mut library, queue) = library();
    library.reveal();
    library.clear_sensitive();
    queue.flush(&mut library);
    assert!(!library.revealed);
    assert!(library.details.as_ref().unwrap().value.is_none());
}

#[test]
fn cancelling_an_edit_refuses_a_secret_editor_already_in_flight() {
    let (_fixture, mut library, queue) = library();
    library.start_edit();
    library.cancel_edit();
    queue.flush(&mut library);
    assert!(library.editing.is_none());
    assert!(!library.revealed);
}

#[test]
fn changing_tabs_refuses_a_secret_editor_already_in_flight() {
    let (_fixture, mut library, queue) = library();
    library.start_edit();
    library.select_tab(Kind::Command);
    queue.flush(&mut library);
    assert!(library.editing.is_none());
    assert!(!library.revealed);
}

#[test]
fn locking_refuses_metadata_and_counts_already_in_flight() {
    let (fixture, mut library, queue) = library();
    library.load_list();
    let mut effects = Effects::default();
    let locked = library.hub.library.lock_vault(&mut effects).unwrap();
    library.apply_vault(locked);
    queue.flush(&mut library);
    assert!(library.entries.is_empty());
    assert!(library.details.is_none());
    assert_eq!(library.counts[1], 0);
    assert!(!fixture.state.vault.state().unlocked);
}

#[test]
fn reopening_settings_refuses_an_earlier_recovery_reveal() {
    let (_fixture, mut library, queue) = library();
    library.open_settings();
    queue.flush(&mut library);
    library.reveal_recovery();
    library.close_settings();
    library.open_settings();
    queue.flush(&mut library);
    assert!(library.settings.as_ref().unwrap().settings.is_some());
    assert!(library.settings.as_ref().unwrap().recovery.is_none());
}

#[test]
fn locking_during_settings_load_does_not_restore_secret_counts() {
    let (_fixture, mut library, queue) = library();
    library.open_settings();
    let mut effects = Effects::default();
    let locked = library.hub.library.lock_vault(&mut effects).unwrap();
    library.apply_vault(locked);
    queue.flush(&mut library);
    let panel = library.settings.as_ref().unwrap();
    assert!(panel.settings.is_some(), "general settings still load");
    assert!(panel.current_secrets.is_none());
}

#[test]
fn secret_editor_is_secure_before_its_first_frame() {
    let (fixture, _library, _queue) = library();
    let mut editor = Editor::new(fixture.secret_draft("synthetic-token", VALUE));
    assert!(editor.value.is_secure());
    assert!(!format!("{:?}", editor.value).contains(VALUE));
    editor.wipe_value();
    assert!(editor.value.text.is_empty());
    assert!(editor.value.is_secure());
}

#[test]
fn unlocking_reloads_the_secret_tab_without_a_tab_switch() {
    let (fixture, mut library, queue) = library();
    fixture.state.vault.lock().unwrap();
    library.apply_vault(fixture.state.vault.state());
    queue.flush(&mut library);
    assert!(library.entries.is_empty());
    fixture.state.vault.unlock_with(|| Ok(())).unwrap();
    library.apply_vault(fixture.state.vault.state());
    queue.flush(&mut library);
    assert_eq!(library.entries.len(), 1);
    assert_eq!(library.entries[0].name, "synthetic-token");
    assert!(!library.revealed);
}

#[test]
fn a_suggestion_selection_refuses_an_older_saved_entry_answer() {
    let fixture = Fixture::new("suggestion-late-answer");
    fixture.seed_command("synthetic-command", "Shell", "true", &[]);
    let hub = Hub::new(fixture.state.clone(), "0.0.0-test");
    let mut library = Library::new(hub, Exec::Inline);
    library
        .suggestions
        .push(dev_console_core::history::Suggestion {
            key: "synthetic-suggestion".into(),
            command: "echo synthetic".into(),
            count: 3,
            suggested_name: "synthetic-suggestion".into(),
            project: None,
            project_root: None,
            working_directory: None,
            project_count: 3,
        });
    let queue = TestQueue::default();
    library.exec = Exec::Deferred(queue.clone());
    library.select("synthetic-command");
    library.select_suggestion("synthetic-suggestion");
    queue.flush(&mut library);
    assert_eq!(
        library.selected_suggestion.as_deref(),
        Some("synthetic-suggestion")
    );
    assert!(library.details.is_none());
    assert!(library.editing.is_none());
}

/* The CLI Proxy dashboard reads local history before the online limits, so the totals are on
 * screen without waiting for the network. These drive that order through the real refresh code
 * with synthetic sources: no transcripts are read and no request leaves the machine. */
mod usage_refresh {
    use super::*;
    use crate::library::usage::Sources;
    use dev_console_core::cliproxy::{Account, CliproxyLimits, CliproxyUsageResponse, UsageRecord};
    use dev_console_core::error::AppResult;

    fn history(session: &str) -> CliproxyUsageResponse {
        CliproxyUsageResponse {
            account: Account::default(),
            limits: None,
            limits_error: None,
            records: vec![UsageRecord {
                ts: "2026-11-12T10:00:00Z".into(),
                session: session.into(),
                model: "gpt-6-sol".into(),
                input_tokens: 100,
                cached_input_tokens: 0,
                cache_creation_tokens: 0,
                output_tokens: 10,
                reasoning_tokens: 0,
            }],
        }
    }

    fn limits(email: &str) -> CliproxyLimits {
        let mut data = history("synthetic-limits");
        data.account.email = Some(email.into());
        data.limits = Some(dev_console_core::cliproxy::LimitLimits {
            checked_at: "2026-11-12T10:00:00Z".into(),
            plan: Some("plus".into()),
            windows: Vec::new(),
            reset_credits: None,
        });
        data.limits_snapshot()
    }

    fn email(library: &Library) -> Option<String> {
        library
            .usage
            .data
            .as_ref()
            .and_then(|data| data.account.email.clone())
    }

    #[test]
    fn totals_land_before_the_online_limits_answer() {
        fn local(_: Option<String>, _: Option<String>) -> AppResult<CliproxyUsageResponse> {
            Ok(history("synthetic-local"))
        }
        fn online() -> CliproxyLimits {
            limits("synthetic@example.com")
        }
        let (_fixture, mut library, queue) = library();
        library.usage_sources = Sources {
            local,
            limits: online,
        };
        library.open_usage();
        assert!(library.usage.busy && library.usage.limits_busy);
        assert!(library.usage.data.is_none());
        assert!(queue.step(&mut library), "the history answer lands");
        let data = library.usage.data.as_ref().expect("history lands first");
        assert_eq!(data.records[0].session, "synthetic-local");
        assert!(data.limits.is_none(), "limits are still pending");
        assert!(!library.usage.busy);
        assert!(library.usage.limits_busy);
        assert!(queue.step(&mut library), "the limits answer lands next");
        assert!(!library.usage.limits_busy);
        assert_eq!(email(&library).as_deref(), Some("synthetic@example.com"));
        assert!(library.usage.data.as_ref().unwrap().limits.is_some());
    }

    #[test]
    fn a_refresh_keeps_the_previous_limits_until_the_new_ones_land() {
        fn local(_: Option<String>, _: Option<String>) -> AppResult<CliproxyUsageResponse> {
            Ok(history("synthetic-local"))
        }
        fn first() -> CliproxyLimits {
            limits("first@example.com")
        }
        fn second() -> CliproxyLimits {
            limits("second@example.com")
        }
        let (_fixture, mut library, queue) = library();
        library.usage_sources = Sources {
            local,
            limits: first,
        };
        library.open_usage();
        queue.flush(&mut library);
        assert_eq!(email(&library).as_deref(), Some("first@example.com"));
        library.usage_sources = Sources {
            local,
            limits: second,
        };
        library.refresh_usage();
        assert!(queue.step(&mut library), "the history answer lands");
        assert_eq!(
            email(&library).as_deref(),
            Some("first@example.com"),
            "the old limits stay on screen while the new ones are pending"
        );
        assert!(library.usage.limits_busy);
        assert!(queue.step(&mut library), "the limits answer lands");
        assert_eq!(email(&library).as_deref(), Some("second@example.com"));
    }

    #[test]
    fn an_older_history_answer_cannot_replace_a_newer_refresh() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static LOCAL: AtomicUsize = AtomicUsize::new(0);
        static ONLINE: AtomicUsize = AtomicUsize::new(0);
        fn local(_: Option<String>, _: Option<String>) -> AppResult<CliproxyUsageResponse> {
            let n = LOCAL.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(history(&format!("synthetic-{n}")))
        }
        fn online() -> CliproxyLimits {
            let n = ONLINE.fetch_add(1, Ordering::SeqCst) + 1;
            limits(&format!("synthetic-{n}@example.com"))
        }
        let (_fixture, mut library, queue) = library();
        library.usage_sources = Sources {
            local,
            limits: online,
        };
        library.refresh_usage();
        library.refresh_usage();
        queue.flush(&mut library);
        assert_eq!(
            library.usage.data.as_ref().unwrap().records[0].session,
            "synthetic-2"
        );
        assert_eq!(email(&library).as_deref(), Some("synthetic-1@example.com"));
        assert_eq!(
            ONLINE.load(Ordering::SeqCst),
            1,
            "a superseded refresh never asks for limits"
        );
        assert!(!library.usage.limits_busy);
    }

    #[test]
    fn a_limits_only_refresh_leaves_the_loaded_history_alone() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static LOCAL: AtomicUsize = AtomicUsize::new(0);
        static ONLINE: AtomicUsize = AtomicUsize::new(0);
        fn local(_: Option<String>, _: Option<String>) -> AppResult<CliproxyUsageResponse> {
            LOCAL.fetch_add(1, Ordering::SeqCst);
            Ok(history("synthetic-local"))
        }
        fn online() -> CliproxyLimits {
            ONLINE.fetch_add(1, Ordering::SeqCst);
            limits("refreshed@example.com")
        }
        let (_fixture, mut library, queue) = library();
        library.usage_sources = Sources {
            local,
            limits: online,
        };
        library.usage.data = Some(history("synthetic-kept"));
        library.refresh_limits();
        queue.flush(&mut library);
        assert_eq!(
            LOCAL.load(Ordering::SeqCst),
            0,
            "limits refresh rescanned history"
        );
        assert_eq!(ONLINE.load(Ordering::SeqCst), 1);
        let data = library.usage.data.as_ref().unwrap();
        assert_eq!(data.records[0].session, "synthetic-kept");
        assert_eq!(data.account.email.as_deref(), Some("refreshed@example.com"));
        assert!(!library.usage.limits_busy);
    }
}
