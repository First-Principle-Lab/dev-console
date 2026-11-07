#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum VaultCommand {
    Lock,
    Unlock,
    Status,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Intent {
    /// `dev-console` / `dev-console --library`
    Library,
    /// `dev-console --background`
    Background,
    /// `dev-console --finder`
    Finder,
    Vault(VaultCommand),
    /// `dev-console import bitwarden <file>`
    ImportBitwarden(String),
    /// `dev-console import project <dir>`: list what the folder defines, save nothing.
    ScanProject {
        dir: String,
    },
    /// `dev-console import project <dir> --all`: save everything the folder defines.
    ImportProject {
        dir: String,
    },
    HistorySetup,
    Help,
    Version,
}

pub const USAGE: &str = "\
dev-console — local commands and secrets console

Usage:
  dev-console                  open the library window
  dev-console --library        open the library window
  dev-console --background     start resident in the tray without a window
  dev-console --finder         toggle the search finder
  dev-console vault lock       manually lock the vault (persists until authenticated unlock)
  dev-console vault unlock     unlock with system authentication
  dev-console vault status     print vault state (never prints secret data)
  dev-console import bitwarden <file>
                               import a plain Bitwarden JSON export as secrets
  dev-console import project <dir>
                               list the runnable tasks a project folder defines (package.json,
                               Makefile, justfile, compose.yaml) and import nothing
  dev-console import project <dir> --all
                               import every task that folder defines
  dev-console history setup    enable project-aware tracking for new zsh terminals
  dev-console --version
  dev-console --help
";

pub fn parse<I, S>(args: I) -> Intent
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let Some(first) = iter.next() else {
        return Intent::Library;
    };
    let first = first.as_ref();
    match first {
        "--help" | "-h" => Intent::Help,
        "--version" | "-V" => Intent::Version,
        "--library" => Intent::Library,
        "--background" => Intent::Background,
        "--finder" | "-f" => Intent::Finder,
        "history" if iter.next().is_some_and(|s| s.as_ref() == "setup") => Intent::HistorySetup,
        "vault" => {
            let second = iter
                .next()
                .map(|s| s.as_ref().to_string())
                .unwrap_or_default();
            match second.as_str() {
                "lock" => Intent::Vault(VaultCommand::Lock),
                "unlock" => Intent::Vault(VaultCommand::Unlock),
                "status" | "" => Intent::Vault(VaultCommand::Status),
                _ => Intent::Help,
            }
        }
        "import" => {
            let kind = iter.next().map(|s| s.as_ref().to_string());
            match kind.as_deref() {
                Some("bitwarden") => match iter.next() {
                    Some(file) => Intent::ImportBitwarden(file.as_ref().to_string()),
                    None => Intent::Help,
                },
                Some("project") => parse_project(iter),
                _ => Intent::Help,
            }
        }
        // Unknown flags must never be interpreted as "run something".
        _ => Intent::Help,
    }
}

/// `import project` arguments, in either order: one folder, and an optional `--all`.
fn parse_project<I, S>(iter: I) -> Intent
where
    I: Iterator<Item = S>,
    S: AsRef<str>,
{
    let mut dir: Option<String> = None;
    let mut all = false;
    for arg in iter {
        let arg = arg.as_ref();
        if arg == "--all" {
            all = true;
        } else if arg.starts_with('-') || dir.as_deref().is_some_and(|d| !d.is_empty()) {
            // An unknown flag, or a second folder: this is not an invocation this version
            // understands, so show the usage rather than guess which part was meant.
            return Intent::Help;
        } else {
            dir = Some(arg.to_string());
        }
    }
    // Without a folder the request is ambiguous — it looks like "scan where I am standing" — and
    // `.` is one character to type, so this is refused rather than defaulted.
    match dir.filter(|d| !d.trim().is_empty()) {
        Some(dir) if all => Intent::ImportProject { dir },
        Some(dir) => Intent::ScanProject { dir },
        None => Intent::Help,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_documented_invocation() {
        assert_eq!(parse(Vec::<String>::new()), Intent::Library);
        assert_eq!(parse(["--library"]), Intent::Library);
        assert_eq!(parse(["--background"]), Intent::Background);
        assert_eq!(parse(["--finder"]), Intent::Finder);
        assert_eq!(parse(["vault", "lock"]), Intent::Vault(VaultCommand::Lock));
        assert_eq!(
            parse(["vault", "unlock"]),
            Intent::Vault(VaultCommand::Unlock)
        );
        assert_eq!(
            parse(["vault", "status"]),
            Intent::Vault(VaultCommand::Status)
        );
        assert_eq!(parse(["--version"]), Intent::Version);
        assert_eq!(parse(["--nope"]), Intent::Help);
        assert_eq!(parse(["vault", "nuke"]), Intent::Help);
    }

    #[test]
    fn parses_the_import_subcommands() {
        assert_eq!(
            parse(["import", "bitwarden", "/tmp/export.json"]),
            Intent::ImportBitwarden("/tmp/export.json".into())
        );
        assert_eq!(
            parse(["import", "project", "~/src/api"]),
            Intent::ScanProject {
                dir: "~/src/api".into()
            }
        );
        assert_eq!(
            parse(["import", "project", ".", "--all"]),
            Intent::ImportProject { dir: ".".into() }
        );
        // The flag may come first; it is the same request either way.
        assert_eq!(
            parse(["import", "project", "--all", "/srv/app"]),
            Intent::ImportProject {
                dir: "/srv/app".into()
            }
        );
    }

    #[test]
    fn an_import_without_its_argument_is_the_usage() {
        assert_eq!(parse(["import"]), Intent::Help);
        assert_eq!(parse(["import", "bitwarden"]), Intent::Help);
        assert_eq!(parse(["import", "svn", "/tmp"]), Intent::Help);
        // `project` with no folder is not a licence to scan the current directory.
        assert_eq!(parse(["import", "project"]), Intent::Help);
        assert_eq!(parse(["import", "project", ""]), Intent::Help);
        assert_eq!(parse(["import", "project", "  ", "--all"]), Intent::Help);
        assert_eq!(parse(["import", "project", "a", "b"]), Intent::Help);
        assert_eq!(parse(["import", "project", "--all"]), Intent::Help);
        assert_eq!(parse(["import", "project", "--everything"]), Intent::Help);
    }
}
