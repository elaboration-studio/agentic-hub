use std::path::PathBuf;

/// The user's home directory, resolved from the environment.
///
/// We avoid the deprecated `std::env::home_dir` and keep the dependency surface
/// minimal. Windows is not exercised in v1 but the `USERPROFILE` fallback keeps
/// the contract honest.
pub fn home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home);
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        if !profile.is_empty() {
            return PathBuf::from(profile);
        }
    }
    PathBuf::from(".")
}

/// Expand a leading `~` / `~/` to the home directory. Any other input is taken
/// as-is. Used when reading user-authored path strings from settings.
pub fn expand_tilde(input: &str) -> PathBuf {
    if input == "~" {
        return home_dir();
    }
    if let Some(rest) = input.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(input)
}

/// Best-effort one-level backup of a dotfile before it is overwritten. Copies
/// `path` to `<path>.bak` only when the file exists and is non-empty, so an
/// accidental clobber (a botched git merge resolved while the app is open, a
/// mistaken edit) is recoverable without reaching for `git checkout`. The empty
/// guard means a freshly-emptied file never overwrites a good backup. Never
/// errors the caller: a failed backup must not block the real write.
pub fn back_up_dotfile(path: &std::path::Path) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if !meta.is_file() || meta.len() == 0 {
        return;
    }
    let mut bak = path.as_os_str().to_os_string();
    bak.push(".bak");
    let _ = std::fs::copy(path, std::path::Path::new(&bak));
}

/// Render a path home-relative (`~/…`) for display. Inverse of [`expand_tilde`].
/// Paths outside the home directory are returned unchanged.
pub fn tildify(path: &std::path::Path) -> String {
    let home = home_dir();
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy().replace('\\', "/")),
        Err(_) => path.to_string_lossy().replace('\\', "/"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_takes_plain_paths_literally() {
        assert_eq!(expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
        assert_eq!(expand_tilde("relative/x"), PathBuf::from("relative/x"));
        // `~user` is not the home shortcut — kept verbatim.
        assert_eq!(expand_tilde("~user"), PathBuf::from("~user"));
    }

    #[test]
    fn expand_tilde_resolves_home() {
        // Computed against home_dir() so the assertion is independent of the
        // actual HOME value in the test environment.
        assert_eq!(expand_tilde("~"), home_dir());
        assert_eq!(expand_tilde("~/a/b"), home_dir().join("a/b"));
    }

    #[test]
    fn tildify_renders_home_relative_and_round_trips() {
        let p = home_dir().join("proj/skills");
        assert_eq!(tildify(&p), "~/proj/skills");
        assert_eq!(tildify(&home_dir()), "~");
        // tildify is the inverse of expand_tilde under the home directory.
        assert_eq!(expand_tilde(&tildify(&p)), p);
    }
}
