use crate::context::{home_dir, is_under, FinderContext};
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

/// Deterministic ranking: the worst tier any term matches at decides the bucket, project
/// context outranks usage inside that bucket (and after pinning for empty queries), and
/// usage and pinning only break ties inside a bucket otherwise. The id keeps the order
/// stable.
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

/// True when `entry` belongs to `ctx`: it is tagged `project:<name>` (case-insensitive, the
/// convention the project-scripts import uses), or its stored working directory — expanded
/// exactly like `execution` does at run time — resolves inside the project root. Secrets
/// carry no working directory, so only their tags can match.
fn in_context(entry: &SearchKey<'_>, ctx: &FinderContext, home: Option<&std::path::Path>) -> bool {
    let expected_tag = format!("project:{}", ctx.name.to_lowercase());
    if entry.tags.iter().any(|tag| tag == &expected_tag) {
        return true;
    }
    let raw = entry.entry.working_directory.trim();
    if raw.is_empty() {
        return false;
    }
    let expanded = crate::context::expand_working_directory(raw, home);
    is_under(&expanded, &ctx.root)
}

pub fn rank(
    entries: &[EntryMeta],
    query: &str,
    limit: usize,
    context: Option<&FinderContext>,
) -> Vec<SearchResult> {
    let terms = parse_query(query);
    let trimmed = query.trim();
    // $HOME is only consulted when a context exists to compare against.
    let home = if context.is_some() { home_dir() } else { None };
    let keys: Vec<SearchKey<'_>> = entries.iter().map(SearchKey::new).collect();
    let mut scored: Vec<(u8, bool, i64, bool, &SearchKey<'_>)> = keys
        .iter()
        .filter_map(|entry| {
            score(entry, &terms).map(|s| {
                let in_ctx = context.is_some_and(|ctx| in_context(entry, ctx, home.as_deref()));
                (s.0, s.1, s.2, in_ctx, entry)
            })
        })
        .collect();

    if terms.is_empty() {
        // Empty query: pinned, then in-context, then most recently used, then most used,
        // then name. Timestamps are UTC RFC 3339, so they order correctly as strings;
        // None (never used) sorts after every Some.
        scored.sort_by(|a, b| {
            (!a.1)
                .cmp(&!b.1)
                .then(b.3.cmp(&a.3))
                .then_with(|| b.4.entry.last_used_at.cmp(&a.4.entry.last_used_at))
                .then(b.2.cmp(&a.2))
                .then_with(|| a.4.name.cmp(&b.4.name))
                .then_with(|| a.4.entry.id.cmp(&b.4.entry.id))
        });
    } else {
        // With a query the match tier still decides first: an exact match outside the
        // project beats a fuzzy match inside it. Within one tier, in-context comes before
        // usage.
        scored.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(b.3.cmp(&a.3))
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| (!a.1).cmp(&!b.1))
                .then_with(|| a.4.name.cmp(&b.4.name))
                .then_with(|| a.4.entry.id.cmp(&b.4.entry.id))
        });
    }

    scored
        .into_iter()
        .take(limit.min(DEFAULT_LIMIT))
        .map(|(_tier, _pinned, usage, in_ctx, key)| SearchResult {
            completion: if terms.is_empty() {
                None
            } else {
                completion_for(trimmed, &key.entry.name)
            },
            id: key.entry.id.clone(),
            kind: key.entry.kind,
            name: key.entry.name.clone(),
            entry_type: key.entry.entry_type.clone(),
            tags: key.entry.tags.clone(),
            description: key.entry.description.clone(),
            pinned: key.entry.pinned,
            usage_count: usage,
            last_used_at: key.entry.last_used_at.clone(),
            in_context: in_ctx,
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
            working_directory: String::new(),
        }
    }

    fn meta_with_dir(
        id: &str,
        name: &str,
        entry_type: &str,
        tags: &[&str],
        working_directory: &str,
    ) -> EntryMeta {
        let mut entry = meta(id, name, entry_type, tags, "");
        entry.working_directory = working_directory.to_string();
        entry
    }

    fn context(root: &str, name: &str) -> crate::context::FinderContext {
        crate::context::FinderContext {
            root: std::path::PathBuf::from(root),
            name: name.to_string(),
        }
    }

    fn ids(results: &[SearchResult]) -> Vec<String> {
        results.iter().map(|r| r.id.clone()).collect()
    }

    #[test]
    fn ranks_exact_before_prefix_token_and_fuzzy() {
        let entries = vec![
            meta("f", "restart docker service", "Docker", &[], ""),
            meta("t", "git revert HEAD", "Git", &[], ""),
            meta("p", "docker compose up", "Docker", &[], ""),
            meta("e", "docker", "Docker", &[], ""),
        ];
        assert_eq!(
            ids(&rank(&entries, "docker", 10, None)),
            vec!["e", "p", "f"]
        );
    }

    #[test]
    fn usage_only_breaks_ties() {
        let mut a = meta("a", "docker prune", "Docker", &[], "");
        a.usage_count = 99;
        let b = meta("b", "docker", "Docker", &[], "");
        assert_eq!(ids(&rank(&[a, b], "docker", 10, None)), vec!["b", "a"]);
    }

    #[test]
    fn filters_by_type_and_tag() {
        let entries = vec![
            meta("1", "token", "API key", &["work"], ""),
            meta("2", "token refresh", "Git", &["home"], ""),
        ];
        assert_eq!(
            ids(&rank(&entries, "token type:api", 10, None)),
            vec!["1".to_string()]
        );
        assert_eq!(
            ids(&rank(&entries, "token tag:home", 10, None)),
            vec!["2".to_string()]
        );
    }

    #[test]
    fn empty_query_prefers_pinned_then_usage() {
        let mut pinned = meta("p", "zeta", "Z", &[], "");
        pinned.pinned = true;
        let mut hot = meta("h", "alpha", "A", &[], "");
        hot.usage_count = 5;
        assert_eq!(ids(&rank(&[hot, pinned], "", 10, None)), vec!["p", "h"]);
    }

    #[test]
    fn empty_query_puts_recently_used_before_frequently_used() {
        let mut frequent = meta("f", "alpha", "A", &[], "");
        frequent.usage_count = 9;
        frequent.last_used_at = Some("2026-10-01T09:00:00Z".into());
        let mut recent = meta("r", "beta", "B", &[], "");
        recent.usage_count = 1;
        recent.last_used_at = Some("2026-11-03T09:00:00Z".into());
        let never = meta("n", "aardvark", "C", &[], "");
        assert_eq!(
            ids(&rank(&[never, frequent, recent], "", 10, None)),
            vec!["r", "f", "n"]
        );
    }

    #[test]
    fn results_carry_tags_for_the_finder_rows() {
        let entries = vec![meta("w", "wireguard-off", "Network", &["vpn"], "")];
        let results = rank(&entries, "wire", 10, None);
        assert_eq!(results[0].tags, vec!["vpn".to_string()]);
    }

    #[test]
    fn description_only_match_is_ranked_below_name_match() {
        let entries = vec![
            meta("d", "cleanup", "Docker", &[], "prunes containers"),
            meta("n", "prune containers", "Docker", &[], ""),
        ];
        // The description match wins; the fuzzy name near-miss is still offered, below it.
        assert_eq!(ids(&rank(&entries, "prunes", 10, None)), vec!["d", "n"]);
    }

    #[test]
    fn ranking_without_context_never_flags_entries() {
        let mut hot = meta("h", "docker run", "Docker", &[], "");
        hot.usage_count = 40;
        let cold = meta("c", "docker ps", "Docker", &[], "");
        let results = rank(&[hot, cold], "docker", 10, None);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| !r.in_context));
        // ...and the pre-feature order assertions above all pass against rank(.., None).
    }

    #[test]
    fn empty_query_ranks_pinned_then_context_then_recency() {
        let root = "/home/dev/projects/api";
        let ctx = context(root, "api");
        let mut pinned_outside = meta("p", "zebra", "Z", &[], "");
        pinned_outside.pinned = true;
        let inside = meta_with_dir("i", "inside", "S", &[], root); // never used
        let mut outside_recent = meta_with_dir("o", "outside", "S", &[], "/elsewhere");
        outside_recent.last_used_at = Some("2026-11-03T09:00:00Z".into());
        let order = ids(&rank(
            &[outside_recent, inside, pinned_outside],
            "",
            10,
            Some(&ctx),
        ));
        assert_eq!(order, vec!["p", "i", "o"]);
    }

    #[test]
    fn with_a_query_tiers_still_decide_before_context_and_context_beats_usage() {
        let root = "/home/dev/projects/api";
        let ctx = context(root, "api");
        // Exact name match outside the project vs fuzzy match inside: tier wins.
        let outside_exact = meta_with_dir("x", "api", "A", &[], "/elsewhere");
        let mut inside_fuzzy = meta_with_dir("i", "a-p-i staging", "D", &[], root);
        inside_fuzzy.usage_count = 999;
        let order = ids(&rank(&[inside_fuzzy, outside_exact], "api", 10, Some(&ctx)));
        assert_eq!(order, vec!["x", "i"]);

        // Same tier: in-context outranks a far higher usage count.
        let mut inside = meta_with_dir("i", "deploy alpha", "D", &[], root);
        inside.usage_count = 1;
        let mut outside = meta_with_dir("o", "deploy beta", "D", &[], "/elsewhere");
        outside.usage_count = 999;
        let order = ids(&rank(&[outside, inside], "deploy", 10, Some(&ctx)));
        assert_eq!(order, vec!["i", "o"]);
    }

    #[test]
    fn membership_matches_the_project_tag_case_insensitively() {
        let ctx = context("/home/dev/projects/api", "api");
        let tagged = meta("t", "deploy", "D", &["Project:API"], "");
        let untagged = meta("u", "redeploy", "D", &["other"], "");
        let results = rank(&[untagged, tagged], "deploy", 10, Some(&ctx));
        assert_eq!(results[0].id, "t");
        assert!(results[0].in_context);
        assert!(!results[1].in_context);
    }

    #[test]
    fn membership_matches_working_directories_under_the_root() {
        let root = "/home/dev/projects/api";
        let ctx = context(root, "api");
        let exact = meta_with_dir("e", "build one", "B", &[], root);
        let nested = meta_with_dir(
            "n",
            "build two",
            "B",
            &[],
            "/home/dev/projects/api/src-tauri",
        );
        let sibling = meta_with_dir("s", "build three", "B", &[], "/home/dev/projects/apitest");
        let blank = meta_with_dir("x", "build four", "B", &[], "   ");
        let results = rank(&[sibling, blank, nested, exact], "build", 10, Some(&ctx));
        let flagged: Vec<String> = results
            .iter()
            .filter(|r| r.in_context)
            .map(|r| r.id.clone())
            .collect();
        assert_eq!(flagged, vec!["e", "n"]);
    }

    #[test]
    fn membership_expands_tilde_like_execution_does() {
        // rank() expands through the same helper execution uses; test the seam directly
        // because $HOME must not be mutated process-wide.
        let expanded = crate::context::expand_working_directory(
            "~/projects/api/scripts",
            Some(std::path::Path::new("/home/dev")),
        );
        assert_eq!(
            expanded,
            std::path::PathBuf::from("/home/dev/projects/api/scripts")
        );
        assert!(crate::context::is_under(
            &expanded,
            std::path::Path::new("/home/dev/projects/api")
        ));

        // With the real $HOME as the project's parent, the flag must follow for real:
        // build a context under $HOME itself.
        let home = crate::context::home_dir().expect("tests run with HOME set");
        let entry = meta_with_dir("t", "ship it", "S", &[], "~/ctx-probe/sub");
        let ctx = crate::context::FinderContext {
            root: home.join("ctx-probe"),
            name: "ctx-probe".into(),
        };
        let results = rank(&[entry], "ship", 10, Some(&ctx));
        assert!(
            results[0].in_context,
            "a ~/ path expanding under the root must count as in-context"
        );
    }

    #[test]
    fn context_ordering_is_deterministic() {
        let ctx = context("/home/dev/projects/api", "api");
        let a = meta_with_dir("a", "build one", "B", &[], "/home/dev/projects/api");
        let b = meta_with_dir("b", "build two", "B", &[], "/elsewhere");
        let c = meta_with_dir("c", "build three", "B", &[], "/home/dev/projects/api/x");
        let first = rank(&[a.clone(), b.clone(), c.clone()], "build", 10, Some(&ctx));
        let second = rank(&[c, b, a], "build", 10, Some(&ctx));
        assert_eq!(ids(&first), ids(&second));
        assert_eq!(
            first.iter().map(|r| r.in_context).collect::<Vec<_>>(),
            second.iter().map(|r| r.in_context).collect::<Vec<_>>()
        );
    }
}
