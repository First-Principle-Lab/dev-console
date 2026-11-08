//! POSIX-style word splitting for the terminal command setting.
//!
//! Ported from `src/lib/argv.ts`, which plan §3 lists as TypeScript-only logic that has to move to
//! Rust ("port TypeScript-only drafts, validation, argument parsing, formatting, and Finder
//! selection logic into Rust"). The setting is stored as `Vec<String>` (`Settings::terminal`), so
//! the split is a *storage* concern, not only a rendering one: the settings field is the text form
//! of the argv the executor builds in `execution::build_argv`.
//!
//! The behaviour of the old UI splitter (`value.split(' ')`) is what this replaces, and the reason
//! it exists at all is recorded in the TypeScript it ports: it broke any launcher whose program or
//! arguments contain spaces (`/usr/bin/my terminal -e`) and silently mangled quoted arguments
//! (`foot --title "Dev Console" -e` became four nonsense entries). Unbalanced quoting is an error
//! the UI shows, never a silent guess.
//!
//! Handled exactly as the ported code handles it: single quotes take their contents literally,
//! double quotes allow `\\` to escape the next character, a backslash outside quotes escapes the
//! next character, and an unbalanced quote is [`AppError::Validation`] whose message is the same
//! text the webview showed (`unbalanced single quote` / `unbalanced double quote`).
//!
//! The tests below are the ported `tests/unit/argv.test.ts` cases, named after them.

use crate::error::{AppError, AppResult};

/// Splits one input line into argv words.
///
/// Whitespace here is the set the ported splitter used: space, tab and newline. A carriage return
/// is *not* a separator (it is part of the word), matching `argv.ts` rather than a shell.
pub fn parse_argv(input: &str) -> AppResult<Vec<String>> {
    let chars: Vec<char> = input.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    // `started` distinguishes "an empty word was quoted into existence" (`a "" b`) from "nothing
    // has been typed yet", which is why an empty quoted word survives and stray spaces do not.
    let mut started = false;
    let mut index = 0usize;

    while index < chars.len() {
        let char = chars[index];
        if char == ' ' || char == '\t' || char == '\n' {
            if started {
                words.push(std::mem::take(&mut current));
                started = false;
            }
            index += 1;
            continue;
        }
        if char == '\'' {
            started = true;
            index += 1;
            while index < chars.len() && chars[index] != '\'' {
                current.push(chars[index]);
                index += 1;
            }
            if index >= chars.len() {
                return Err(AppError::Validation("unbalanced single quote".into()));
            }
            index += 1;
            continue;
        }
        if char == '"' {
            started = true;
            index += 1;
            while index < chars.len() && chars[index] != '"' {
                if chars[index] == '\\' && index + 1 < chars.len() {
                    index += 1;
                }
                current.push(chars[index]);
                index += 1;
            }
            if index >= chars.len() {
                return Err(AppError::Validation("unbalanced double quote".into()));
            }
            index += 1;
            continue;
        }
        if char == '\\' && index + 1 < chars.len() {
            started = true;
            index += 1;
            current.push(chars[index]);
            index += 1;
            continue;
        }
        started = true;
        current.push(char);
        index += 1;
    }

    if started {
        words.push(current);
    }
    Ok(words)
}

/// True when the word cannot be shown in a single input field without ambiguity, so
/// [`format_argv`] has to quote it. An empty word needs quotes to exist at all.
pub fn needs_quoting(word: &str) -> bool {
    word.is_empty()
        || word
            .chars()
            .any(|char| char.is_whitespace() || char == '\'' || char == '"')
}

/// Renders argv back into an input field: empty words become `""`, spaced words get quoted.
///
/// [`parse_argv`] reads this back, which the ported round-trip test asserts.
pub fn format_argv<S: AsRef<str>>(argv: &[S]) -> String {
    argv.iter()
        .map(|word| {
            let word = word.as_ref();
            if needs_quoting(word) {
                format!("\"{}\"", word.replace('"', "\\\""))
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `parseArgv > splits plain words like the old splitter did`
    #[test]
    fn splits_plain_words_like_the_old_splitter_did() {
        assert_eq!(
            parse_argv("x-terminal-emulator -e").unwrap(),
            ["x-terminal-emulator", "-e"]
        );
        assert_eq!(
            parse_argv("  spaced   out  \ttabs\t").unwrap(),
            ["spaced", "out", "tabs"]
        );
        assert_eq!(parse_argv("").unwrap(), Vec::<String>::new());
    }

    /// `parseArgv > keeps paths and arguments that contain spaces when quoted`
    #[test]
    fn keeps_paths_and_arguments_that_contain_spaces_when_quoted() {
        assert_eq!(
            parse_argv("\"/usr/local/bin/my terminal\" -e").unwrap(),
            ["/usr/local/bin/my terminal", "-e"]
        );
        assert_eq!(
            parse_argv("'/home/me/My Terminal.app/Contents/MacOS/run' --wait").unwrap(),
            ["/home/me/My Terminal.app/Contents/MacOS/run", "--wait"]
        );
        assert_eq!(
            parse_argv("foot --title \"Dev Console\" -e").unwrap(),
            ["foot", "--title", "Dev Console", "-e"]
        );
    }

    /// `parseArgv > supports escapes and empty quoted words`
    #[test]
    fn supports_escapes_and_empty_quoted_words() {
        assert_eq!(
            parse_argv("run\\ with\\ spaces --x").unwrap(),
            ["run with spaces", "--x"]
        );
        assert_eq!(parse_argv("a \"\" b").unwrap(), ["a", "", "b"]);
        assert_eq!(
            parse_argv("\"say \\\"hi\\\"\" -e").unwrap(),
            ["say \"hi\"", "-e"]
        );
    }

    /// `parseArgv > rejects unbalanced quotes instead of guessing`
    #[test]
    fn rejects_unbalanced_quotes_instead_of_guessing() {
        let double = parse_argv("foot --title \"broken").unwrap_err();
        assert_eq!(double.to_string(), "unbalanced double quote");
        assert_eq!(double.code(), "validation");
        let single = parse_argv("alacritty -e 'broken").unwrap_err();
        assert_eq!(single.to_string(), "unbalanced single quote");
    }

    /// `parseArgv > round-trips argv through the input field`
    #[test]
    fn round_trips_argv_through_the_input_field() {
        let argv = [
            "/usr/local/bin/my terminal".to_string(),
            "--title".to_string(),
            "Dev Console".to_string(),
            "-e".to_string(),
        ];
        assert_eq!(parse_argv(&format_argv(&argv)).unwrap(), argv);
    }

    #[test]
    fn only_ambiguous_words_get_quoted() {
        assert!(needs_quoting(""));
        assert!(needs_quoting("my terminal"));
        assert!(needs_quoting("it's"));
        assert!(needs_quoting("say \"hi\""));
        assert!(!needs_quoting("x-terminal-emulator"));
        assert!(!needs_quoting("-e"));
        // The empty word has to keep its quotes, or the round trip loses an argv entry.
        assert_eq!(format_argv(&["a", "", "b"]), "a \"\" b");
        assert_eq!(parse_argv("a \"\" b").unwrap(), ["a", "", "b"]);
    }
}
