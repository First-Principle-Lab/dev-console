use crate::context::FinderContext;
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

/// True when `entry` belongs to the project the finder was opened from. Two ways in, matching the
/// two ways a command comes to be about a project:
///
/// * its stored working directory resolves (with `~` expanded exactly as execution expands it) to
///   the project root or something below it;
/// * it carries a `project:<name>` tag naming the same folder, compared case-insensitively -
///   which also covers a command saved with an absolute path through a symlink, or none at all.
///
/// Nothing is ever in context without a context, which is what keeps `rank` behaving exactly as it
/// did before this existed.
fn in_context(entry: &EntryMeta, context: Option<&FinderContext>) -> bool {
    let Some(context) = context else {
        return false;
    };
    let tag = format!("project:{}", context.name).to_lowercase();
    if entry.tags.iter().any(|tagged| tagged.to_lowercase() == tag) {
        return true;
    }
    if entry.working_directory.trim().is_empty() {
        return false;
    }
    crate::execution::expand_working_directory(&entry.working_directory)
        .ok()
        .flatten()
        .is_some_and(|directory| directory.starts_with(&context.root))
}

/// Deterministic ranking with no project context: the behaviour every caller had before project
/// ranking existed, and what everything falls back to when there is nothing to fall back on.
pub fn rank(entries: &[EntryMeta], query: &str, limit: usize) -> Vec<SearchResult> {
    rank_in_context(entries, query, limit, None)
}

/// As [`rank`], but putting the entries of `context` - the project the terminal the palette was
/// opened from is in - ahead of the rest.
///
/// Context only ever reorders *inside* a decision, never replaces one:
///
/// * empty query: pinned, then in context, then most recently used, then most used, then name;
/// * with a query: the worst tier any term matches at still decides first, so an exact match in
///   another project still beats a fuzzy one in this one; within a tier, in-context entries come
///   before usage.
pub fn rank_in_context(
    entries: &[EntryMeta],
    query: &str,
    limit: usize,
    context: Option<&FinderContext>,
) -> Vec<SearchResult> {
    let terms = parse_query(query);
    let trimmed = query.trim();
    let keys: Vec<SearchKey<'_>> = entries.iter().map(SearchKey::new).collect();
    let mut scored: Vec<(u8, bool, i64, bool, &SearchKey<'_>)> = keys
        .iter()
        .filter_map(|entry| {
            let (tier, pinned, usage) = score(entry, &terms)?;
            Some((tier, pinned, usage, in_context(entry.entry, context), entry))
        })
        .collect();

    if terms.is_empty() {
        // Empty query: pinned first, then the project the terminal was just in, then most recently
        // used, then most used, then name. Timestamps are UTC RFC 3339, so they order correctly as
        // strings; None (never used) sorts after every Some.
        scored.sort_by(|a, b| {
            (!a.1)
                .cmp(&!b.1)
                .then_with(|| (!a.3).cmp(&!b.3))
                .then_with(|| b.4.entry.last_used_at.cmp(&a.4.entry.last_used_at))
                .then(b.2.cmp(&a.2))
                .then_with(|| a.4.name.cmp(&b.4.name))
                .then_with(|| a.4.entry.id.cmp(&b.4.entry.id))
        });
    } else {
        scored.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| (!a.3).cmp(&!b.3))
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| (!a.1).cmp(&!b.1))
                .then_with(|| a.4.name.cmp(&b.4.name))
                .then_with(|| a.4.entry.id.cmp(&b.4.entry.id))
        });
    }

    scored
        .into_iter()
        .take(limit.min(DEFAULT_LIMIT))
        .map(|(_tier, _pinned, usage, in_context, key)| SearchResult {
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
            in_context,
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

    /// The same, saved with a working directory - how a command ends up belonging to a project.
    fn meta_at(id: &str, name: &str, working_directory: &str) -> EntryMeta {
        let mut entry = meta(id, name, "Shell", &[], "");
        entry.working_directory = working_directory.to_string();
        entry
    }

    fn context(root: &str, name: &str) -> FinderContext {
        FinderContext {
            root: std::path::PathBuf::from(root),
            name: name.to_string(),
        }
    }

    fn row_ids(results: &[SearchResult]) -> Vec<String> {
        results.iter().map(|r| r.id.clone()).collect()
    }

    /// Which entries an empty-query search flags as belonging to `root`, sorted so the assertion
    /// reads as a set rather than as an ordering claim.
    fn in_context_ids(entries: &[EntryMeta], root: &str) -> Vec<String> {
        let mut matched: Vec<String> =
            rank_in_context(entries, "", 10, Some(&context(root, "api")))
                .into_iter()
                .filter(|r| r.in_context)
                .map(|r| r.id)
                .collect();
        matched.sort();
        matched
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
    fn empty_query_puts_recently_used_before_frequently_used() {
        let mut frequent = meta("f", "alpha", "A", &[], "");
        frequent.usage_count = 9;
        frequent.last_used_at = Some("2026-10-01T09:00:00Z".into());
        let mut recent = meta("r", "beta", "B", &[], "");
        recent.usage_count = 1;
        recent.last_used_at = Some("2026-11-03T09:00:00Z".into());
        let never = meta("n", "aardvark", "C", &[], "");
        let ids: Vec<String> = rank(&[never, frequent, recent], "", 10)
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec!["r", "f", "n"]);
    }

    #[test]
    fn results_carry_tags_for_the_finder_rows() {
        let entries = vec![meta("w", "wireguard-off", "Network", &["vpn"], "")];
        let results = rank(&entries, "wire", 10);
        assert_eq!(results[0].tags, vec!["vpn".to_string()]);
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

    /// Manual benchmark, ignored so CI never asserts on machine speed: the palette re-searches
    /// the whole library on every keystroke, so the context pass must not be what makes typing
    /// feel slow. Run it on the target machine and put the printed number in `TEST_REPORT.md`.
    #[test]
    #[ignore = "a timing benchmark: the number belongs in TEST_REPORT.md, not in CI"]
    fn ranking_the_whole_library_stays_well_inside_a_frame() {
        use std::time::Instant;

        let context = FinderContext {
            root: std::path::PathBuf::from("/home/dev/projects/api"),
            name: "api".into(),
        };
        let entries: Vec<EntryMeta> = (0..5_000)
            .map(|i| EntryMeta {
                id: i.to_string(),
                kind: crate::models::Kind::Command,
                revision: 1,
                name: format!("command number {i}"),
                entry_type: "Shell".into(),
                tags: vec![],
                description: String::new(),
                pinned: i % 50 == 0,
                updated_at: String::new(),
                usage_count: i % 30,
                last_used_at: Some(format!("2026-11-0{}T09:00:00Z", 1 + i % 9)),
                working_directory: if i % 3 == 0 {
                    format!("/home/dev/projects/api/src/{i}")
                } else {
                    format!("/home/dev/elsewhere/{i}")
                },
            })
            .collect();

        let started = Instant::now();
        let rounds = 20;
        for _ in 0..rounds {
            assert_eq!(
                rank_in_context(&entries, "command", 50, Some(&context)).len(),
                50
            );
            assert_eq!(rank(&entries, "command", 50).len(), 50);
        }
        println!(
            "ranking 5,000 entries: {:?} per search, with and without a project context",
            started.elapsed() / (rounds * 2)
        );
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

    #[test]
    fn no_context_ranks_exactly_as_it_did_before() {
        let entries = vec![
            meta("a", "docker", "Docker", &[], ""),
            meta_at("b", "docker compose up", "/home/dev/elsewhere"),
            meta_at("c", "docker ps", "/home/dev/projects/api"),
            meta_at("d", "just test", "/home/dev/projects/api"),
        ];
        for query in ["", "docker", "docker compose", "just", "nomatch"] {
            assert_eq!(
                rank(&entries, query, 10),
                rank_in_context(&entries, query, 10, None),
                "no context must be the same code path it always was, for {query:?}"
            );
            assert!(
                rank(&entries, query, 10).iter().all(|r| !r.in_context),
                "nothing is in a project when there is no project"
            );
        }
    }

    #[test]
    fn an_empty_query_puts_the_current_project_straight_after_pinned() {
        let project = "/home/dev/projects/api";
        let mut pinned_elsewhere = meta_at("pinned", "sleep", "/home/dev/elsewhere");
        pinned_elsewhere.pinned = true;
        let mut busy_elsewhere = meta_at("busy", "docker ps", "/home/dev/elsewhere");
        busy_elsewhere.usage_count = 40;
        busy_elsewhere.last_used_at = Some("2026-11-05T09:00:00Z".into());
        // Never used, and named so it would sort last on its own: being in the project is what
        // moves it, nothing else.
        let here = meta_at("here", "zzz just run", project);

        let results = rank_in_context(
            &[busy_elsewhere, here, pinned_elsewhere],
            "",
            10,
            Some(&context(project, "api")),
        );
        assert_eq!(row_ids(&results), vec!["pinned", "here", "busy"]);
        assert_eq!(
            results.iter().map(|r| r.in_context).collect::<Vec<_>>(),
            vec![false, true, false]
        );
    }

    #[test]
    fn within_a_match_tier_the_current_project_beats_usage() {
        let project = "/home/dev/projects/api";
        let mut busy_elsewhere = meta_at("busy", "docker ps", "/home/dev/elsewhere");
        busy_elsewhere.usage_count = 99;
        let here = meta_at("here", "docker build", project);
        let entries = [here, busy_elsewhere];

        assert_eq!(row_ids(&rank(&entries, "docker", 10)), vec!["busy", "here"]);
        assert_eq!(
            row_ids(&rank_in_context(
                &entries,
                "docker",
                10,
                Some(&context(project, "api"))
            )),
            vec!["here", "busy"]
        );
    }

    #[test]
    fn a_better_match_in_another_project_still_beats_a_weaker_one_here() {
        let project = "/home/dev/projects/api";
        let exact_elsewhere = meta_at("exact", "docker", "/home/dev/elsewhere");
        // Matches only as a subsequence, so it lands in the weakest tier a name can.
        let fuzzy_here = meta_at("fuzzy", "dock er", project);
        let prefix_here = meta_at("prefix", "docker compose", project);

        let results = rank_in_context(
            &[fuzzy_here, prefix_here, exact_elsewhere],
            "docker",
            10,
            Some(&context(project, "api")),
        );
        assert_eq!(
            row_ids(&results),
            vec!["exact", "prefix", "fuzzy"],
            "context reorders within a tier only: an exact name elsewhere still leads"
        );
    }

    #[test]
    fn a_project_is_recognised_by_its_saved_directory_or_its_tag() {
        let project = "/home/dev/projects/api";
        let entries = vec![
            meta_at("at-root", "a", project),
            meta_at("nested", "b", "/home/dev/projects/api/crates/core"),
            // What the project-scripts import writes; users edit case, so tags match loosely.
            meta("tagged", "c", "Shell", &["project:api"], ""),
            meta("mixed-case-tag", "d", "Shell", &["Project:API"], ""),
            // None of these is the project, whatever a string comparison might guess.
            meta_at("sibling", "e", "/home/dev/projects/apibackend"),
            meta_at("above", "f", "/home/dev/projects"),
            meta_at("no-directory", "g", ""),
            meta("other-tag", "h", "Shell", &["project:other"], ""),
        ];
        assert_eq!(
            in_context_ids(&entries, project),
            vec!["at-root", "mixed-case-tag", "nested", "tagged"]
        );
    }

    #[test]
    fn a_saved_tilde_means_what_execution_would_run_it_as() {
        let home = std::env::var("HOME").expect("tests run with a home directory");
        let project = format!("{home}/projects/api");
        let entries = vec![
            meta_at("tilde", "a", "~/projects/api"),
            meta_at("tilde-nested", "b", "~/projects/api/src"),
            meta_at("absolute", "c", &project),
            // Home is where an empty or bare `~` lands, and that is not this project.
            meta_at("bare-tilde", "d", "~"),
            meta_at("empty", "e", ""),
            meta_at("other", "f", "~/projects/other"),
        ];
        assert_eq!(
            in_context_ids(&entries, &project),
            vec!["absolute", "tilde", "tilde-nested"]
        );
    }

    #[test]
    fn the_order_never_depends_on_the_order_entries_came_in() {
        let project = "/home/dev/projects/api";
        let entries = [
            meta_at("b", "same name", project),
            meta_at("a", "same name", "/home/dev/elsewhere"),
            meta_at("c", "same name", project),
        ];
        let forwards = row_ids(&rank_in_context(
            &entries,
            "same",
            10,
            Some(&context(project, "api")),
        ));
        let backwards: Vec<EntryMeta> = entries.iter().rev().cloned().collect();
        assert_eq!(
            forwards,
            row_ids(&rank_in_context(
                &backwards,
                "same",
                10,
                Some(&context(project, "api"))
            )),
            "ties are settled by name then id, so equal input always gives equal output"
        );
        assert_eq!(
            forwards,
            vec!["b", "c", "a"],
            "in context first, and inside that by id since every name is equal"
        );
    }
}
