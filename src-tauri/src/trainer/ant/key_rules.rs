//! The ANT+ network key's rules: its hash, how a written key reads, and
//! when a build must stop. Compiled twice: into the app (through
//! [`key`](super::key)) and into build.rs (`#[path]`) — so std, sha2 and hex
//! only: no `crate::`, and no `option_env!` (the build script's dep-info
//! would keep the key's value too). A build given the key keeps it under
//! `target/` anyway — the app's dep-info, cargo's fingerprints of the
//! variable, the build script's `output` when it comes from `.env`, and the
//! binary — all ignored by git; `cargo -vv` and rust-analyzer see it too.

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

/// `SYZIFY_ANT_KEY` in the text of a `.env` file, read the way dotenv
/// files are: a byte-order mark, comments and blank lines skipped, any
/// line ending, an `export` prefix, spaces around `=`, quotes and a
/// trailing `# comment` taken off; the last one wins (an empty one too).
pub fn key_in_dotenv(text: &str) -> Option<String> {
    let text = text.trim_start_matches('\u{feff}');
    text.split(['\n', '\r']).rev().find_map(|line| {
        let line = line.trim();
        let line = line.strip_prefix("export").filter(|rest| rest.starts_with(char::is_whitespace)).map_or(line, str::trim_start);
        let (name, value) = line.split_once('=')?;
        (name.trim() == KEY_VAR).then(|| unquote(value.trim()).to_string())
    })
}

/// A value without its quotes, or without a `#` comment after a space.
fn unquote(value: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = value.strip_prefix(q).and_then(|rest| rest.split(q).next()) {
            return inner;
        }
    }
    let comment = value.char_indices().find(|&(i, c)| c == '#' && value[..i].ends_with(char::is_whitespace));
    comment.map_or(value, |(i, _)| value[..i].trim_end())
}

/// The text of the repository's `.env` as build.rs read it: none when there
/// is no such file; one that is there but cannot be read stops the build
/// (left unread, a key in it would be silently missing). Bytes that are not
/// UTF-8 elsewhere in it do not matter: the key is ASCII.
pub fn dotenv_text(read: std::io::Result<Vec<u8>>) -> Result<Option<String>, &'static str> {
    match read {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("the repository's .env is there but cannot be read"),
    }
}

/// What a build does with the key: the key to hand the app from `.env`
/// (`dotenv`, the file's text) when the environment (`var`) gives none —
/// the environment wins, even with a wrong key: the release workflow's
/// secret, a shell export — as plain hex; else nothing to hand. Either
/// source is checked as [`build_problem`] does, and an error is why the
/// build stops, naming the file when the key came from it.
pub fn build_key(var: impl Fn(&str) -> Option<OsString>, dotenv: Option<&str>, sha: &str) -> Result<Option<String>, &'static str> {
    let in_env = var(KEY_VAR).is_some_and(|v| !v.to_string_lossy().trim().is_empty());
    let from_file = if in_env { None } else { dotenv.and_then(key_in_dotenv) };
    let lookup = |name: &str| match (name == KEY_VAR, &from_file) {
        (true, Some(key)) => Some(OsString::from(key)),
        _ => var(name),
    };
    match (build_problem(lookup, sha), from_file.as_deref().map(str::trim)) {
        (Some(_), Some(key)) if !key.is_empty() => Err("SYZIFY_ANT_KEY in the repository's .env is not the ANT+ network key"),
        (Some(problem), _) => Err(problem),
        (None, _) => Ok(from_file.and_then(|key| checked(&key, sha))),
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

    #[test]
    fn the_key_is_read_from_a_dotenv_as_a_shell_would() {
        let read = |text: &str| key_in_dotenv(text);
        assert_eq!(read("SYZIFY_ANT_KEY=0123456789abcdef").as_deref(), Some("0123456789abcdef"));
        let file = "# the ANT+ key\n#128 SYZIFY_ANT_KEY=ffff\n\nVITE_X=1\nexport  SYZIFY_ANT_KEY = \"01 23 45 67 89 AB CD EF\"\n";
        assert_eq!(read(file).as_deref(), Some("01 23 45 67 89 AB CD EF"), "comments skipped, export and quotes off");
        assert_eq!(read("SYZIFY_ANT_KEY='0123'").as_deref(), Some("0123"));
        assert_eq!(read("SYZIFY_ANT_KEY=0123456789abcdef # Garmin's").as_deref(), Some("0123456789abcdef"));
        assert_eq!(read("SYZIFY_ANT_KEY=a\nSYZIFY_ANT_KEY=b").as_deref(), Some("b"), "the last one wins");
        assert_eq!(read("SYZIFY_ANT_KEY=a\nSYZIFY_ANT_KEY=").as_deref(), Some(""), "an empty last one too");
        assert_eq!(read("OTHER=1\nSYZIFY_ANT_KEY_REQUIRED=1\nnonsense"), None);
        // Files written elsewhere: a BOM, CRLF or CR alone, a tab.
        assert_eq!(read("\u{feff}SYZIFY_ANT_KEY=0123\r\n").as_deref(), Some("0123"));
        assert_eq!(read("A=1\rSYZIFY_ANT_KEY=0123\rB=2").as_deref(), Some("0123"));
        assert_eq!(read("export\tSYZIFY_ANT_KEY=0123").as_deref(), Some("0123"));
        assert_eq!(read("exportSYZIFY_ANT_KEY=0123"), None, "not an export");
        // Comments after a value, quoted or not; what a value holds stays.
        assert_eq!(read("SYZIFY_ANT_KEY=\"01 23\"  # Garmin's").as_deref(), Some("01 23"));
        assert_eq!(read("SYZIFY_ANT_KEY='01 23' # Garmin's").as_deref(), Some("01 23"));
        assert_eq!(read("SYZIFY_ANT_KEY=0123\t# Garmin's").as_deref(), Some("0123"));
        assert_eq!(read("SYZIFY_ANT_KEY=01#23").as_deref(), Some("01#23"), "a # inside is no comment");
        assert_eq!(read("SYZIFY_ANT_KEY=a=b").as_deref(), Some("a=b"));
    }

    #[test]
    fn a_dotenv_missing_is_none_one_unreadable_stops_the_build() {
        use std::io::{Error, ErrorKind};
        assert_eq!(dotenv_text(Ok(b"A=1\xFF\nSYZIFY_ANT_KEY=0123".to_vec())).map(|t| t.and_then(|t| key_in_dotenv(&t))), Ok(Some("0123".into())));
        assert_eq!(dotenv_text(Err(Error::from(ErrorKind::NotFound))), Ok(None));
        assert!(dotenv_text(Err(Error::from(ErrorKind::PermissionDenied))).unwrap_err().contains("cannot be read"));
    }

    /// The environment's key wins over the file's; the file's is handed
    /// to the app as plain hex; either is checked; none is fine unless
    /// required.
    #[test]
    fn a_build_takes_the_key_from_its_environment_else_from_dotenv() {
        let sha = hex::encode(Sha256::digest(DUMMY));
        let file = Some("SYZIFY_ANT_KEY=01 23 45 67 89 ab cd ef\n");
        assert_eq!(build_key(env(None, None), file, &sha), Ok(Some("0123456789abcdef".into())));
        assert_eq!(build_key(env(text("  "), None), file, &sha), Ok(Some("0123456789abcdef".into())), "a blank export is none");
        assert_eq!(build_key(env(text("0123456789abcdef"), None), Some("SYZIFY_ANT_KEY=nonsense"), &sha), Ok(None), "the environment's wins");
        let wrong = build_key(env(None, None), Some("SYZIFY_ANT_KEY=0123456789abcdee"), &sha).unwrap_err();
        assert!(wrong.contains("in the repository's .env is not the ANT+ network key"), "{wrong}");
        let wrong_env = build_key(env(text("0123456789abcdee"), None), file, &sha).unwrap_err();
        assert_eq!(wrong_env, "SYZIFY_ANT_KEY is set but is not the ANT+ network key", "the environment wins, wrong or not");
        let emptied = build_key(env(None, Some("1")), Some("SYZIFY_ANT_KEY=0123456789abcdef\nSYZIFY_ANT_KEY="), &sha);
        assert!(emptied.unwrap_err().contains("a release needs"), "emptied by its last line");
        assert_eq!(build_key(env(None, None), None, &sha), Ok(None));
        assert_eq!(build_key(env(None, None), Some("VITE_X=1"), &sha), Ok(None));
        let missing = build_key(env(None, Some("1")), Some("VITE_X=1"), &sha).unwrap_err();
        assert!(missing.contains("a release needs"), "{missing}");
        assert_eq!(build_key(env(None, Some("1")), file, &sha), Ok(Some("0123456789abcdef".into())), "required, given by the file");
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
