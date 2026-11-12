//! Persisted Library shortcuts. Navigation and text editing keep their standard keys.
use std::collections::{BTreeMap, BTreeSet};

use crate::error::{AppError, AppResult};

pub const ACTIONS: [(&str, &str, &str); 7] = [
    ("search", "Search", "ctrl+k"),
    ("newEntry", "New entry", "ctrl+n"),
    ("editEntry", "Edit the selected entry", "ctrl+e"),
    ("saveEditor", "Save the editor", "ctrl+s"),
    ("commands", "Commands", "ctrl+1"),
    ("secrets", "Secrets", "ctrl+2"),
    ("cliProxy", "CLI Proxy", "ctrl+3"),
];

pub fn defaults() -> BTreeMap<String, String> {
    ACTIONS
        .iter()
        .map(|(id, _, key)| (id.to_string(), key.to_string()))
        .collect()
}

/// Accept Ctrl/Alt with optional Shift and an ASCII letter or digit. Requiring a
/// modifier leaves typing, navigation, Escape and the desktop's Super keys alone.
pub fn normalize(value: &str) -> AppResult<String> {
    let value = value.trim().to_ascii_lowercase();
    let parts: Vec<_> = value.split('+').map(str::trim).collect();
    let invalid = || {
        AppError::Validation(
            "Use Ctrl or Alt, optional Shift, and a letter or digit (for example Ctrl+Shift+K)."
                .into(),
        )
    };
    let Some((key, modifiers)) = parts.split_last() else {
        return Err(invalid());
    };
    if key.len() != 1 || !key.as_bytes()[0].is_ascii_alphanumeric() {
        return Err(invalid());
    }
    let mut seen = BTreeSet::new();
    for modifier in modifiers {
        if !matches!(*modifier, "ctrl" | "alt" | "shift") || !seen.insert(*modifier) {
            return Err(invalid());
        }
    }
    if !seen.contains("ctrl") && !seen.contains("alt") {
        return Err(invalid());
    }
    let mut result: Vec<&str> = ["ctrl", "alt", "shift"]
        .into_iter()
        .filter(|m| seen.contains(m))
        .collect();
    result.push(key);
    Ok(result.join("+"))
}

/// Merge missing actions for old settings, normalize spellings and reject conflicts.
pub fn validated(bindings: &BTreeMap<String, String>) -> AppResult<BTreeMap<String, String>> {
    let mut result = defaults();
    let mut used = BTreeMap::new();
    for (id, label, default) in ACTIONS {
        let key = normalize(bindings.get(id).map_or(default, String::as_str))?;
        if let Some(previous) = used.insert(key.clone(), label) {
            return Err(AppError::Validation(format!(
                "{label} and {previous} use the same shortcut. Choose different keys."
            )));
        }
        result.insert(id.into(), key);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_partial_settings_keep_search_on_ctrl_k() {
        assert_eq!(validated(&BTreeMap::new()).unwrap()["search"], "ctrl+k");
        assert_eq!(normalize(" Shift + CTRL + K ").unwrap(), "ctrl+shift+k");
    }
    #[test]
    fn invalid_and_duplicate_bindings_are_rejected() {
        for key in [
            "k",
            "shift+k",
            "ctrl+ctrl+k",
            "super+k",
            "ctrl+escape",
            "ctrl+",
            "ctrl+ab",
        ] {
            assert!(normalize(key).is_err(), "{key}");
        }
        let mut bindings = defaults();
        bindings.insert("search".into(), "CTRL+N".into());
        assert!(validated(&bindings).is_err());
    }
}
