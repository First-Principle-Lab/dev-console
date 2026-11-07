//! Shell command credential filtering and suggestion names, plus legacy history parsers.
//! Project-aware collection and ranking live in `project_history`; legacy global history has
//! no cwd and is never assigned to a project by the application.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::models::EntryRecord;

pub const DEFAULT_LIMIT: usize = 5;
/// The most suggestions one request can ask for: a panel longer than this is a list to scroll, not
/// a suggestion.
pub const MAX_LIMIT: usize = 20;

/// Reading more bytes than this cannot change the ranking usefully, and history files in the wild
/// run to hundreds of megabytes. The tail is what matters: it holds the recent commands.
const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;
/// Entries kept per file after the tail is parsed, for the same reason: the count and the recency
/// weight of a command run 50 000 lines ago are not worth the parse.
const MAX_ENTRIES_PER_SOURCE: usize = 20_000;

/// zsh writes a metafied byte as `0x83` followed by the original byte with bit 5 flipped.
const ZSH_META: u8 = 0x83;
const ZSH_META_XOR: u8 = 0x20;

/// Commands that are used constantly and are useless as saved entries. Compared against the
/// normalised text, so `git  status` is on the list too.
const DENY_LIST: [&str; 7] = ["ls", "cd", "clear", "exit", "history", "pwd", "git status"];

/// Substrings that make an environment assignment a credential rather than a setting.
const SECRET_NAME_MARKERS: [&str; 5] = ["TOKEN", "SECRET", "PASS", "KEY", "AUTH"];
/// Substrings that make a long option, or a dotted/dashed/underscored setting name, one that
/// carries a credential: `--api-key`, `--client-secret`, `aws_secret_access_key`, `user.password`.
const SECRET_WORD_MARKERS: [&str; 11] = [
    "password",
    "passwd",
    "secret",
    "token",
    "apikey",
    "api-key",
    "api_key",
    "access-key",
    "access_key",
    "private-key",
    "private_key",
];
/// Options that take a `user:password` value.
const CREDENTIAL_FLAGS: [&str; 3] = ["-u", "--user", "--username"];
/// Programs (and the `login` subcommand of docker, podman, helm …) whose standalone `-p` takes a
/// password as the next argument - unlike `ssh -p 2222` or `docker run -p 8080:80`, a port.
const PASSWORD_P_COMMANDS: [&str; 7] = [
    "sshpass",
    "mysql",
    "mysqldump",
    "mysqladmin",
    "mariadb",
    "mariadb-dump",
    "login",
];
/// Header names (in `-H 'Name: value'`) that carry a credential. `Authorization` is caught on its
/// own, wherever it appears.
const SECRET_HEADER_MARKERS: [&str; 7] = [
    "key", "token", "secret", "auth", "cookie", "password", "session",
];
/// git subcommands that take a commit id, so a 40- or 64-character hex argument there is a hash,
/// not a credential.
const GIT_REVISION_COMMANDS: [&str; 10] = [
    "checkout",
    "switch",
    "show",
    "revert",
    "cherry-pick",
    "reset",
    "log",
    "diff",
    "rebase",
    "branch",
];

/// A shorter command than this is almost always something the user types over and over rather than
/// saves.
const MIN_COMMAND_CHARS: usize = 6;
/// A name is a label, not a copy of the command line.
const MAX_NAME_CHARS: usize = 60;
/// A word longer than this is a value (a path, a glob, an id), not something to put in a name.
const MAX_WORD_CHARS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

/// One history file that exists, with the parser its shell writes.
#[derive(Debug, Clone)]
pub struct HistorySource {
    pub shell: Shell,
    pub path: PathBuf,
}

/// A candidate for the library: the exact command line, how often it was run, and a name worth
/// putting on the entry. `key` is the hash of the normalised command, which is what a dismissal
/// stores - never the command itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub key: String,
    pub command: String,
    pub count: usize,
    pub suggested_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    pub project_count: usize,
}

impl HistorySource {
    /// Parses this source. A file that vanished, is unreadable or is not a file simply yields no
    /// lines: a suggestion panel must never be able to fail a library load.
    pub fn read(&self) -> Vec<String> {
        let Some(bytes) = read_tail(&self.path) else {
            return Vec::new();
        };
        let mut lines = match self.shell {
            Shell::Zsh => parse_zsh(&bytes),
            Shell::Bash => parse_bash(&bytes),
            Shell::Fish => parse_fish(&bytes),
        };
        if lines.len() > MAX_ENTRIES_PER_SOURCE {
            let stale = lines.len() - MAX_ENTRIES_PER_SOURCE;
            lines.drain(..stale);
        }
        lines
    }
}

/// Which history files exist on this machine, in the order they are merged.
pub fn locate() -> Vec<HistorySource> {
    locate_with(
        |name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        },
        &home_dir(),
    )
}

/// `locate` with its inputs supplied, so nothing has to read a real home directory to be tested.
///
/// A `$HISTFILE` replaces the zsh default rather than adding to it - zsh only ever writes one
/// history file - and the same path is never offered twice. The sources come back least recently
/// written first, which is what [`collect`] relies on for recency.
pub fn locate_with(env: impl Fn(&str) -> Option<String>, home: &Path) -> Vec<HistorySource> {
    let mut sources = Vec::new();
    let zsh = env("HISTFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".zsh_history"));
    push_existing(&mut sources, Shell::Zsh, &zsh);
    push_existing(&mut sources, Shell::Bash, &home.join(".bash_history"));
    let xdg_data = env("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local").join("share"));
    push_existing(
        &mut sources,
        Shell::Fish,
        &xdg_data.join("fish").join("fish_history"),
    );
    // Stable, so files with the same (or an unreadable) modification time keep the order above.
    sources.sort_by_key(|source| {
        std::fs::metadata(&source.path)
            .and_then(|meta| meta.modified())
            .ok()
    });
    sources
}

fn push_existing(sources: &mut Vec<HistorySource>, shell: Shell, path: &Path) {
    if sources.iter().any(|source| source.path == path) {
        return;
    }
    if path.is_file() {
        sources.push(HistorySource {
            shell,
            path: path.to_path_buf(),
        });
    }
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Every command line from every source, oldest first within a file.
///
/// Files are concatenated in `locate` order - least recently written first - rather than merged by
/// timestamp: the ranking only needs *relative* recency, and the shell written to most recently is
/// last, so its commands carry the highest weight. A bash history nobody has touched in months
/// cannot outrank the zsh history in use today just by being read second.
pub fn collect(sources: &[HistorySource]) -> Vec<String> {
    let mut lines = Vec::new();
    for source in sources {
        lines.extend(source.read());
    }
    lines
}

/// Reads at most the last [`MAX_READ_BYTES`] of a file. The first line of a trimmed tail is
/// dropped, because it is almost certainly cut in half.
fn read_tail(path: &Path) -> Option<Vec<u8>> {
    let size = std::fs::metadata(path).ok()?.len();
    if size <= MAX_READ_BYTES {
        return std::fs::read(path).ok();
    }
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::End(-(MAX_READ_BYTES as i64))).ok()?;
    let mut bytes = Vec::with_capacity(MAX_READ_BYTES as usize);
    file.read_to_end(&mut bytes).ok()?;
    let trimmed = bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|newline| bytes.split_off(newline + 1));
    Some(trimmed.unwrap_or(bytes))
}

/// Undoes zsh metafication (`0x83` plus the byte with bit 5 flipped). It runs per record, *after*
/// the bytes are split on newlines, so a metafied newline (`0x83 0x2a`) becomes a real newline
/// inside the command instead of splitting the record in two, and a byte sequence that is not UTF-8
/// costs one replacement character rather than the whole file.
fn unmetafy(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == ZSH_META && index + 1 < bytes.len() {
            out.push(bytes[index + 1] ^ ZSH_META_XOR);
            index += 2;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    out
}

/// zsh's `extended_history` prefix: `: <ts>:<dur>;command`. Both fields have to be numeric (a
/// duration may be negative), so a command that genuinely starts with `:` is left alone.
fn strip_zsh_prefix(line: &str) -> &str {
    let Some(rest) = line.strip_prefix(": ") else {
        return line;
    };
    let Some(colon) = rest.find(':') else {
        return line;
    };
    if !digits(&rest[..colon], false) {
        return line;
    }
    let after = &rest[colon + 1..];
    let Some(semi) = after.find(';') else {
        return line;
    };
    if !digits(&after[..semi], true) {
        return line;
    }
    &after[semi + 1..]
}

fn digits(text: &str, allow_sign: bool) -> bool {
    let body = if allow_sign {
        text.strip_prefix(['-', '+']).unwrap_or(text)
    } else {
        text
    };
    !body.is_empty() && body.bytes().all(|byte| byte.is_ascii_digit())
}

/// Splits a physical line into its text and whether the command continues on the next line. Only an
/// odd number of trailing backslashes continues, so `printf 'x\\'` stays one command.
fn split_continuation(line: &str) -> (&str, bool) {
    let trailing = line.chars().rev().take_while(|ch| *ch == '\\').count();
    if trailing == 0 || trailing % 2 == 0 {
        return (line, false);
    }
    (&line[..line.len() - 1], true)
}

/// zsh history, extended or plain, with backslash continuations joined.
pub fn parse_zsh(bytes: &[u8]) -> Vec<String> {
    let records: Vec<String> = bytes
        .split(|byte| *byte == b'\n')
        .map(|record| String::from_utf8_lossy(&unmetafy(record)).into_owned())
        .collect();
    parse_plain(records.iter().map(String::as_str), strip_zsh_prefix)
}

/// bash history: plain lines and backslash continuations, minus the `#<epoch>` timestamps
/// `histtimeformat` writes and the comments a shell never runs.
pub fn parse_bash(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    parse_plain(
        text.lines(),
        |line| {
            if line.starts_with('#') {
                ""
            } else {
                line
            }
        },
    )
}

/// fish history: the `- cmd: <value>` rows, with `\n` unescaped back into a real newline. The
/// `when:`/`paths:`/`dirs:` keys that follow each entry carry no command and are skipped.
pub fn parse_fish(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut commands = Vec::new();
    for line in text.lines() {
        let Some(value) = line.strip_prefix("- cmd:") else {
            continue;
        };
        // Exactly one space belongs to the separator; a second one is the user's own leading space,
        // which the ignore-space convention drops.
        let command = unescape_fish(value.strip_prefix(' ').unwrap_or(value));
        if !command.trim().is_empty() && !command.starts_with(' ') {
            commands.push(command);
        }
    }
    commands
}

fn unescape_fish(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// The line machine the zsh and bash parsers share: strip each record's prefix, join backslash
/// continuations with a real newline (which is what the user ran), then drop the records that are
/// empty or begin with a space - the `HIST_IGNORE_SPACE` convention, applied to the whole record
/// rather than to its first physical line, so an indented continuation stays part of the command.
fn parse_plain<'a>(
    lines: impl Iterator<Item = &'a str>,
    strip: impl Fn(&'a str) -> &'a str,
) -> Vec<String> {
    let mut commands = Vec::new();
    let mut record: Option<String> = None;
    for physical in lines {
        if let Some(open) = record.as_mut() {
            let (body, continues) = split_continuation(physical);
            open.push('\n');
            open.push_str(body);
            if !continues {
                let joined = std::mem::take(open);
                commands.push(joined);
                record = None;
            }
            continue;
        }
        let line = strip(physical);
        let (body, continues) = split_continuation(line);
        if continues {
            record = Some(body.to_string());
        } else if !body.trim().is_empty() && !body.starts_with(' ') {
            commands.push(body.to_string());
        }
    }
    // A file whose last record was cut off mid-continuation still holds that command.
    if let Some(open) = record {
        commands.push(open);
    }
    commands
        .into_iter()
        .filter(|command| !command.trim().is_empty() && !command.starts_with(' '))
        .collect()
}

/// The identity of a command: what is counted, what is compared against the library, and what a
/// dismissal hashes. Trims and collapses every run of whitespace - including the newlines of a
/// continued command - to a single space, so the same command typed with different wrapping counts
/// once.
pub fn normalise(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// SHA-256 of the normalised command, hex. This is what `dismissed_suggestions` stores.
pub fn key_for(normalised: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalised.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A command the library should not offer because it looks like it carries a credential.
///
/// Deliberately over-broad: a false positive costs one lost suggestion, a false negative puts a
/// password into a searchable, backed-up library. The known cost is that a `-p<value>` which is not
/// a port spec, or a 32+ character argument, is dropped along with the real credentials.
pub fn looks_like_secret(command: &str) -> bool {
    let lower = command.to_lowercase();

    for token in command.split_whitespace() {
        let Some(name) = assignment_name(token) else {
            continue;
        };
        let name = name.to_ascii_uppercase();
        if SECRET_NAME_MARKERS
            .iter()
            .any(|marker| name.contains(marker))
        {
            return true;
        }
    }

    if lower.contains("authorization:") || lower.contains("bearer ") {
        return true;
    }
    if url_has_credentials(&lower) {
        return true;
    }

    let tokens: Vec<&str> = lower.split_whitespace().collect();
    let password_p = tokens.iter().any(|token| {
        let program = token.rsplit('/').next().unwrap_or(token);
        PASSWORD_P_COMMANDS.contains(&program)
    });
    for (index, token) in tokens.iter().enumerate() {
        if short_password_flag(token) {
            return true;
        }
        // `sshpass -p pw`, `docker login -p pw`: the password is the next word.
        if password_p
            && *token == "-p"
            && tokens
                .get(index + 1)
                .is_some_and(|next| !next.starts_with('-'))
        {
            return true;
        }
        if secret_long_flag(token) {
            return true;
        }
        if (*token == "-h" || *token == "--header" || token.starts_with("--header="))
            && header_carries_secret(token, tokens.get(index + 1).copied())
        {
            return true;
        }
        // `aws configure set aws_secret_access_key X`, `git config user.password X`, and the same
        // name with its value attached: `db-password=x`.
        if secret_setting_name(token)
            && tokens
                .get(index + 1)
                .is_some_and(|next| !next.starts_with('-'))
        {
            return true;
        }
        if let Some((name, value)) = token.split_once('=') {
            if !value.is_empty() && secret_setting_name(name) {
                return true;
            }
        }
        if CREDENTIAL_FLAGS.contains(token)
            && option_value(token, index, &tokens).is_some_and(looks_like_credential_pair)
        {
            return true;
        }
    }

    high_entropy_argument(command).is_some()
}

/// A long option whose name says it carries a credential: `--password`, `--api-key=…`,
/// `--client-secret`, `--token`. Flagged whether or not a value follows, because one that prompts
/// is as useless to save as one that leaks.
fn secret_long_flag(token: &str) -> bool {
    let Some(flag) = token.strip_prefix("--") else {
        return false;
    };
    let name = flag.split_once('=').map_or(flag, |(name, _)| name);
    name == "pass"
        || SECRET_WORD_MARKERS
            .iter()
            .any(|marker| name.contains(marker))
}

/// The header an `-H`/`--header` option sets, when its name carries a credential
/// (`X-Api-Key:`, `Private-Token:`, `Cookie:`). The value arrives either after `=` or as the next
/// word, usually quoted.
fn header_carries_secret(token: &str, next: Option<&str>) -> bool {
    let value = match token.strip_prefix("--header=") {
        Some(value) => value,
        None => match next {
            Some(next) => next,
            None => return false,
        },
    };
    let value = value.trim_start_matches(['"', '\'']);
    let Some((name, _)) = value.split_once(':') else {
        return false;
    };
    SECRET_HEADER_MARKERS
        .iter()
        .any(|marker| name.contains(marker))
}

/// A setting key that names a credential: a single word joined by `_`, `-` or `.` that contains a
/// credential marker (`aws_secret_access_key`, `user.password`, `db-password`). A bare word like
/// `secret` - `kubectl get secret` - is not one.
fn secret_setting_name(token: &str) -> bool {
    !token.starts_with('-')
        && token.contains(['_', '-', '.'])
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        && SECRET_WORD_MARKERS
            .iter()
            .any(|marker| token.contains(marker))
}

/// `-p<value>`: mysql's way of passing a password inline. A value that reads as a port or a publish
/// spec is left alone, because `docker run -p8080:80` is a far more common suggestion candidate than
/// an inline password.
fn short_password_flag(token: &str) -> bool {
    if token.len() < 3 || !token.starts_with("-p") || token.starts_with("--") {
        return false;
    }
    let value = &token[2..];
    let port_spec = value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b':' | b'/' | b'-' | b'.'));
    !(port_spec && value.bytes().any(|byte| byte.is_ascii_digit()))
}

/// The name side of a `NAME=value` token (`export NAME=value`, `NAME=value command …`).
fn assignment_name(token: &str) -> Option<&str> {
    let token = token.trim_start_matches(['$', '{']);
    let (name, _value) = token.split_once('=')?;
    let name = name.trim_end_matches('}');
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    Some(name)
}

/// The value an option takes: `--user=x`, or the token after a standalone `-u alice:secret`.
fn option_value<'a>(token: &'a str, index: usize, tokens: &'a [&'a str]) -> Option<&'a str> {
    if let Some((_, value)) = token.split_once('=') {
        return (!value.is_empty()).then_some(value);
    }
    if !CREDENTIAL_FLAGS.contains(&token) {
        return None;
    }
    tokens
        .get(index + 1)
        .filter(|next| !next.starts_with('-'))
        .copied()
}

/// `user:password`, but not a URL (`https://host`) and not a uid/gid pair (`1000:1000`).
fn looks_like_credential_pair(value: &str) -> bool {
    let uid_gid = value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b':' | b'.' | b'-' | b'_'));
    if value.contains("://") || uid_gid {
        return false;
    }
    value.split_once(':').is_some_and(|(user, secret)| {
        !user.is_empty() && !secret.is_empty() && !secret.contains('/')
    })
}

/// `scheme://user:pass@host`, and `https://<token>@host` - a personal access token as the whole
/// user part, which is how GitHub and GitLab tokens are usually pasted into a clone URL. A plain
/// user name outside http(s) (`ssh://git@host`) is left alone. Only the authority is scanned, so
/// an `@` in a later path segment (`https://example.test/at/issue`) does not look like
/// credentials.
fn url_has_credentials(lower: &str) -> bool {
    let mut rest = lower;
    while let Some(scheme) = rest.find("://") {
        let after = &rest[scheme + 3..];
        let end = after.find(['/', '?', '#', ' ']).unwrap_or(after.len());
        let authority = &after[..end];
        if let Some((userinfo, _host)) = authority.rsplit_once('@') {
            let scheme_name = rest[..scheme]
                .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '+')
                .next()
                .unwrap_or("");
            let http = scheme_name.starts_with("http");
            if userinfo.contains(':') || (http && !userinfo.is_empty()) || userinfo.len() >= 20 {
                return true;
            }
        }
        let Some(next) = rest[scheme + 3 + end..].find("://") else {
            break;
        };
        rest = &rest[scheme + 3 + end + next..];
    }
    false
}

/// A 32+ character run of base64 or hex is a credential far more often than it is an argument worth
/// suggesting, so those commands stay out of the panel. Requiring a digit as well as a letter keeps
/// long flags, ordinary words and simple paths in play.
fn high_entropy_argument(command: &str) -> Option<String> {
    let words: Vec<&str> = command.split_whitespace().collect();
    // `git checkout 3f2a9c…`: a commit id, not a token.
    let git_revision = words.first() == Some(&"git")
        && words
            .get(1)
            .is_some_and(|sub| GIT_REVISION_COMMANDS.contains(sub));
    words
        .into_iter()
        .filter(|token| !(git_revision && is_commit_id(token)))
        .find(|token| {
            let body = token
                .trim_matches(['"', '\'', '`', '(', ')', '[', ']', '{', '}', ';', ','].as_slice());
            body.len() >= 32
                && body.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(byte, b'+' | b'/' | b'-' | b'_' | b'=' | b'.')
                })
                && body.bytes().any(|byte| byte.is_ascii_digit())
                && body.bytes().any(|byte| byte.is_ascii_alphabetic())
        })
        .map(str::to_string)
}

/// A full SHA-1 or SHA-256 commit id.
fn is_commit_id(token: &str) -> bool {
    matches!(token.len(), 40 | 64) && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The name a saved suggestion gets: the first two plain words of the command, plus the first plain
/// argument after them, joined with dashes - so `docker compose -f staging.yml up -d` becomes
/// "docker-compose-up".
///
/// Dashes rather than spaces because that is the library's own naming rule: `Space` copies the
/// selected row in the finder, so an entry name may not contain one. Options, the value of a short
/// option (`-f staging.yml`), quoted text and anything that plainly is a value rather than a word
/// (paths, versions, long tokens) are skipped, because they make a label unreadable.
pub fn suggested_name(command: &str) -> String {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut head: Vec<&str> = Vec::new();
    let mut argument = None;
    let mut quoted = false;
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        index += 1;
        if quoted {
            // Still inside a quoted value: skip until the quotes close again.
            if odd_quotes(token) {
                quoted = false;
            }
            continue;
        }
        if token.starts_with(['"', '\'']) {
            quoted = odd_quotes(token);
            continue;
        }
        if token.starts_with('-') {
            // A short option carries its value in the next token; a long option spells it out with
            // `=`, so only the flag itself is skipped.
            if token.len() == 2 {
                if let Some(next) = tokens.get(index) {
                    if !next.starts_with(['-', '"', '\'']) {
                        index += 1;
                    }
                }
            }
            continue;
        }
        if !is_word_like(token) {
            continue;
        }
        if head.len() < 2 {
            head.push(token);
            continue;
        }
        if argument.is_none() {
            argument = Some(token);
        }
    }

    let mut words: Vec<&str> = head.to_vec();
    if let Some(argument) = argument.filter(|argument| !head.contains(argument)) {
        words.push(argument);
    }
    words.join("-").chars().take(MAX_NAME_CHARS).collect()
}

fn odd_quotes(token: &str) -> bool {
    token.chars().filter(|ch| matches!(ch, '"' | '\'')).count() % 2 == 1
}

/// Something a short label can be built from: letters, digits and a few quiet separators, and short
/// enough to read. Paths, versions, globs and expansions are all excluded.
fn is_word_like(token: &str) -> bool {
    !token.is_empty()
        && token.chars().count() <= MAX_WORD_CHARS
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '@' | '#'))
}

fn is_trivial(normalised: &str) -> bool {
    normalised.chars().count() < MIN_COMMAND_CHARS
        || normalised.split_whitespace().count() < 2
        || DENY_LIST.contains(&normalised)
}

/// Ranks history lines into suggestions.
///
/// `lines` must be oldest first (what [`collect`] returns). A command scores `count * recency`,
/// where recency grows from 1.0 at the top of the file to 2.0 at the bottom, so something used often
/// *and* lately beats a burst of activity from two years ago. Ties break on the command text, which
/// makes the ranking a function of the file contents alone: the same history always produces the
/// same panel.
///
/// Dropped: trivial commands ([`is_trivial`]), anything [`looks_like_secret`] flags, commands already
/// in the library, and commands dismissed before (by hash).
pub fn suggest(
    lines: &[String],
    saved: &[EntryRecord],
    dismissed: &HashSet<String>,
    limit: usize,
) -> Vec<Suggestion> {
    if lines.is_empty() || limit == 0 {
        return Vec::new();
    }
    let saved_keys: HashSet<String> = saved
        .iter()
        .filter(|record| !record.command.trim().is_empty())
        .map(|record| normalise(&record.command))
        .collect();

    struct Candidate {
        command: String,
        count: usize,
        last: usize,
    }
    let mut seen: HashMap<String, Candidate> = HashMap::new();
    for (position, line) in lines.iter().enumerate() {
        let command = line.trim();
        if command.is_empty() || looks_like_secret(command) {
            continue;
        }
        let key = normalise(command);
        if key.is_empty() || is_trivial(&key) {
            continue;
        }
        seen.entry(key)
            .and_modify(|candidate| {
                candidate.count += 1;
                candidate.last = position;
                // The most recent spelling of the command is the one worth offering.
                candidate.command = command.to_string();
            })
            .or_insert(Candidate {
                command: command.to_string(),
                count: 1,
                last: position,
            });
    }

    let total = lines.len().max(1);
    let mut scored: Vec<(f64, Suggestion)> = seen
        .into_iter()
        .filter(|(normalised, _)| !saved_keys.contains(normalised))
        .filter(|(normalised, _)| !dismissed.contains(&key_for(normalised)))
        .map(|(normalised, candidate)| {
            let recency = 1.0 + (candidate.last as f64 / total as f64);
            (
                candidate.count as f64 * recency,
                Suggestion {
                    key: key_for(&normalised),
                    command: candidate.command.clone(),
                    count: candidate.count,
                    suggested_name: suggested_name(&candidate.command),
                    project: None,
                    project_root: None,
                    working_directory: None,
                    project_count: 0,
                },
            )
        })
        .collect();

    // Score first, then the command text: the order depends on the file alone, never on the
    // iteration order of a hash map.
    scored.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .total_cmp(left_score)
            .then_with(|| left.command.cmp(&right.command))
    });
    scored
        .into_iter()
        .map(|(_, suggestion)| suggestion)
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Kind;

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn record(command: &str) -> EntryRecord {
        EntryRecord {
            id: "c1".into(),
            revision: 1,
            kind: Kind::Command,
            name: "Existing".into(),
            entry_type: "Shell".into(),
            tags: vec![],
            description: String::new(),
            notes: String::new(),
            fields: vec![],
            pinned: false,
            command: command.into(),
            working_directory: String::new(),
            value: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn commands(suggestions: &[Suggestion]) -> Vec<&str> {
        suggestions.iter().map(|s| s.command.as_str()).collect()
    }

    // ---- zsh ---------------------------------------------------------------

    #[test]
    fn parses_extended_zsh_history_and_plain_lines() {
        let file = b": 1700000001:0;bun install\n: 1700000002:0;git push origin main\nbun test\n";
        assert_eq!(
            parse_zsh(file),
            lines(&["bun install", "git push origin main", "bun test"])
        );
    }

    #[test]
    fn keeps_a_command_that_really_starts_with_a_colon() {
        // Not a history prefix: the field after `: ` is not numeric, so the line stands as typed.
        assert_eq!(
            parse_zsh(b": () { : | :& };:\necho hi\n"),
            lines(&[": () { : | :& };:", "echo hi"])
        );
    }

    #[test]
    fn drops_metafied_bytes_without_splitting_the_record() {
        // `0x83 0x60` is a metafied '@'; `0x83 0x2a` is a metafied newline, which belongs *inside*
        // the command rather than ending the record.
        let file = b"echo \x83\x60host\nprintf a\x83\x2ab\n";
        assert_eq!(parse_zsh(file), lines(&["echo @host", "printf a\nb"]));
    }

    #[test]
    fn joins_zsh_continuation_lines_and_keeps_a_literal_backslash() {
        let file =
            b": 1:0;for f in *.rs; do \\\n  cargo fmt -- $f; \\\ndone\n: 2:0;printf 'x\\\\'\n";
        assert_eq!(
            parse_zsh(file),
            lines(&[
                "for f in *.rs; do \n  cargo fmt -- $f; \ndone",
                "printf 'x\\\\'"
            ])
        );
    }

    #[test]
    fn drops_blank_and_ignored_space_prefixed_lines() {
        let file = b": 1:0;\n\n: 2:0;  git commit -m wip\necho ok\n";
        assert_eq!(parse_zsh(file), lines(&["echo ok"]));
    }

    #[test]
    fn an_unterminated_continuation_at_the_end_of_the_file_still_counts() {
        assert_eq!(
            parse_zsh(b"make deploy \\\n  --profile prod"),
            lines(&["make deploy \n  --profile prod"])
        );
    }

    // ---- bash --------------------------------------------------------------

    #[test]
    fn parses_bash_history_with_and_without_timestamps() {
        let file = b"#1700000000\nbun run build\n#1700000060\ndocker compose up -d\n  git stash\n";
        assert_eq!(
            parse_bash(file),
            lines(&["bun run build", "docker compose up -d"])
        );
    }

    #[test]
    fn drops_bash_comments_as_well_as_timestamps() {
        assert_eq!(parse_bash(b"# not a timestamp\nmake ship\n").len(), 1);
        assert_eq!(parse_bash(b"# not a timestamp\n"), Vec::<String>::new());
    }

    // ---- fish --------------------------------------------------------------

    #[test]
    fn parses_fish_cmd_entries_and_skips_the_other_keys() {
        let file = b"- cmd: rg -n TODO\n  when: 1700000000\n- cmd: cargo test --locked\n  when: 1700000060\n  paths:\n    - /src/lib.rs\n";
        assert_eq!(
            parse_fish(file),
            lines(&["rg -n TODO", "cargo test --locked"])
        );
    }

    #[test]
    fn unescapes_fish_newlines_and_backslashes() {
        let file = b"- cmd: echo one\\ntwo\n- cmd: echo a\\\\b\n- cmd:  ls\n";
        assert_eq!(parse_fish(file), lines(&["echo one\ntwo", "echo a\\b"]));
    }

    // ---- normalise / key ---------------------------------------------------

    #[test]
    fn normalise_trims_and_collapses_whitespace() {
        assert_eq!(normalise("  docker   compose\n  up "), "docker compose up");
        assert_eq!(normalise("git\tstatus"), "git status");
        assert_eq!(normalise("   "), "");
    }

    #[test]
    fn key_is_the_sha256_of_the_normalised_command() {
        // Known digests, produced with `printf '%s' <text> | sha256sum`.
        assert_eq!(
            key_for(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            key_for("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            key_for("bun test"),
            "d8c272072687a75fab62bf32f66fc8948c86222dad85719db412b4d8bf0e50c7"
        );
        // The caller normalises first; the same line spelled differently hashes the same.
        assert_eq!(key_for("bun test"), key_for(&normalise("  bun\ttest  ")));
        assert_ne!(key_for("bun test"), key_for(&normalise("bun tests")));
    }

    // ---- secret filter -----------------------------------------------------

    #[test]
    fn rejects_named_credential_assignments() {
        for command in [
            "export GITHUB_TOKEN=ghp_abcdefghijklmnop",
            "AWS_SECRET_ACCESS_KEY=abc123 aws s3 ls",
            "export DB_PASSWORD=hunter2",
            "MY_API_KEY=xyzzy curl -s https://api.example.test",
            "export AUTH_HEADER=abc",
            "REGISTRY_KEY=1 docker login",
        ] {
            assert!(looks_like_secret(command), "missed: {command}");
        }
    }

    #[test]
    fn rejects_password_token_and_authority_shapes() {
        for command in [
            "mysql -uroot -pSuperSecret",
            "psql --password=hunter2 -h db.internal",
            "curl --token abc123 https://api.example.test",
            "curl -H 'Authorization: Basic Zm9vOmJhcg==' https://api.example.test",
            "curl -H 'Bearer abc.def.ghi' https://api.example.test",
            "git clone https://user:password@github.com/acme/private",
            "curl -u alice:s3cret https://internal.example.test",
            "psql -h db.internal --password",
        ] {
            assert!(looks_like_secret(command), "missed: {command}");
        }
    }

    #[test]
    fn rejects_a_token_as_the_whole_user_part_of_an_http_url() {
        for command in [
            "git clone https://ghp_abcdefghijklmnopqrstuvwxyz0123456789@github.com/acme/private",
            "git remote set-url origin https://glpat-abc123@gitlab.example.test/acme/app.git",
            "curl https://bot@registry.example.test/v2/",
            "git clone git+ssh://a0b1c2d3e4f5a6b7c8d9e0f1@example.test/acme/app",
        ] {
            assert!(looks_like_secret(command), "missed: {command}");
        }
    }

    #[test]
    fn rejects_a_password_given_as_the_word_after_dash_p() {
        for command in [
            "sshpass -p hunter2 ssh deploy@host.example.test",
            "docker login -u bot -p S3cretPass registry.example.test",
            "podman login registry.example.test -p hunter2",
            "mysql -u root -p hunter2 appdb",
            "/usr/bin/mysqldump -p hunter2 appdb",
        ] {
            assert!(looks_like_secret(command), "missed: {command}");
        }
    }

    #[test]
    fn rejects_credential_headers_flags_and_setting_names() {
        for command in [
            "curl -H 'X-Api-Key: abc123def456' https://api.example.test",
            "curl -H \"Private-Token: abc123\" https://gitlab.example.test/api",
            "curl --header=Cookie:session=abc https://app.example.test",
            "openai --api-key sk-short123 chat",
            "stripe login --api-key sk_test_123",
            "az login --client-secret abc --tenant t",
            "aws configure set aws_secret_access_key AbCd1234",
            "git config --global user.password hunter2",
            "vault write auth/token db-password=x",
        ] {
            assert!(looks_like_secret(command), "missed: {command}");
        }
    }

    #[test]
    fn rejects_long_high_entropy_arguments() {
        for command in [
            "op item get 1b2c3d4e5f60718293a4b5c6d7e8f9012345",
            "restic restore --tag 9f86d081884c7d65a90f051b18e8d1c8f2a5b7e9d0c1f3a5b7d9e1",
            "echo Q1RIRUFURVhBTVBMRTBmb3J0aHljaGFyc2VjcmV0",
        ] {
            assert!(looks_like_secret(command), "missed: {command}");
        }
    }

    #[test]
    fn accepts_harmless_lookalikes() {
        for command in [
            "git push --tags",
            "git push --follow-tags origin main",
            "docker compose up -d",
            "docker run -p 8080:80 nginx",
            "docker run -p8080:80 nginx",
            "docker compose -p payments up",
            "docker run -u 1000:1000 nginx",
            "cargo test --release",
            "bun run build --summarize",
            "ssh -N -L 5433:db.staging:5432 bastion",
            "curl -s https://example.test/releases/latest",
            "curl -s https://example.test/at/issue",
            "curl -u alice https://api.example.test",
            "git checkout a1b2c34",
            "pnpm add --save-exact svelte@5.0.0",
            "export PATH=$HOME/.local/bin:$PATH",
            "make deploy staging",
            "tar -czf build.tar.gz dist",
            "rg --hidden --no-ignore TODO src",
            // A port after `-p`, where `-p` is not a password option.
            "ssh -p 2222 deploy@host.example.test",
            "kubectl port-forward svc/api 8080:80 -n staging",
            // A user name in a URL that is not http(s), and a non-secret header.
            "git clone ssh://git@github.com/acme/app.git",
            "psql postgres://app@db.internal/app",
            "curl -H 'Content-Type: application/json' https://api.example.test",
            // The resource kind, not a credential.
            "kubectl get secret -n staging",
            "kubectl describe secret db-password -n staging",
            // A full commit id where git expects a revision.
            "git checkout 3f2a9c1e5b7d4a6f8e0c2b4d6f8a0c2e4b6d8f0a",
            "git show 9f86d081884c7d65a90f051b18e8d1c8f2a5b7e9d0c1f3a5b7d9e1f3a5b7d9e1",
        ] {
            assert!(!looks_like_secret(command), "false positive: {command}");
        }
    }

    #[test]
    fn a_long_hex_value_outside_a_git_revision_is_still_a_secret() {
        // Old GitHub tokens are 40 hex characters too: only git's revision slots are excused.
        assert!(looks_like_secret(
            "gh auth login --hostname example.test 3f2a9c1e5b7d4a6f8e0c2b4d6f8a0c2e4b6d8f0a"
        ));
        assert!(looks_like_secret(
            "git config http.extraheader 3f2a9c1e5b7d4a6f8e0c2b4d6f8a0c2e4b6d8f0a"
        ));
    }

    // ---- naming ------------------------------------------------------------

    #[test]
    fn suggests_a_name_from_the_first_two_words_and_the_first_argument() {
        assert_eq!(
            suggested_name("docker compose -f staging.yml up -d"),
            "docker-compose-up"
        );
        assert_eq!(suggested_name("git push origin main"), "git-push-origin");
        assert_eq!(suggested_name("bun run test:watch"), "bun-run-test:watch");
        assert_eq!(suggested_name("make deploy"), "make-deploy");
        assert_eq!(
            suggested_name("kubectl -n prod rollout restart deploy/api"),
            "kubectl-rollout-restart"
        );
        assert_eq!(
            suggested_name("rsync -az build staging"),
            "rsync-build-staging"
        );
    }

    #[test]
    fn a_name_never_carries_a_quoted_value_or_a_long_token() {
        assert_eq!(
            suggested_name("git commit -m \"a very long message indeed here\""),
            "git-commit"
        );
        assert_eq!(
            suggested_name("rsync -az ./build/ user@host:/srv/app/releases/aaaaaaaaaaaaaa"),
            "rsync"
        );
        assert!(suggested_name(&format!("head -n 1 {}", "a".repeat(40))).len() <= 60);
    }

    // ---- ranking and filtering --------------------------------------------

    #[test]
    fn counts_and_recency_rank_a_frequent_recent_command_first() {
        let history = lines(&[
            "bun test",
            "docker compose up",
            "bun test",
            "docker compose up",
            "bun test",
            "rsync -az build staging",
            "rsync -az build staging",
            "rsync -az build staging",
            "rsync -az build staging",
            "docker compose up",
        ]);
        let suggestions = suggest(&history, &[], &HashSet::new(), 5);
        assert_eq!(
            commands(&suggestions),
            vec!["rsync -az build staging", "docker compose up", "bun test"]
        );
        assert_eq!(suggestions[0].count, 4);
        assert_eq!(suggestions[1].count, 3);
        assert_eq!(suggestions[0].suggested_name, "rsync-build-staging");
    }

    #[test]
    fn recency_beats_an_equal_count_and_loses_to_a_much_bigger_one() {
        let mut history = lines(&[
            "make deploy",
            "make deploy",
            "make deploy",
            "make deploy",
            "make deploy",
            "make deploy",
            "bun run lint",
        ]);
        history.extend(lines(&["bun run lint", "bun run lint"]));
        let suggestions = suggest(&history, &[], &HashSet::new(), 2);
        // Six uses of an older command still outrank three of a recent one: recency weights, it
        // does not override.
        assert_eq!(suggestions[0].command, "make deploy");
        assert_eq!(suggestions[0].count, 6);

        let same_counts = lines(&[
            "alpha tool run",
            "beta tool run",
            "gamma tool run",
            "beta tool run",
        ]);
        let suggestions = suggest(&same_counts, &[], &HashSet::new(), 3);
        assert_eq!(
            commands(&suggestions),
            vec!["beta tool run", "gamma tool run", "alpha tool run"]
        );
    }

    #[test]
    fn an_exact_score_tie_breaks_on_the_command_text() {
        // Three uses ending early score exactly the same as two uses ending last (8 lines).
        let history = lines(&[
            "zebra tool run",
            "zebra tool run",
            "zebra tool run",
            "fill one run",
            "fill two run",
            "fill three",
            "alpha tool run",
            "alpha tool run",
        ]);
        let suggestions = suggest(&history, &[], &HashSet::new(), 2);
        assert_eq!(
            commands(&suggestions),
            vec!["alpha tool run", "zebra tool run"]
        );
        assert_eq!(suggest(&history, &[], &HashSet::new(), 2), suggestions);
    }

    #[test]
    fn drops_trivial_and_denied_commands() {
        let history = lines(&[
            "ls",
            "pwd",
            "git status",
            "git  status",
            "clear",
            "ls -la",
            "ls -la",
            "echo hi",
            "echo hi",
        ]);
        let suggestions = suggest(&history, &[], &HashSet::new(), 5);
        assert_eq!(commands(&suggestions), vec!["echo hi", "ls -la"]);
    }

    #[test]
    fn skips_commands_already_in_the_library_whatever_their_spacing() {
        let history = lines(&[
            "docker compose up -d",
            "docker compose up -d",
            "bun run dev",
        ]);
        let saved = vec![record("docker   compose up -d")];
        let suggestions = suggest(&history, &saved, &HashSet::new(), 5);
        assert_eq!(commands(&suggestions), vec!["bun run dev"]);
    }

    #[test]
    fn dismissed_suggestions_are_matched_by_hash_not_by_spacing() {
        let history = lines(&[
            "bun test --watch",
            "bun test --watch",
            "cargo clippy --locked",
        ]);
        let dismissed: HashSet<String> = [key_for(&normalise("bun  test\t--watch"))]
            .into_iter()
            .collect();
        let suggestions = suggest(&history, &[], &dismissed, 5);
        assert_eq!(commands(&suggestions), vec!["cargo clippy --locked"]);
        assert!(suggestions
            .iter()
            .all(|suggestion| suggestion.key != key_for("bun test --watch")));
    }

    #[test]
    fn never_offers_a_command_that_looks_like_a_credential() {
        let history = lines(&[
            "export GITHUB_TOKEN=ghp_abcdefghijklmnop",
            "mysql -uroot -pSuperSecret",
            "docker compose up",
            "docker compose up",
        ]);
        let suggestions = suggest(&history, &[], &HashSet::new(), 5);
        assert_eq!(commands(&suggestions), vec!["docker compose up"]);
    }

    #[test]
    fn honours_the_limit_and_ignores_an_empty_history() {
        let history = lines(&[
            "a1 tool run one",
            "a2 tool run two",
            "a3 tool run three",
            "a4 tool run four",
            "a5 tool run five",
            "a6 tool run six",
        ]);
        assert_eq!(suggest(&history, &[], &HashSet::new(), 3).len(), 3);
        assert_eq!(suggest(&history, &[], &HashSet::new(), 0).len(), 0);
        assert!(suggest(&[], &[record("make ship")], &HashSet::new(), 5).is_empty());
    }

    #[test]
    fn counts_one_command_whatever_how_it_was_spaced() {
        let history = lines(&[
            "terraform plan -out plan",
            "terraform  plan  -out  plan",
            "terraform plan -out  plan",
        ]);
        let suggestions = suggest(&history, &[], &HashSet::new(), 1);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].count, 3);
        // The newest spelling is what gets offered, spacing and all.
        assert_eq!(suggestions[0].command, "terraform plan -out  plan");
    }

    // ---- locating ----------------------------------------------------------

    #[test]
    fn histfile_replaces_the_zsh_default_and_a_missing_file_is_no_source() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("home");
        let zsh = home.join(".zsh_history");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::write(&zsh, b"make ship\n").expect("write zsh");

        let sources = locate_with(|_| None, &home);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].shell, Shell::Zsh);
        assert_eq!(sources[0].path, zsh);

        let pointed = temp.path().join("custom-history");
        std::fs::write(&pointed, b"bun test\n").expect("write histfile");
        let sources = locate_with(
            |name| (name == "HISTFILE").then(|| pointed.to_string_lossy().to_string()),
            &home,
        );
        assert_eq!(sources.len(), 1, "$HISTFILE replaces the zsh default");
        assert_eq!(sources[0].path, pointed);
        assert_eq!(collect(&sources), lines(&["bun test"]));
    }

    #[test]
    fn every_shell_contributes_and_the_order_is_stable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("home");
        let fish = home.join(".local/share/fish");
        std::fs::create_dir_all(&fish).expect("dirs");
        std::fs::write(home.join(".zsh_history"), b"zsh one\nzsh two\n").expect("zsh");
        std::fs::write(home.join(".bash_history"), b"bash one\n").expect("bash");
        std::fs::write(fish.join("fish_history"), b"- cmd: fish one\n").expect("fish");
        // The same modification time everywhere, so the order below is the tie-break alone.
        let same =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for file in [
            home.join(".zsh_history"),
            home.join(".bash_history"),
            fish.join("fish_history"),
        ] {
            set_modified(&file, same);
        }

        let sources = locate_with(|_| None, &home);
        assert_eq!(
            sources
                .iter()
                .map(|source| source.shell)
                .collect::<Vec<_>>(),
            vec![Shell::Zsh, Shell::Bash, Shell::Fish]
        );
        assert_eq!(
            collect(&sources),
            lines(&["zsh one", "zsh two", "bash one", "fish one"])
        );
    }

    fn set_modified(path: &Path, time: std::time::SystemTime) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .expect("open")
            .set_modified(time)
            .expect("set mtime");
    }

    #[test]
    fn the_history_written_most_recently_is_read_last_and_ranks_as_recent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).expect("home");
        // An old bash history read after a live zsh one would hand its stale commands the top
        // recency weight. Ordered by when each was last written, the live one comes last.
        std::fs::write(home.join(".bash_history"), b"make old-thing\n").expect("bash");
        std::fs::write(home.join(".zsh_history"), b"make new-thing\n").expect("zsh");
        let epoch = std::time::SystemTime::UNIX_EPOCH;
        set_modified(
            &home.join(".bash_history"),
            epoch + std::time::Duration::from_secs(1_600_000_000),
        );
        set_modified(
            &home.join(".zsh_history"),
            epoch + std::time::Duration::from_secs(1_700_000_000),
        );

        let sources = locate_with(|_| None, &home);
        assert_eq!(
            sources
                .iter()
                .map(|source| source.shell)
                .collect::<Vec<_>>(),
            vec![Shell::Bash, Shell::Zsh]
        );
        let ranked = suggest(&collect(&sources), &[], &HashSet::new(), 5);
        assert_eq!(
            ranked
                .iter()
                .map(|suggestion| suggestion.command.as_str())
                .collect::<Vec<_>>(),
            vec!["make new-thing", "make old-thing"],
            "equal counts: the command from the history in use today wins"
        );
    }

    #[test]
    fn a_history_file_that_is_missing_or_not_a_file_yields_nothing() {
        let missing = HistorySource {
            shell: Shell::Zsh,
            path: std::path::PathBuf::from("/nonexistent/dev-console-history"),
        };
        assert!(missing.read().is_empty());
        let directory = HistorySource {
            shell: Shell::Bash,
            path: std::env::temp_dir(),
        };
        assert!(directory.read().is_empty());
    }
}
