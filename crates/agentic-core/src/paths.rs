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
