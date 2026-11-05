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
            let file = iter.next().map(|s| s.as_ref().to_string());
            match (kind.as_deref(), file) {
                (Some("bitwarden"), Some(file)) => Intent::ImportBitwarden(file),
                _ => Intent::Help,
            }
        }
        // Unknown flags must never be interpreted as "run something".
        _ => Intent::Help,
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
}
