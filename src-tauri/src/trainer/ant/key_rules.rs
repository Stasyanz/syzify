//! The ANT+ network key's rules: its hash, how a written key reads, and
//! when a build must stop. Compiled twice: into the app (through
//! [`key`](super::key)) and into build.rs (`#[path]`) — so std, sha2 and hex
//! only: no `crate::`, and no `option_env!` (the build script's dep-info
//! would keep the key's value).

use std::ffi::OsString;

use sha2::{Digest, Sha256};

/// SHA-256 of the key's eight bytes.
pub const KEY_SHA256: &str = "ba53300909ed532d17a25cad3c433266e2864ee5db97d6424b9021fc3334a774";

/// The build variable that carries the key.
pub const KEY_VAR: &str = "SYZIFY_ANT_KEY";
/// The build variable that makes the key required (a release): any
/// non-empty value.
pub const REQUIRED_VAR: &str = "SYZIFY_ANT_KEY_REQUIRED";

/// Eight bytes from 16 hex digits; spaces, colons, commas and a `0x` per
/// byte are left out.
pub fn parse_key(text: &str) -> Option<[u8; 8]> {
    let digits: String = text
        .split(|c: char| c.is_whitespace() || c == ':' || c == ',')
        .map(|part| part.trim_start_matches("0x").trim_start_matches("0X"))
        .collect();
    if digits.len() != 16 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut key = [0u8; 8];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(key)
}

/// Whether `key` is the key whose hash is `sha`.
pub fn matches(key: &[u8; 8], sha: &str) -> bool {
    hex::encode(Sha256::digest(key)) == sha
}

/// A key the user typed in, as it is kept, if it is the key whose hash is
/// `sha` (`KEY_SHA256`; tests give their own).
pub fn checked(text: &str, sha: &str) -> Option<String> {
    parse_key(text).filter(|k| matches(k, sha)).map(hex::encode)
}

/// Why a build must stop, if it must, reading its variables through `var`:
/// a key that is not the one whose hash is `sha` (not text at all
/// included), or none where one is required. Never says the value. A
/// blank key is none, as the app reads it.
pub fn build_problem(var: impl Fn(&str) -> Option<OsString>, sha: &str) -> Option<&'static str> {
    let required = var(REQUIRED_VAR).is_some_and(|v| !v.is_empty());
    let given = var(KEY_VAR).filter(|v| !v.to_string_lossy().trim().is_empty());
    match given.map(|v| v.to_str().and_then(|text| checked(text, sha))) {
        Some(Some(_)) => None,
        Some(None) => Some("SYZIFY_ANT_KEY is set but is not the ANT+ network key"),
        None if required => Some("SYZIFY_ANT_KEY_REQUIRED is set but SYZIFY_ANT_KEY is not: a release needs the ANT+ network key"),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMMY: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];

    /// A build environment holding `key` and `required`.
    fn env(key: Option<OsString>, required: Option<&'static str>) -> impl Fn(&str) -> Option<OsString> {
        move |name| {
            assert!([KEY_VAR, REQUIRED_VAR].contains(&name), "read {name}");
            if name == KEY_VAR { key.clone() } else { required.map(OsString::from) }
        }
    }

    fn text(s: &str) -> Option<OsString> {
        Some(OsString::from(s))
    }

    /// The build stops on a key that is not the key, and on none in a
    /// release; a build without a key otherwise goes on.
    #[test]
    fn a_build_stops_on_a_wrong_key_or_a_release_without_one() {
        let sha = hex::encode(Sha256::digest(DUMMY));
        let goes_on = [
            (text("0123456789abcdef"), Some("1")),
            (text("01 23 45 67 89 AB CD EF\r\n"), None),
            (None, None),
            (None, Some("")),
            (text(" \n"), None),
            (text(""), Some("")),
        ];
        for (key, required) in goes_on {
            assert_eq!(build_problem(env(key.clone(), required), &sha), None, "{key:?} required {required:?}");
        }
        for (key, required) in [(text("0123456789abcdee"), None), (text("0123456789abcdee"), Some("1")), (text("\"0123456789abcdef\""), None)] {
            let problem = build_problem(env(key.clone(), required), &sha).expect("stops");
            assert!(problem.contains("not the ANT+ network key"), "{key:?}: {problem}");
        }
        for (key, required) in [(None, "1"), (text(""), "1"), (text("  "), "true"), (None, "0")] {
            let problem = build_problem(env(key.clone(), Some(required)), &sha).expect("stops");
            assert!(problem.contains("a release needs"), "{key:?} {required}: {problem}");
        }
    }

    /// This file is compiled into build.rs too: nothing of the crate, and
    /// no `option_env!` (the build script's dep-info would keep the key).
    #[test]
    fn the_rules_stand_alone() {
        let source = include_str!("key_rules.rs");
        let (_, code) = source.split_once("\nuse ").expect("past the header");
        let (code, _) = code.split_once("#[cfg(test)]").expect("before the tests");
        for not in ["option_env!(", "env!(", "crate::", "super::"] {
            assert!(!code.contains(not), "{not} in key_rules.rs");
        }
    }

    /// A key that is not text is set, and not the key.
    #[cfg(unix)]
    #[test]
    fn a_key_that_is_not_text_is_not_the_key() {
        use std::os::unix::ffi::OsStringExt;
        let sha = hex::encode(Sha256::digest(DUMMY));
        let bytes = OsString::from_vec(vec![0x30, 0xFF, 0x31]);
        let problem = build_problem(env(Some(bytes), Some("1")), &sha).expect("stops");
        assert!(problem.contains("not the ANT+ network key"), "{problem}");
    }
}
