use crate::models::{EntryMeta, SearchResult};

pub const DEFAULT_LIMIT: usize = 50;

/// A search term plus the field it is restricted to (`type:`, `tag:`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub field: Option<Field>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Type,
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Exact = 0,
    Prefix = 1,
    TokenPrefix = 2,
    Contains = 3,
    Field = 4,
    Fuzzy = 5,
}

const MAX_TIER: Tier = Tier::Fuzzy;

pub fn parse_query(query: &str) -> Vec<Term> {
    let mut terms = Vec::new();
    for token in query.split_whitespace() {
        let lower = token.to_lowercase();
        if let Some(value) = lower.strip_prefix("type:") {
            if !value.is_empty() {
                terms.push(Term {
                    field: Some(Field::Type),
                    text: value.to_string(),
                });
            }
        } else if let Some(value) = lower.strip_prefix("tag:") {
            if !value.is_empty() {
                terms.push(Term {
                    field: Some(Field::Tag),
                    text: value.to_string(),
                });
            }
        } else if !lower.is_empty() {
            terms.push(Term {
                field: None,
                text: lower,
            });
        }
    }
    terms
}

fn fuzzy(haystack: &str, needle: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|c| chars.any(|h| h == c))
}

fn token_prefix(haystack: &str, needle: &str) -> bool {
    haystack
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| token.starts_with(needle))
}

/// Pre-lowercased search keys for one entry. A ranking pass builds one of these per entry
/// once, so neither term matching nor the sort comparator re-lowercases name, type,
/// description and tags (the comparator alone was O(n log n) allocations per search).
pub struct SearchKey<'a> {
    entry: &'a EntryMeta,
    name: String,
    entry_type: String,
    description: String,
    tags: Vec<String>,
}

impl<'a> SearchKey<'a> {
    pub fn new(entry: &'a EntryMeta) -> SearchKey<'a> {
        SearchKey {
            entry,
            name: entry.name.to_lowercase(),
            entry_type: entry.entry_type.to_lowercase(),
            description: entry.description.to_lowercase(),
            tags: entry.tags.iter().map(|t| t.to_lowercase()).collect(),
        }
    }
}

/// Best tier a single term matches at, or None when it does not match at all.
fn match_term(entry: &SearchKey<'_>, term: &Term) -> Option<Tier> {
    let name = entry.name.as_str();
    let entry_type = entry.entry_type.as_str();
    let description = entry.description.as_str();
    let tags: &[String] = &entry.tags;

    match term.field {
        Some(Field::Type) => {
            if entry_type == term.text {
                Some(Tier::Exact)
            } else if entry_type.starts_with(&term.text) || token_prefix(entry_type, &term.text) {
                Some(Tier::Prefix)
            } else if entry_type.contains(&term.text) {
                Some(Tier::Field)
            } else {
                None
            }
        }
        Some(Field::Tag) => {
            if tags.iter().any(|t| t == &term.text) {
                Some(Tier::Exact)
            } else if tags.iter().any(|t| t.starts_with(&term.text)) {
                Some(Tier::Prefix)
            } else if tags.iter().any(|t| t.contains(&term.text)) {
                Some(Tier::Field)
            } else {
                None
            }
        }
        None => {
            if name == term.text {
                return Some(Tier::Exact);
            }
            if name.starts_with(&term.text) {
                return Some(Tier::Prefix);
            }
            if token_prefix(name, &term.text) {
                return Some(Tier::TokenPrefix);
            }
            if name.contains(&term.text) {
                return Some(Tier::Contains);
            }
            if entry_type.contains(&term.text)
                || description.contains(&term.text)
                || tags.iter().any(|t| t.contains(&term.text))
            {
                return Some(Tier::Field);
            }
            if term.text.chars().count() >= 2 && fuzzy(name, &term.text) {
                return Some(Tier::Fuzzy);
            }
            None
        }
    }
}

/// Deterministic ranking: the worst tier any term matches at decides the bucket, usage and
/// pinning only break ties inside a bucket, and the id keeps the order stable.
pub fn score(entry: &SearchKey<'_>, terms: &[Term]) -> Option<(u8, bool, i64)> {
    if terms.is_empty() {
        return Some((0, entry.entry.pinned, entry.entry.usage_count));
    }
    let mut worst = Tier::Exact as u8;
    for term in terms {
        let tier = match_term(entry, term)?;
        worst = worst.max(tier as u8);
        if tier as u8 > MAX_TIER as u8 {
            return None;
        }
    }
    Some((worst, entry.entry.pinned, entry.entry.usage_count))
}

/// Completion text shown muted in the input: only ever the entry name.
pub fn completion_for(query: &str, name: &str) -> Option<String> {
    if query.is_empty() || name.to_lowercase() == query.to_lowercase() {
        return None;
    }
    if name.to_lowercase().starts_with(&query.to_lowercase()) {
        Some(name.to_string())
    } else {
        None
    }
}

pub fn rank(entries: &[EntryMeta], query: &str, limit: usize) -> Vec<SearchResult> {
    let terms = parse_query(query);
    let trimmed = query.trim();
    let keys: Vec<SearchKey<'_>> = entries.iter().map(SearchKey::new).collect();
    let mut scored: Vec<(u8, bool, i64, &SearchKey<'_>)> = keys
        .iter()
        .filter_map(|entry| score(entry, &terms).map(|s| (s.0, s.1, s.2, entry)))
        .collect();

    if terms.is_empty() {
        // Empty query: pinned first, then most recently used, then name.
        scored.sort_by(|a, b| {
            (!a.1)
                .cmp(&!b.1)
                .then(b.2.cmp(&a.2))
                .then_with(|| a.3.name.cmp(&b.3.name))
                .then_with(|| a.3.entry.id.cmp(&b.3.entry.id))
        });
    } else {
        scored.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| (!a.1).cmp(&!b.1))
                .then_with(|| a.3.name.cmp(&b.3.name))
                .then_with(|| a.3.entry.id.cmp(&b.3.entry.id))
        });
    }

    scored
        .into_iter()
        .take(limit.min(DEFAULT_LIMIT))
        .map(|(_tier, _pinned, usage, key)| SearchResult {
            completion: if terms.is_empty() {
                None
            } else {
                completion_for(trimmed, &key.entry.name)
            },
            id: key.entry.id.clone(),
            kind: key.entry.kind,
            name: key.entry.name.clone(),
            entry_type: key.entry.entry_type.clone(),
            description: key.entry.description.clone(),
            pinned: key.entry.pinned,
            usage_count: usage,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(id: &str, name: &str, entry_type: &str, tags: &[&str], description: &str) -> EntryMeta {
        EntryMeta {
            id: id.to_string(),
            kind: crate::models::Kind::Command,
            revision: 1,
            name: name.to_string(),
            entry_type: entry_type.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            description: description.to_string(),
            pinned: false,
            updated_at: String::new(),
            usage_count: 0,
            last_used_at: None,
        }
    }

    #[test]
    fn ranks_exact_before_prefix_token_and_fuzzy() {
        let entries = vec![
            meta("f", "restart docker service", "Docker", &[], ""),
            meta("t", "git revert HEAD", "Git", &[], ""),
            meta("p", "docker compose up", "Docker", &[], ""),
            meta("e", "docker", "Docker", &[], ""),
        ];
        let ids: Vec<String> = rank(&entries, "docker", 10)
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec!["e", "p", "f"]);
    }

    #[test]
    fn usage_only_breaks_ties() {
        let mut a = meta("a", "docker prune", "Docker", &[], "");
        a.usage_count = 99;
        let b = meta("b", "docker", "Docker", &[], "");
        let ids: Vec<String> = rank(&[a, b], "docker", 10)
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec!["b", "a"]);
    }

    #[test]
    fn filters_by_type_and_tag() {
        let entries = vec![
            meta("1", "token", "API key", &["work"], ""),
            meta("2", "token refresh", "Git", &["home"], ""),
        ];
        let ids: Vec<String> = rank(&entries, "token type:api", 10)
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec!["1"]);
        let ids: Vec<String> = rank(&entries, "token tag:home", 10)
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec!["2"]);
    }

    #[test]
    fn empty_query_prefers_pinned_then_usage() {
        let mut pinned = meta("p", "zeta", "Z", &[], "");
        pinned.pinned = true;
        let mut hot = meta("h", "alpha", "A", &[], "");
        hot.usage_count = 5;
        let ids: Vec<String> = rank(&[hot, pinned], "", 10)
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec!["p", "h"]);
    }

    #[test]
    fn description_only_match_is_ranked_below_name_match() {
        let entries = vec![
            meta("d", "cleanup", "Docker", &[], "prunes containers"),
            meta("n", "prune containers", "Docker", &[], ""),
        ];
        let ids: Vec<String> = rank(&entries, "prunes", 10)
            .into_iter()
            .map(|r| r.id)
            .collect();
        // The description match wins; the fuzzy name near-miss is still offered, ranked below it.
        assert_eq!(ids, vec!["d", "n"]);
    }

    #[test]
    fn completion_only_extends_a_prefix() {
        assert_eq!(
            completion_for("doc", "Docker cleanup").as_deref(),
            Some("Docker cleanup")
        );
        assert_eq!(completion_for("clean", "Docker cleanup"), None);
        assert_eq!(completion_for("docker", "docker"), None);
    }
}
