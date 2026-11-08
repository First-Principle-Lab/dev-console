//! Finder selection and completion rules.
//!
//! Ported from `src/lib/finder-state.ts`, the fourth of the TypeScript-only modules plan §3 names.
//! These are the rules that decide which row is highlighted while the query
//! is typed, what the muted completion next to the caret shows, and what Tab puts in the field -
//! i.e. the palette's keyboard behaviour, which plan §2 says the Rune view has to preserve.
//!
//! The rules are pure functions over the ranked results the backend already produced
//! ([`crate::search::rank_in_context`]), which is the point of the port: selection identity is the
//! entry id, never the row index or the displayed name, so a re-rank arriving from a slower search
//! cannot move the highlight out from under a keystroke.
//!
//! * [`selected_index`] / [`next_selection`] / [`selected`] - arrow keys and Enter. The id is
//!   echoed back, so the caller can hold the id across a re-rank.
//! * [`displayed_query`] - the muted completion, and it is the *same* rule as
//!   [`crate::search::completion_for`] (the backend already computes a completion string for every
//!   result), so this delegates instead of restating it. One rule, two callers.
//! * [`tab_completion`] - Tab accepts a prefix completion, and for a fuzzy match it replaces the
//!   query with the whole name. It has to differ from the muted completion here: showing a fuzzy
//!   match's name in the field is the only way the caret stays meaningful.
//! * [`should_commit`] - Enter/Space while an IME is composing must not commit. The webview read
//!   `isComposing` and the phantom keyCode 229; a native key handler needs the same guard, so it
//!   lives here where it can be tested without a text input.
//!
//! The tests are the ported `tests/unit/finder-state.test.ts` cases, named after them.
//!
//! Not ported: the `FinderState` interface in the same TypeScript file. It was four fields the
//! Svelte component held; the Rune view keeps its own state (plan §2), and these functions are the
//! behaviour it needs from here.

use crate::models::SearchResult;
use crate::search::completion_for;

/// Where the highlight is, by id. `None` when the id is not in `results` - which is the normal
/// state right after a re-rank, and why the callers go through [`next_selection`] rather than
/// indexing a row they remembered.
pub fn selected_index(results: &[SearchResult], selected_id: Option<&str>) -> Option<usize> {
    let selected_id = selected_id?;
    results.iter().position(|row| row.id == selected_id)
}

/// The id to highlight after moving by `delta` rows (-1 up, +1 down, 0 to settle).
///
/// Always leaves a valid selection when there are results, and keeps the same id when that id is
/// still present, which is what stops the highlight jumping to the top on every keystroke. With no
/// valid current row, `delta >= 0` lands on the first row and a negative one on the last, so the
/// very first Down and the very first Up both do something sensible. Movement clamps at both ends;
/// the palette has never wrapped.
pub fn next_selection<'results>(
    results: &'results [SearchResult],
    selected_id: Option<&str>,
    delta: i64,
) -> Option<&'results str> {
    if results.is_empty() {
        return None;
    }
    let last = results.len() as i64 - 1;
    let index = match selected_index(results, selected_id) {
        None => {
            if delta >= 0 {
                0
            } else {
                last
            }
        }
        Some(current) => (current as i64 + delta).clamp(0, last),
    };
    Some(&results[index as usize].id)
}

/// The highlighted row, or `None` when the id is gone (a delete, or a re-rank that dropped it).
pub fn selected<'rows>(
    results: &'rows [SearchResult],
    selected_id: Option<&str>,
) -> Option<&'rows SearchResult> {
    let selected_id = selected_id?;
    results.iter().find(|row| row.id == selected_id)
}

/// The text the input shows while a row is highlighted: the query the user typed plus a muted
/// completion. `None` means "show no completion".
///
/// Only a real prefix earns one; for a fuzzy match nothing is appended, so the caret stays where
/// the user left it. Delegates to [`completion_for`], the backend's single prefix rule, which the
/// ranked results already carry in [`SearchResult::completion`] - a second copy of the rule here
/// is how the muted text and the Tab result would start to disagree.
pub fn displayed_query(query: &str, entry: Option<&SearchResult>) -> Option<String> {
    let entry = entry?;
    completion_for(query, &entry.name)
}

/// What Tab puts in the query field. `None` when there is nothing to do (the field already holds
/// exactly this name).
///
/// A prefix match completes to the name; a fuzzy match *replaces* the query with the name, which
/// is the current behaviour and the reason this is not simply [`displayed_query`].
pub fn tab_completion(query: &str, entry: Option<&SearchResult>) -> Option<String> {
    let entry = entry?;
    if entry.name == query {
        return None;
    }
    Some(entry.name.clone())
}

/// True when a key event may commit the selection.
///
/// Keys delivered as part of IME composition must not commit, including the phantom keyCode 229
/// that reports composition without saying which key it was. Without this guard, typing a
/// multi-byte character with a space in the composition path copies the row and closes the
/// palette - the bug the webview's `isComposing` check existed for.
pub fn should_commit(is_composing: bool, key_code: u32) -> bool {
    !is_composing && key_code != 229
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Kind;

    /// The fixture rows from `finder-state.test.ts`: one exact prefix, one with the same prefix
    /// plus more words, and one that only fuzzy-matches `rvr`.
    fn row(id: &str, name: &str) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            kind: Kind::Command,
            name: name.to_string(),
            entry_type: "Git".to_string(),
            tags: Vec::new(),
            description: String::new(),
            completion: None,
            pinned: false,
            usage_count: 0,
            last_used_at: None,
            in_context: false,
        }
    }

    fn rows() -> Vec<SearchResult> {
        vec![
            row("1", "docker"),
            row("2", "docker compose up"),
            row("3", "git revert HEAD"),
        ]
    }

    /// `finder selection > selects the first result automatically`
    #[test]
    fn selects_the_first_result_automatically() {
        let rows = rows();
        assert_eq!(next_selection(&rows, None, 0), Some("1"));
        // No current row: the first Up still lands somewhere valid, on the last row.
        assert_eq!(next_selection(&rows, None, -1), Some("3"));
        assert_eq!(selected_index(&rows, Some("2")), Some(1));
        assert_eq!(selected_index(&rows, Some("gone")), None);
        assert_eq!(selected_index(&rows, None), None);
    }

    /// `finder selection > moves and clamps without wrapping past the ends`
    #[test]
    fn moves_and_clamps_without_wrapping_past_the_ends() {
        let rows = rows();
        assert_eq!(next_selection(&rows, Some("1"), 1), Some("2"));
        assert_eq!(next_selection(&rows, Some("2"), 1), Some("3"));
        assert_eq!(next_selection(&rows, Some("3"), 1), Some("3"));
        assert_eq!(next_selection(&rows, Some("3"), -1), Some("2"));
        assert_eq!(next_selection(&rows, Some("1"), -1), Some("1"));
        // A remembered row that is gone re-enters at the end appropriate to the direction, and a
        // large delta clamps instead of wrapping.
        assert_eq!(next_selection(&rows, Some("gone"), 1), Some("1"));
        assert_eq!(next_selection(&rows, Some("1"), 99), Some("3"));
        assert_eq!(next_selection(&rows, Some("3"), -99), Some("1"));
    }

    /// `finder selection > keeps the stable id when the list is replaced with a matching row`
    #[test]
    fn keeps_the_stable_id_when_the_list_is_replaced_with_a_matching_row() {
        let rows = rows();
        assert_eq!(next_selection(&rows, Some("2"), 0), Some("2"));
        // `selected` hands back the row itself, so the view can render it without a second lookup
        // that could disagree with the highlight.
        assert_eq!(
            selected(&rows, Some("2")).map(|row| row.name.as_str()),
            Some("docker compose up")
        );
    }

    /// `finder selection > has no selection when there are no results`
    #[test]
    fn has_no_selection_when_there_are_no_results() {
        let empty: Vec<SearchResult> = Vec::new();
        assert_eq!(next_selection(&empty, Some("1"), 1), None);
        assert_eq!(next_selection(&empty, Some("1"), -1), None);
        assert_eq!(selected(&empty, Some("1")), None);
        assert_eq!(selected_index(&empty, Some("1")), None);
        // A remembered id with nothing behind it is not a selection either.
        let rows = rows();
        assert_eq!(selected(&rows, None), None);
        assert_eq!(selected(&rows, Some("nope")), None);
    }

    /// `finder completion > shows a muted completion only for a real name prefix`
    #[test]
    fn shows_a_muted_completion_only_for_a_real_name_prefix() {
        let rows = rows();
        assert_eq!(
            displayed_query("doc", selected(&rows, Some("2"))).as_deref(),
            Some("docker compose up")
        );
        assert_eq!(displayed_query("compose", selected(&rows, Some("2"))), None);
        assert_eq!(displayed_query("", selected(&rows, Some("1"))), None);
        assert_eq!(displayed_query("doc", None), None);
        // A row whose name is already the query has nothing left to suggest.
        assert_eq!(displayed_query("docker", selected(&rows, Some("1"))), None);
    }

    /// `finder completion > Tab accepts a prefix completion and replaces the query for a fuzzy
    /// match`
    #[test]
    fn tab_accepts_a_prefix_completion_and_replaces_the_query_for_a_fuzzy_match() {
        let rows = rows();
        assert_eq!(
            tab_completion("doc", selected(&rows, Some("2"))).as_deref(),
            Some("docker compose up")
        );
        assert_eq!(
            tab_completion("rvr", Some(&row("9", "git revert HEAD"))).as_deref(),
            Some("git revert HEAD")
        );
        assert_eq!(tab_completion("docker", selected(&rows, Some("1"))), None);
        assert_eq!(tab_completion("doc", None), None);
    }

    /// `finder completion > never lets completion change the selection identity`
    #[test]
    fn never_lets_completion_change_the_selection_identity() {
        let rows = rows();
        let completed = tab_completion("doc", selected(&rows, Some("2"))).unwrap();
        assert_eq!(completed, "docker compose up");
        // Accepting a completion changes the text, never the row the highlight is on: the id the
        // user is on is still the id the completion names.
        assert_eq!(
            selected(&rows, Some("2")).map(|row| row.name.as_str()),
            Some(completed.as_str())
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.name == completed)
                .map(|row| row.id.as_str()),
            Some("2")
        );
    }

    /// `IME handling > ignores keys delivered during composition`
    #[test]
    fn ignores_keys_delivered_during_composition() {
        assert!(!should_commit(true, 13));
        assert!(!should_commit(false, 229));
        assert!(should_commit(false, 13));
        // Space (32) is the other committing key in the palette, and it commits normally.
        assert!(should_commit(false, 32));
    }
}
