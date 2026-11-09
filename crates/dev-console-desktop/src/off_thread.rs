//! Off-thread work, delivered: a channel, a Rune wakeup, and the guards from `dev_console_core`.
//!
//! Plan §3 asks for four things in one sentence: "Run blocking database, crypto, filesystem,
//! authentication, scanning, and execution operations **outside the UI thread**. Deliver results
//! through **channels and Rune wakeups**. Retain **request IDs, revision checks, and vault
//! generations** so obsolete results cannot restore stale or sensitive state." The first two are
//! Rune's [`Latest`], which is used here rather than reimplemented; the third is
//! [`dev_console_core::freshness`], which Rune has no way to know about. [`OffThread`] is where they
//! meet, and the meeting is the interesting part: an answer that arrives over a channel is only
//! useful if the view can tell whether it still describes what the user is looking at.
//!
//! ## Why [`Latest`] and not a thread per call
//!
//! `rune::Latest` (at the pinned rev, `crates/rune/src/latest.rs`) runs one request per stream on up
//! to four shared workers, started on demand and retired after ten idle seconds, and wakes once when
//! an answer lands. It also *supersedes*: a request the UI has already replaced cannot land and cannot
//! wake, because [`Latest::take`] hands over one answer and a stale frame is worse than a missing one.
//! That is the coarse half of the staleness story and it is Rune's. The fine half - the vault was
//! locked, somebody else saved this record, the user cleared the box - is Dev Console's, and lives in
//! [`dev_console_core::freshness`], where the palette can share it with the app that still ships.
//!
//! ## What is checked here, and what is not
//!
//! These tests run on a headless host, so they can count wakeups and move values between threads, and
//! they do. What no headless host can settle is that a wakeup produced a *frame*: that the loop woke,
//! `poll` ran, and the window repainted. Per `lib.rs` and plan §5 that stays UNVERIFIED in this
//! container and is carried to the owner's desktop check; nothing here claims to have drawn anything.
//!
//! ## What a view does with it
//!
//! Build one per thing the view waits for, hand it the window's [`Waker`], and drain it from
//! `AppSpec::poll`: ask in the key handler, and in `poll` treat `Applied` as "paint this" and
//! `None` as "nothing to do" - which covers both nothing-landed and landed-but-refused, because in
//! neither case may the screen change. `is_pending` is the spinner, and `abandon` is what Escape and
//! clearing the box do. A view that skips the drain keeps working, just with a stale frame - which is
//! why the guards live on the type rather than in the view.
//!
//! ## Why the palette's existing debounce is not reused
//!
//! `src/lib/finder-state.ts` has no debounce at all: the shipped palette fires an IPC search per
//! keystroke, and `search_debounce_ms` in `src/lib/settings-types` is a *webview-side* concern that
//! never reaches Rust. So there was no shipped debounce to port. [`OffThread::with_debounce`] is
//! therefore new behaviour, and deliberately so - `rune::Latest` restarts the wait when a newer
//! request arrives, so one keystroke burst costs one scan, and the `request_id` the burst shares is
//! issued on the UI thread where the ordering is actually known.
//!
//! The other half of the honest budget: no view calls [`OffThread`] yet. The Library and Finder
//! elements in [`crate::ui`] are still placeholder trees with no profile behind them, so there is no
//! search box to hang this on until plan milestone 4. What is *not* adjacent to anything is the guard
//! logic itself: `AppState::finder_detection` already runs on the same [`Ledger`] (see
//! `AppState::begin_finder_detection`), the Tauri palette compares the same two numbers in
//! `runSearch`, and the shipped app and these tests consult one copy of the rules, so they cannot
//! drift apart.

use std::sync::Arc;
use std::time::Duration;

use dev_console_core::freshness::{Landing, Ledger, RequestTag, Stale};
use dev_console_core::models::Kind;
use dev_console_core::AppState;
use rune::{AppHandle, Latest, Waker};

/// The `Waker` for a window: [`AppHandle::wake`], callable from any thread.
///
/// This is the documented way a background thread tells a Rune loop there is something to
/// drain - "wake the event loop so `AppSpec::poll` runs, also while the window is hidden". Handing
/// the clone to a worker is the whole point: `AppHandle` is the one thing a background thread is
/// allowed to hold.
///
/// Waking before the loop exists is harmless rather than fatal: `AppHandle::wake` sets a flag on
/// shared state, and with no loop connected yet there is nothing to interrupt - the first `poll`
/// after it starts simply finds the answer already waiting.
/// [`AppHandle::wake`]: rune::AppHandle::wake
pub fn wakeup<S: 'static>(handle: &AppHandle<S>) -> Waker {
    let handle = handle.clone();
    Arc::new(move || handle.wake())
}

/// What came back out of an [`OffThread`]: an answer, or the reason one was thrown away.
#[derive(Debug)]
pub enum Taken<T> {
    /// Still about what the user is looking at. Paint it.
    Applied(T),
    /// An answer landed and was dropped. There is deliberately no value in here: the guard
    /// consumed it, which is what stops a view from painting it "just to be safe".
    Refused(Stale),
}

impl<T> Taken<T> {
    /// The value, if the guards agreed. `None` means the UI keeps what it already shows.
    pub fn value(self) -> Option<T> {
        match self {
            Taken::Applied(value) => Some(value),
            Taken::Refused(_) => None,
        }
    }
}

/// One stream of blocking work for one window: asks go to a Rune worker, answers come back through
/// the ledger's staleness guards.
///
/// A view holds one of these per thing it waits for - the search box, the inspector's load, a
/// project scan - because each has its own idea of "the newest one". `Latest` and [`Ledger`] are
/// deliberately separate: Rune's superseding only knows about *this* stream's requests, while the
/// vault generation and the record revision belong to the app, and the palette's copy in
/// `src/windows/Finder.svelte` compares both today.
pub struct OffThread<T: Send + 'static> {
    /// The channel and the worker pool. Rune's type, used as-is.
    latest: Latest<Landing<T>>,
    /// Which request number is the current one, shared with every answer this stream produces.
    ledger: Arc<Ledger>,
    state: Arc<AppState>,
}

impl<T: Send + 'static> OffThread<T> {
    /// A stream whose answers wake `waker` - normally [`wakeup`]ped from a window's `AppHandle`, and
    /// normally handed to [`AppSpec::setup`](rune::AppSpec#structfield.setup) so the view's `poll`
    /// drains it.
    pub fn new(waker: Waker, state: Arc<AppState>) -> OffThread<T> {
        OffThread {
            latest: Latest::new(waker),
            ledger: Arc::new(Ledger::new()),
            state,
        }
    }

    /// The same, but a request only starts after `wait` with no newer one behind it. This is what a
    /// search box wants: one ask per keystroke burst rather than per keystroke. `Latest` restarts the
    /// wait when a newer request supersedes one that is still waiting, so typing ten characters costs
    /// one run.
    pub fn with_debounce(waker: Waker, state: Arc<AppState>, wait: Duration) -> OffThread<T> {
        OffThread {
            latest: Latest::new(waker).debounce(wait),
            ledger: Arc::new(Ledger::new()),
            state,
        }
    }

    /// Send `work` to a worker thread.
    ///
    /// The tag is captured *inside* the job, not here, because an answer can only be true of the
    /// state the work actually read: between asking and running there may be a lock, an unlock or
    /// somebody else's Save, and a tag minted at ask time would blame the answer for a change that
    /// happened before it started. The request number is the exception - it is decided here, on the
    /// UI thread, because the order the user asked in is the order that decides which answer is
    /// current.
    pub fn request<F>(&self, work: F)
    where
        F: FnOnce(&AppState, RequestTag) -> T + Send + 'static,
    {
        let state = self.state.clone();
        let ledger = self.ledger.clone();
        let request_id = ledger.issue();
        self.latest.request(request_id, move |request_id| {
            let tag = RequestTag::capture(&state, request_id);
            // The work sees the very tag its answer will be judged by, so it can carry or log the
            // identity it answered under rather than reconstruct it.
            let value = work(&state, tag.clone());
            Landing::new(tag, value)
        });
    }

    /// [`OffThread::request`] for work about one entry, so the revision guard has something to
    /// compare. `kind` and `id` are recorded in the tag at the revision they were found at.
    pub fn request_for<F>(&self, kind: Kind, id: &str, work: F)
    where
        F: FnOnce(&AppState, RequestTag) -> T + Send + 'static,
    {
        let state = self.state.clone();
        let ledger = self.ledger.clone();
        let id = id.to_string();
        let request_id = ledger.issue();
        self.latest.request(request_id, move |request_id| {
            let tag = RequestTag::capture_record(&state, request_id, kind, &id);
            let value = work(&state, tag.clone());
            Landing::new(tag, value)
        });
    }

    /// The newest answer that has landed and is still current, once. This is the `poll` hook a view
    /// wants: `None` means either nothing landed or the answer was refused, and in both cases what is
    /// on screen is still right.
    pub fn take(&self) -> Option<T> {
        self.drain().and_then(Taken::value)
    }

    /// [`OffThread::take`] with the reason, for a test or a status line that wants to know whether the
    /// worker was slow or simply wrong.
    pub fn drain(&self) -> Option<Taken<T>> {
        let landing = self.latest.take()?;
        Some(match landing.accept(&self.ledger, &self.state) {
            Ok(value) => Taken::Applied(value),
            Err(stale) => Taken::Refused(stale),
        })
    }

    /// Whether the newest request is still queued or running - a spinner's condition, and the same
    /// one `Latest` uses. Note that a refused answer settles the stream too: the work finished, the
    /// result was thrown away.
    pub fn is_pending(&self) -> bool {
        self.latest.is_pending()
    }

    /// Stop caring about whatever this stream has in flight: the user cleared the box, closed the
    /// window, or the palette was dismissed (`reset()` and `hide()` in the shipped palette both do
    /// exactly this).
    ///
    /// Both halves matter. Bumping the ledger refuses an answer that already landed but has not been
    /// drained yet - which is the common case, since a wakeup only runs `poll` at the loop's next
    /// convenience. Cancelling the stream tells `Latest` that a queued or debounced request should
    /// not even run, which is the difference between throwing away a result and paying a full scan
    /// for a screen that is already gone.
    pub fn abandon(&self) {
        self.ledger.abandon();
        self.latest.cancel();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use dev_console_core::freshness::{Scope, Stale};
    use dev_console_core::models::{EntryDraft, EntryMeta, Kind};
    use dev_console_core::testing::Fixture;
    use dev_console_core::AppState;
    use rune::Waker;

    use super::{wakeup, OffThread, Taken};

    /// How long a test waits for a worker to get somewhere. The work itself is microseconds; this is
    /// scheduler slack, not a guess at how long the work takes.
    const SETTLE: Duration = Duration::from_secs(5);
    /// A debounce long enough that the statements a test runs immediately after `request` are
    /// certainly inside the window, and short enough to stay cheap.
    const DEBOUNCE: Duration = Duration::from_millis(400);

    fn counting_wakes() -> (Waker, Arc<AtomicUsize>) {
        let counter = Arc::new(AtomicUsize::new(0));
        let inner = counter.clone();
        let waker: Waker = Arc::new(move || {
            inner.fetch_add(1, Ordering::SeqCst);
        });
        (waker, counter)
    }

    /// A stream plus the counter its waker increments.
    fn stream<T: Send + 'static>(state: Arc<AppState>) -> (OffThread<T>, Arc<AtomicUsize>) {
        let (waker, wakes) = counting_wakes();
        (OffThread::new(waker, state), wakes)
    }

    fn names(entries: Vec<EntryMeta>) -> Vec<String> {
        entries.into_iter().map(|entry| entry.name).collect()
    }

    /// Wait for an applied answer or fail loudly.
    fn applied<T: Send + 'static>(stream: &OffThread<T>, what: &str) -> T {
        let deadline = Instant::now() + SETTLE;
        loop {
            if let Some(taken) = stream.drain() {
                match taken {
                    Taken::Applied(value) => return value,
                    Taken::Refused(stale) => {
                        panic!("{what}: expected an applied answer, got a refusal: {stale}")
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "{what}: nothing ever landed (still pending: {})",
                stream.is_pending()
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// Wait for any verdict, applied or refused.
    fn verdict<T: Send + 'static>(stream: &OffThread<T>, what: &str) -> Taken<T> {
        let deadline = Instant::now() + SETTLE;
        loop {
            if let Some(taken) = stream.drain() {
                return taken;
            }
            assert!(
                Instant::now() < deadline,
                "{what}: no verdict ever arrived (still pending: {})",
                stream.is_pending()
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// A job that reaches a worker and then stops there until the test lets it go, so a second
    /// request is unambiguously issued *while* the first is running. Both messages come from inside
    /// the job: `running` says "I am on a worker", and the job waits on `go`.
    struct Parked {
        running: SyncSender<()>,
        go: Receiver<()>,
    }

    /// `(job, let_it_go, wait_until_running)`.
    fn parked() -> (Parked, SyncSender<()>, Receiver<()>) {
        let (running_tx, running_rx) = sync_channel::<()>(0);
        let (go_tx, go_rx) = sync_channel::<()>(0);
        (
            Parked {
                running: running_tx,
                go: go_rx,
            },
            go_tx,
            running_rx,
        )
    }

    #[test]
    fn a_blocking_call_runs_on_a_worker_thread_and_wakes_the_loop_once() {
        // Plan §3's first two sentences, checked literally: the work does not run on the thread that
        // asked, asking returns at once, and one landed answer is exactly one wakeup.
        let fixture = Fixture::new("off-thread-once");
        fixture.seed_command("cmd-1", "Shell", "git status", &[]);
        let state = fixture.state.clone();
        let (stream, wakes) = stream::<Vec<String>>(state);

        let caller = thread::current().id();
        let where_it_ran: Arc<Mutex<Option<thread::ThreadId>>> = Arc::new(Mutex::new(None));
        let marker = where_it_ran.clone();

        // The job sleeps longer than the budget asserted below, so "the caller was not blocked" is a
        // measurement rather than a hope.
        let asked_at = Instant::now();
        stream.request(move |state, _tag| {
            *marker.lock().unwrap() = Some(thread::current().id());
            thread::sleep(Duration::from_millis(200));
            names(state.searchable_metadata(None))
        });
        let asked_in = asked_at.elapsed();

        let rows = applied(&stream, "the first answer");
        assert_eq!(rows, vec!["cmd-1"]);
        assert!(
            asked_in < Duration::from_millis(100),
            "asking blocked the calling thread for {asked_in:?}; the whole point is that it must not"
        );
        assert_ne!(
            *where_it_ran.lock().unwrap(),
            Some(caller),
            "the work ran on the thread that asked for it"
        );
        assert_eq!(
            wakes.load(Ordering::SeqCst),
            1,
            "one answer landed, so the loop should have been woken exactly once"
        );
        assert!(!stream.is_pending());
        assert!(
            stream.take().is_none(),
            "an answer is handed to the view once, not every time it asks"
        );
    }

    #[test]
    fn a_superseded_answer_never_lands_and_never_wakes_the_loop() {
        // The coarse half of staleness, which is Rune's: two keystrokes, one answer. The slow one is
        // already running on a worker when the newer one supersedes it, so it may not land, may not
        // wake, and above all may not overwrite the newer rows.
        let fixture = Fixture::new("off-thread-superseded");
        fixture.seed_command("cmd-a", "Shell", "git stash", &[]);
        let (stream, wakes) = stream::<usize>(fixture.state.clone());

        let (job, let_it_go, wait_until_running) = parked();
        stream.request(move |state, _tag| {
            let _ = job.running.send(());
            // Released by the test, or by its `Drop` if the test panics first: either way a worker is
            // never held forever.
            let _ = job.go.recv();
            state.searchable_metadata(None).len()
        });
        wait_until_running
            .recv_timeout(SETTLE)
            .expect("the first request should have reached a worker");

        // The newer keystroke, issued while the first is still running.
        stream.request(|state, _tag| state.searchable_metadata(None).len() + 100);
        assert_eq!(applied(&stream, "the newest answer"), 101);
        assert_eq!(
            wakes.load(Ordering::SeqCst),
            1,
            "only the newest answer may wake the loop"
        );

        // Let the superseded one finish. `Latest::land` drops an answer whose request has been
        // replaced - stores nothing, wakes nobody - so nothing may appear afterwards.
        let_it_go
            .send(())
            .expect("the parked worker should still be listening");
        thread::sleep(Duration::from_millis(150));
        assert!(
            stream.drain().is_none(),
            "a superseded answer must not be handed to the view"
        );
        assert_eq!(
            wakes.load(Ordering::SeqCst),
            1,
            "the stale worker woke the loop a second time"
        );
        assert!(!stream.is_pending(), "both requests are settled");
    }

    #[test]
    fn an_answer_computed_before_a_manual_lock_cannot_restore_secret_rows() {
        // C4's third guard, and the interleaving is performed *by the worker* so the test contains no
        // timing assumption: read the rows while the vault is open, then the user locks it, then the
        // rows come back. The answer is genuine, and it is exactly as dangerous as plan §3 describes.
        let fixture = Fixture::new("off-thread-post-lock");
        fixture.seed_command("cmd-1", "Shell", "git status", &[]);
        fixture.seed_secret("deploy-token", "synthetic-value-not-a-real-one");
        let state = fixture.state.clone();
        let (stream, wakes) = stream::<Vec<String>>(state.clone());
        assert!(state.vault.is_unlocked(), "the fixture starts unlocked");
        let before = state.vault.generation();

        stream.request(move |state, tag| {
            let rows = names(state.searchable_metadata(None));
            assert!(
                rows.iter().any(|name| name == "deploy-token"),
                "the seeded secret should be readable while unlocked: {rows:?}"
            );
            assert_eq!(tag.vault_generation, before, "tagged while open");
            // The user picks "Lock vault" - from the tray, say - while this answer is on its way.
            state.vault.lock().expect("manual lock");
            rows
        });

        match verdict(&stream, "the pre-lock answer") {
            Taken::Applied(rows) => {
                panic!("a pre-lock list of secrets was applied to a locked vault: {rows:?}")
            }
            Taken::Refused(stale) => assert!(
                matches!(stale, Stale::VaultGeneration { .. }),
                "the honest reason is the vault, not the request: {stale}"
            ),
        }
        // The loop *was* woken, and correctly did nothing with it: `poll` ran, the guards refused, and
        // the screen kept showing a locked vault. Waking is not drawing, and this is where the
        // difference is supposed to cost nothing.
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert!(!state.vault.is_unlocked(), "the vault is still locked");
        assert!(state.vault.generation() > before);

        // And the search that runs *after* the lock is applied normally: the guard refuses stale
        // answers, not answers.
        stream.request(|state, _tag| names(state.searchable_metadata(None)));
        assert_eq!(
            applied(&stream, "the post-lock search"),
            vec!["cmd-1"],
            "a locked vault contributes commands only"
        );
    }

    #[test]
    fn a_record_saved_while_it_was_being_loaded_is_not_painted() {
        // C4's second guard off the UI thread. Saving a command leaves the vault generation alone, so
        // the revision is the only thing that can catch this one - and it does: the inspector loads
        // `git push` at revision 1 while somebody else saves revision 2.
        let fixture = Fixture::new("off-thread-revision");
        fixture.seed_command("cmd-edit", "Shell", "git push", &[]);
        let state = fixture.state.clone();
        let (stream, wakes) = stream::<String>(state.clone());

        stream.request_for(Kind::Command, "cmd-edit", move |state, tag| {
            let (record, _, _) = state
                .repo
                .get_command("cmd-edit")
                .expect("read")
                .expect("seeded");
            match &tag.scope {
                Scope::Record {
                    id,
                    kind,
                    revision: Some(1),
                } => {
                    assert_eq!(id, "cmd-edit");
                    assert_eq!(*kind, Kind::Command);
                }
                other => panic!("the tag should carry the revision the work read: {other:?}"),
            }

            // Another window saves the same command while this load is in flight.
            let mut draft = EntryDraft {
                id: Some("cmd-edit".into()),
                expected_revision: 1,
                name: "cmd-edit".into(),
                entry_type: "Shell".into(),
                command: "git push --force-with-lease".into(),
                ..Default::default()
            };
            let saved = state.repo.save_command(&mut draft).expect("the other save");
            assert_eq!(saved.revision, 2);
            record.command
        });

        match verdict(&stream, "the load of a moved-on record") {
            Taken::Applied(command) => {
                panic!("a record that moved on was painted into the editor: {command}")
            }
            Taken::Refused(stale) => assert_eq!(
                Some(Stale::RevisionMoved {
                    id: "cmd-edit".into(),
                    then: 1,
                    now: 2
                }),
                Some(stale.clone()),
                "{stale}"
            ),
        }
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        // Refusing wrote nothing: the other window's save is still the truth.
        assert_eq!(
            state
                .repo
                .get_command("cmd-edit")
                .expect("read")
                .expect("present")
                .0
                .command,
            "git push --force-with-lease"
        );

        // A fresh load of the same record, which sees revision 2, is applied: the guard is about
        // moving on, not about refusing to load anything.
        stream.request_for(Kind::Command, "cmd-edit", |state, tag| {
            assert!(
                matches!(
                    &tag.scope,
                    Scope::Record {
                        revision: Some(2),
                        ..
                    }
                ),
                "{:?}",
                tag.scope
            );
            state
                .repo
                .get_command("cmd-edit")
                .expect("read")
                .expect("present")
                .0
                .command
        });
        assert_eq!(
            applied(&stream, "the reload after the conflict"),
            "git push --force-with-lease"
        );
    }

    #[test]
    fn abandoning_a_stream_refuses_work_that_already_landed_and_stops_work_that_has_not() {
        // `reset()` and `hide()` in the shipped palette both do `requestId += 1`. Here the box is
        // cleared while a debounced request is still waiting, so two things must both hold: the
        // queued work never runs at all, and the view is handed nothing.
        let fixture = Fixture::new("off-thread-abandon");
        fixture.seed_command("cmd-1", "Shell", "git status", &[]);
        let (waker, wakes) = counting_wakes();
        let stream: OffThread<Vec<String>> =
            OffThread::with_debounce(waker, fixture.state.clone(), DEBOUNCE);

        let ran = Arc::new(AtomicUsize::new(0));
        let marker = ran.clone();
        stream.request(move |state, _tag| {
            marker.fetch_add(1, Ordering::SeqCst);
            names(state.searchable_metadata(None))
        });
        assert!(
            stream.is_pending(),
            "the request is queued behind its debounce"
        );

        stream.abandon();

        // Longer than the debounce, so "nothing happened" can only mean "it was cancelled".
        thread::sleep(DEBOUNCE + Duration::from_millis(250));
        assert_eq!(
            ran.load(Ordering::SeqCst),
            0,
            "abandoning the stream should have stopped work that had not started"
        );
        assert!(
            stream.drain().is_none(),
            "and it should hand the view nothing"
        );
        assert_eq!(
            wakes.load(Ordering::SeqCst),
            0,
            "nothing landed, so nothing woke"
        );
        assert!(!stream.is_pending(), "the stream is settled");

        // Asking again after the clear works normally: abandoning is not a permanent state.
        stream.request(|state, _tag| names(state.searchable_metadata(None)));
        assert_eq!(
            applied(&stream, "the search after the clear"),
            vec!["cmd-1"]
        );
    }

    #[test]
    fn a_debounced_request_is_tagged_with_the_generation_it_ran_under() {
        // The tag is captured on the worker rather than when the request is made, and this pins it:
        // the vault is locked inside the debounce window, before the work has started, so the answer
        // it produces is a *current* one and must be applied. A tag minted at ask time would already
        // have been behind the live generation, and this perfectly good answer would have been thrown
        // away - the failure mode a "fix" that moves `capture` to the caller would introduce.
        let fixture = Fixture::new("off-thread-capture");
        fixture.seed_command("cmd-1", "Shell", "git status", &[]);
        fixture.seed_secret("deploy-token", "synthetic-value-not-a-real-one");
        let state = fixture.state.clone();
        let (waker, _wakes) = counting_wakes();
        let stream: OffThread<(u64, Vec<String>)> =
            OffThread::with_debounce(waker, state.clone(), DEBOUNCE);

        let asked_at = state.vault.generation();
        assert!(state.vault.is_unlocked());
        stream.request(|state, tag| (tag.vault_generation, names(state.searchable_metadata(None))));

        // Still inside the debounce window: the vault closes before the work starts.
        state.vault.lock().expect("lock while debouncing");
        let locked_at = state.vault.generation();
        assert!(
            locked_at > asked_at,
            "the lock should have moved the generation: {asked_at} -> {locked_at}"
        );

        let (tagged_at, rows) = applied(&stream, "the answer computed after the lock");
        assert_eq!(
            tagged_at, locked_at,
            "the tag must describe the store the work read, not the one that existed when it was asked"
        );
        assert_eq!(
            rows,
            vec!["cmd-1"],
            "and it read a locked vault, so no secret row is in it"
        );
    }

    #[test]
    fn a_wake_for_a_window_that_has_not_started_is_harmless() {
        // The adapter's whole job is `AppHandle::wake`, and the case a shell cannot rule out is a
        // worker finishing while no loop is connected. Whether a wake then produced a *frame* is the
        // part this host cannot see at all (see the module docs): that is the owner's desktop check,
        // and nothing here pretends to have settled it.
        let app = rune::App::new(crate::app::library_spec());
        let handle = app.handle();
        let waker = wakeup(&handle);
        waker();
        waker();
        // Two wakes with no event loop connected: the only assertion is that it survived, which is
        // exactly what a worker finishing after its window went away needs to be able to do.
        drop(waker);
        drop(handle);
    }
}
