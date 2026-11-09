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
