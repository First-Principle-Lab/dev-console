//! What the two windows share: the core services, the window handles, and the route an effect
//! takes to reach them.
//!
//! The Tauri runtime did this in `src-tauri/src/effects.rs` by emitting string events to webviews.
//! Here a service call returns typed [`Effects`], [`Hub::dispatch`] hands each [`AppEvent`] to both
//! windows' state through `AppHandle::update`, and performs each [`WindowCommand`] on the handles.
//! Blocking work runs on a thread through [`Exec`], whose answer lands on the UI thread the same way.

use std::sync::{Arc, OnceLock};

use dev_console_core::services::{AppEvent, Effects, FinderActions, LibraryActions, WindowCommand};
use dev_console_core::state::AppState;
use rune::AppHandle;

use crate::finder::Finder;
use crate::library::Library;

/// The resident process's services and windows. Cheap to clone: everything is shared.
#[derive(Clone)]
pub struct Hub {
    pub state: Arc<AppState>,
    pub library: Arc<LibraryActions>,
    pub finder: Arc<FinderActions>,
    windows: Arc<OnceLock<Windows>>,
    pub restart_requested: Arc<std::sync::atomic::AtomicBool>,
}

/// The two windows' handles, set once both exist.
#[derive(Clone)]
pub struct Windows {
    pub library: AppHandle<Library>,
    pub finder: AppHandle<Finder>,
    /// Refreshes the tray's lock item after a vault change.
    pub tray: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Hub {
    pub fn new(state: Arc<AppState>, version: &str) -> Hub {
        Hub {
            library: Arc::new(LibraryActions::new(state.clone(), version)),
            finder: Arc::new(FinderActions::new(state.clone(), version)),
            state,
            windows: Arc::new(OnceLock::new()),
            restart_requested: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Connects the windows. Effects dispatched before this are dropped, which only happens
    /// during startup, before either window can have asked for anything.
    pub fn connect(&self, windows: Windows) {
        let _ = self.windows.set(windows);
    }

    pub fn windows(&self) -> Option<&Windows> {
        self.windows.get()
    }

    /// Delivers what a service call asked for, in the order it asked: events first, so a window
    /// that is about to be shown has already heard what changed.
    pub fn dispatch(&self, effects: Effects) {
        for event in effects.events {
            self.deliver(event);
        }
        for command in effects.windows {
            self.perform(command);
        }
        if effects.tray_refresh {
            if let Some(refresh) = self.windows().and_then(|windows| windows.tray.clone()) {
                refresh();
            }
        }
    }

    /// Hands one event to both windows. Each decides whether it cares.
    pub fn deliver(&self, event: AppEvent) {
        let Some(windows) = self.windows() else {
            return;
        };
        let for_finder = event.clone();
        windows
            .library
            .update(move |library, tx| library.on_event(&event, tx));
        windows
            .finder
            .update(move |finder, tx| finder.on_event(&for_finder, tx));
    }

    /// Shows the Finder: works out which project the focused terminal is in, then presents the
    /// palette, which clears its query and searches again.
    ///
    /// The focused window is read here, before the palette takes focus; asking afterwards would
    /// only ever name the palette. The `/proc` walk runs on its own thread and its answer is
    /// applied only if no later open has superseded it.
    pub fn show_finder(&self, token: Option<String>) {
        let Some(windows) = self.windows().cloned() else {
            return;
        };
        let enabled = self.state.settings.get().project_context_enabled;
        let ticket = self.state.begin_finder_detection();
        self.deliver(AppEvent::FinderContext(None));
        if enabled {
            let focused = dev_console_core::context::focused_now();
            let hub = self.clone();
            let spawned = std::thread::Builder::new()
                .name("finder-detection".into())
                .spawn(move || {
                    let context = focused
                        .or_else(dev_console_core::context::focused_via_hyprctl)
                        .and_then(|window| dev_console_core::context::resolve(&window));
                    if hub.state.finish_finder_detection(ticket, context.clone()) {
                        hub.deliver(AppEvent::FinderContext(context));
                    }
                });
            if spawned.is_err() {
                eprintln!("dev-console: project detection could not start");
            }
        }
        match token {
            Some(token) => windows.finder.activate(token),
            None => windows.finder.open(),
        }
        self.deliver(AppEvent::FinderShown);
    }

    /// Super+F: a visible Finder hides, anything else shows it.
    pub fn toggle_finder(&self, token: Option<String>) {
        match self.windows() {
            Some(windows) if windows.finder.is_visible() => windows.finder.hide(),
            Some(_) => self.show_finder(token),
            None => {}
        }
    }

    pub fn perform(&self, command: WindowCommand) {
        let Some(windows) = self.windows() else {
            return;
        };
        match command {
            WindowCommand::ShowLibrary { kind, entry_id } => {
                windows
                    .library
                    .update(move |library, tx| library.show_intent(kind, entry_id, tx));
                windows.library.open();
            }
            WindowCommand::Hide { surface } => {
                if surface.is_library() {
                    windows.library.hide();
                } else {
                    windows.finder.hide();
                }
            }
            WindowCommand::Quit => {
                self.state
                    .quitting
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                windows.library.update(|library, _| {
                    if !library.request_close() {
                        if let Some(windows) = library.hub().windows() {
                            windows.library.open();
                        }
                    }
                });
            }
            WindowCommand::FinishQuit => {
                self.state.vault.clear_decrypted();
                self.state.clipboard.clear_owned();
                windows.finder.update(|finder, _| finder.dismiss());
                windows.finder.hide();
                windows
                    .library
                    .update(|library, _| library.clear_sensitive());
                windows.library.quit();
            }
            WindowCommand::CancelQuit => {
                self.state
                    .quitting
                    .store(false, std::sync::atomic::Ordering::SeqCst);
            }
            WindowCommand::Restart => {
                self.restart_requested
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                self.state.vault.clear_decrypted();
                self.state.clipboard.clear_owned();
                windows
                    .library
                    .update(|library, _| library.clear_sensitive());
                windows.library.quit();
            }
        }
    }
}

/// Where a window's blocking work runs. `Thread` is the real app: a worker thread, then
/// `AppHandle::update` back onto the UI thread. `Inline` runs both halves on the spot, which is
/// what the scripted tests use so a press and its answer land in the same step.
pub enum Exec<S: 'static> {
    Thread(AppHandle<S>),
    Inline,
    #[cfg(test)]
    Deferred(TestQueue<S>),
}

impl<S: 'static> Clone for Exec<S> {
    fn clone(&self) -> Self {
        match self {
            Exec::Thread(handle) => Exec::Thread(handle.clone()),
            Exec::Inline => Exec::Inline,
            #[cfg(test)]
            Exec::Deferred(queue) => Exec::Deferred(queue.clone()),
        }
    }
}

impl<S: 'static> Exec<S> {
    /// Runs `work` off the UI thread and `apply` back on it. With `Inline`, `apply` receives the
    /// state directly through `now`.
    pub fn run<T: Send + 'static>(
        &self,
        now: &mut S,
        work: impl FnOnce() -> T + Send + 'static,
        apply: impl FnOnce(&mut S, T) + Send + 'static,
    ) {
        match self {
            Exec::Thread(handle) => {
                let handle = handle.clone();
                let spawned = std::thread::Builder::new()
                    .name("dev-console-work".into())
                    .spawn(move || {
                        let out = work();
                        handle.update(move |state, _| apply(state, out));
                    });
                if let Err(error) = spawned {
                    eprintln!("dev-console: could not start a worker thread: {error}");
                }
            }
            Exec::Inline => {
                let out = work();
                apply(now, out);
            }
            #[cfg(test)]
            Exec::Deferred(queue) => {
                let out = work();
                queue
                    .0
                    .lock()
                    .unwrap()
                    .push_back(Box::new(move |state| apply(state, out)));
            }
        }
    }
}

/// An error as a sentence a status line can show.
pub fn message(error: &dev_console_core::error::AppError) -> String {
    error.to_string()
}

/// Runs service work now but holds UI delivery until a test releases it.
#[cfg(test)]
type Pending<S> = Box<dyn FnOnce(&mut S) + Send>;

#[cfg(test)]
#[doc(hidden)]
pub struct TestQueue<S>(std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<Pending<S>>>>);

#[cfg(test)]
impl<S> Clone for TestQueue<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

#[cfg(test)]
impl<S> Default for TestQueue<S> {
    fn default() -> Self {
        Self(Default::default())
    }
}

#[cfg(test)]
impl<S> TestQueue<S> {
    pub(crate) fn flush(&self, state: &mut S) {
        loop {
            let next = self.0.lock().unwrap().pop_front();
            match next {
                Some(apply) => apply(state),
                None => break,
            }
        }
    }
}
