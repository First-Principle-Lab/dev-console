//! Settings, the terminal list and shell-history suggestions (plan section 3).
//!
//! Part of [`LibraryActions`]. The settings panel belongs to the library window: the palette has no
//! path to any of this, and `open_settings` - the tray's route into the panel - is addressed to the
//! library alone. Saving settings announces the theme before the result is handed back, so a palette
//! in the other window hears about the theme change too.

use crate::error::AppResult;
use crate::execution;
use crate::history;
use crate::models::Settings;

use crate::services::LibraryActions;
use crate::services::{AppEvent, Effects};

impl LibraryActions {
    // -----------------------------------------------------------------------------------------
    // Settings, terminals and history suggestions
    // -----------------------------------------------------------------------------------------

    pub fn get_settings(&self) -> Settings {
        self.state.settings.get()
    }

    /// Saving settings changes the theme for every window, so the theme goes out before the result is
    /// handed back: the library repaints from the event, not from its own return value, which is how
    /// a palette in the other window hears about it too.
    pub fn save_settings(&self, settings: Settings, effects: &mut Effects) -> AppResult<Settings> {
        let saved = self.state.settings.save(settings)?;
        effects.emit(AppEvent::ThemeChanged(saved.theme.clone()));
        Ok(saved)
    }

    /// The tray's Settings item. A window-targeted event: the palette has no settings panel.
    pub fn open_settings(&self, effects: &mut Effects) {
        effects.emit(AppEvent::OpenSettings);
    }

    pub fn list_terminals(&self) -> Vec<execution::TerminalOption> {
        execution::available_terminals()
    }

    /// Shell-history suggestions, clamped to what the panel can show. Off, the history files are
    /// never opened at all.
    pub fn history_suggestions(&self, limit: Option<usize>) -> AppResult<Vec<history::Suggestion>> {
        if !self.state.settings.get().history_suggestions_enabled {
            return Ok(Vec::new());
        }
        let limit = limit
            .unwrap_or(history::DEFAULT_LIMIT)
            .clamp(1, history::MAX_LIMIT);
        self.state.history_suggestions(limit)
    }

    /// Only the digest of a dismissed suggestion is stored, so dismissing cannot put a history line
    /// into the database either.
    pub fn dismiss_suggestion(&self, key: String) -> AppResult<()> {
        self.state.repo.dismiss_suggestion(&key)
    }

    pub fn clear_dismissed_suggestions(&self) -> AppResult<usize> {
        self.state.repo.clear_dismissed_suggestions()
    }

    pub fn dismissed_suggestion_count(&self) -> AppResult<usize> {
        Ok(self.state.repo.dismissed_suggestion_keys()?.len())
    }

    pub fn install_history_tracking(&self) -> AppResult<String> {
        crate::project_history::install_zsh_hook()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::Delivery;
    use crate::testing::Fixture;

    #[test]
    fn saving_settings_announces_the_theme_before_handing_the_result_back() {
        let fixture = Fixture::new("settings");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        let settings = Settings {
            theme: "omarchy-ristretto".into(),
            ..actions.get_settings()
        };
        let mut effects = Effects::default();
        let saved = actions.save_settings(settings, &mut effects).expect("save");
        assert_eq!(saved.theme, "omarchy-ristretto");
        assert_eq!(actions.get_theme(), "omarchy-ristretto");
        assert_eq!(
            effects.events.len(),
            1,
            "saving settings announces one thing"
        );
        match &effects.events[0] {
            AppEvent::ThemeChanged(theme) => assert_eq!(theme, "omarchy-ristretto"),
            other => panic!("expected a theme-changed event, got {other:?}"),
        }
        assert_eq!(effects.events[0].delivery(), Delivery::All);
    }

    #[test]
    fn the_tray_settings_action_addresses_the_library_only() {
        let fixture = Fixture::new("open-settings");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        let mut effects = Effects::default();
        actions.open_settings(&mut effects);
        assert_eq!(effects.events.len(), 1);
        assert_eq!(effects.events[0].name(), "open-settings");
        assert_eq!(
            effects.events[0].delivery(),
            Delivery::Library,
            "the palette has no settings panel to open"
        );
    }

    #[test]
    fn history_suggestions_are_off_by_default_and_clamped_when_on() {
        let fixture = Fixture::new("suggestions");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        let settings = Settings {
            history_suggestions_enabled: false,
            ..actions.get_settings()
        };
        actions
            .save_settings(settings, &mut Effects::default())
            .expect("save");
        assert!(actions
            .history_suggestions(None)
            .expect("off means an empty list, not an error")
            .is_empty());
        // Dismissals are digests, so the count starts at zero for a fresh profile.
        assert_eq!(actions.dismissed_suggestion_count().expect("count"), 0);
        assert_eq!(actions.clear_dismissed_suggestions().expect("clear"), 0);
    }
}
