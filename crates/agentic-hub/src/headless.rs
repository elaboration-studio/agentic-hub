//! The headless `ehub` entry: decides from argv whether this process is the
//! CLI, and runs it without touching Tauri. See
//! `docs/tech/reference/ehub-cli-contract.md`.

use std::io::Write;
use std::path::Path;

/// The words after `ehub` when argv asks for the CLI: invoked as `ehub`
/// (the `~/.agentic-hub/bin/ehub` symlink) or as `<app> ehub ...`.
pub fn ehub_args(argv: &[String]) -> Option<Vec<String>> {
    let program = argv.first()?;
    let basename = Path::new(program).file_name()?.to_string_lossy();
    if basename == "ehub" || basename == "ehub.exe" {
        return Some(argv[1..].to_vec());
    }
    (argv.get(1).map(String::as_str) == Some("ehub")).then(|| argv[2..].to_vec())
}

/// Run the CLI against stdout and return the contract exit code.
pub fn run(args: &[String]) -> i32 {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let code = agentic_core::cli::run(args, &mut out);
    if out.flush().is_err() && code == 0 {
        return 1;
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_string()).collect()
    }

    #[test]
    fn symlink_basename_takes_every_following_word() {
        let got = ehub_args(&argv(&["/Users/a/.agentic-hub/bin/ehub", "agents", "list"]));
        assert_eq!(got, Some(argv(&["agents", "list"])));
    }

    #[test]
    fn bare_ehub_with_no_words_is_headless() {
        assert_eq!(ehub_args(&argv(&["ehub"])), Some(vec![]));
    }

    #[test]
    fn app_binary_with_ehub_subcommand_skips_both() {
        let got = ehub_args(&argv(&["/Applications/Agentic Hub.app/Contents/MacOS/agentic-hub", "ehub", "version"]));
        assert_eq!(got, Some(argv(&["version"])));
    }

    #[test]
    fn app_binary_without_ehub_launches_gui() {
        assert_eq!(ehub_args(&argv(&["agentic-hub"])), None);
        assert_eq!(ehub_args(&argv(&["agentic-hub", "--flag"])), None);
    }

    #[test]
    fn ehub_later_in_argv_is_not_headless() {
        assert_eq!(ehub_args(&argv(&["agentic-hub", "--x", "ehub"])), None);
    }

    #[test]
    fn basename_containing_ehub_is_not_headless() {
        assert_eq!(ehub_args(&argv(&["/bin/not-ehub"])), None);
    }

    #[test]
    fn empty_argv_launches_gui() {
        assert_eq!(ehub_args(&[]), None);
    }
}
