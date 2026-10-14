//! The token the browser UI proves itself with.
//!
//! It is kept in your configuration directory, readable by you only, so a
//! bookmarked UI keeps working after a restart.

use std::fs;
use std::path::PathBuf;

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("getman").join("ui-token"))
}

/// The saved token, or a new random one saved for next time.
pub(crate) fn load_or_create() -> Result<String, String> {
    let Some(path) = path() else {
        return generate();
    };
    if let Ok(saved) = fs::read_to_string(&path) {
        let saved = saved.trim();
        if saved.len() >= 32 && saved.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(saved.to_owned());
        }
    }
    let token = generate()?;
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if fs::write(&path, &token).is_ok() {
        restrict(&path);
    }
    Ok(token)
}

fn generate() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("cannot generate a token: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(unix)]
fn restrict(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_path: &std::path::Path) {
    // The per-user configuration directory is private to the user.
}

/// Compares tokens in constant time.
pub(crate) fn matches(given: &str, expected: &str) -> bool {
    if given.len() != expected.len() || expected.is_empty() {
        return false;
    }
    given
        .bytes()
        .zip(expected.bytes())
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_and_random() {
        let a = generate().unwrap();
        let b = generate().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn only_the_same_token_matches() {
        assert!(matches("abc", "abc"));
        assert!(!matches("abd", "abc"));
        assert!(!matches("ab", "abc"));
        assert!(!matches("", ""));
    }
}
