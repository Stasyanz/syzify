//! The ANT+ network key: every ANT+ sensor talks on this network. Garmin
//! publishes it (developer.garmin.com/ant-program), subject to the ANT+
//! Adopter Agreement, and asks that it not be published in source code —
//! so it is not in these sources, only its SHA-256, which does not give
//! it away. A build may be given it (`SYZIFY_ANT_KEY`, 16 hex digits); a
//! build without it takes the one the user typed in. Either is checked
//! against the hash.

use sha2::{Digest, Sha256};

/// SHA-256 of the key's eight bytes.
pub const KEY_SHA256: &str = "ba53300909ed532d17a25cad3c433266e2864ee5db97d6424b9021fc3334a774";

/// Where a key the user typed in is kept (hex).
pub const SETTING: &str = "training.ant_key";

/// Where Garmin publishes the key…
pub const KEY_PAGE: &str = "https://developer.garmin.com/ant-program/ant-ant-plus/network-keys/";
/// …and the agreement its use is subject to.
pub const AGREEMENT_PAGE: &str = "https://developer.garmin.com/ant-program/licensing/adopter-agreement/";

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

fn matches(key: &[u8; 8], sha: &str) -> bool {
    hex::encode(Sha256::digest(key)) == sha
}

/// The first of `built` and `entered` that is the key whose hash is `sha`.
fn choose(built: Option<&str>, entered: Option<&str>, sha: &str) -> Option<[u8; 8]> {
    [built, entered].into_iter().flatten().filter_map(parse_key).find(|k| matches(k, sha))
}

/// The key this build was given.
pub fn built_in() -> bool {
    choose(option_env!("SYZIFY_ANT_KEY"), None, KEY_SHA256).is_some()
}

/// The key to use: this build's, else the one the user typed in.
pub fn network_key(entered: Option<&str>) -> Option<[u8; 8]> {
    choose(option_env!("SYZIFY_ANT_KEY"), entered, KEY_SHA256)
}

/// A key the user typed in, as it is kept, if it is the key whose hash is
/// `sha` (`KEY_SHA256`; tests give their own).
pub fn checked(text: &str, sha: &str) -> Option<String> {
    parse_key(text).filter(|k| matches(k, sha)).map(hex::encode)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMMY: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];

    #[test]
    fn a_key_reads_however_it_is_written() {
        for text in ["0123456789abcdef", "01 23 45 67 89 AB CD EF", "01:23:45:67:89:ab:cd:ef", "0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF"] {
            assert_eq!(parse_key(text), Some(DUMMY), "{text}");
        }
        for not in ["", "0123", "0123456789abcdef00", "0123456789abcdeg", "01 23 45 67 89 AB CD É"] {
            assert_eq!(parse_key(not), None, "{not}");
        }
    }

    #[test]
    fn only_the_key_its_hash_names_is_taken_the_build_s_first() {
        let sha = hex::encode(Sha256::digest(DUMMY));
        assert_eq!(choose(None, Some("0123456789abcdef"), &sha), Some(DUMMY), "typed in");
        assert_eq!(choose(Some("01 23 45 67 89 ab cd ef"), Some("ffffffffffffffff"), &sha), Some(DUMMY), "built in");
        assert_eq!(choose(Some(""), Some("0123456789abcdef"), &sha), Some(DUMMY), "an empty build variable is none");
        assert_eq!(choose(None, Some("ffffffffffffffff"), &sha), None, "not the key");
        assert_eq!(choose(None, None, &sha), None);
        assert_eq!(checked("0123456789abcdef", KEY_SHA256), None, "not the ANT+ key");
        assert_eq!(checked("01 23 45 67 89 AB CD EF", &sha).as_deref(), Some("0123456789abcdef"), "kept as plain hex");
        assert_eq!(network_key(Some("nonsense")), choose(option_env!("SYZIFY_ANT_KEY"), None, KEY_SHA256));
        assert_eq!(built_in(), network_key(None).is_some());
    }

    /// Sum and XOR of the key's bytes (mod 256): a cheap look before the
    /// hash, the same whichever way round the bytes go. Changed only with
    /// `KEY_SHA256`: a wrong one blinds the scan.
    const KEY_SUM: u8 = 0xB1;
    const KEY_XOR: u8 = 0x8F;

    /// A build given the key checks the cheap look against it.
    #[test]
    fn the_cheap_look_is_the_key_s() {
        if let Some(key) = network_key(None) {
            assert_eq!(key.iter().fold(0u8, |s, &b| s.wrapping_add(b)), KEY_SUM);
            assert_eq!(key.iter().fold(0, |x, &b| x ^ b), KEY_XOR);
        }
    }

    /// What a key with these sum, XOR and hash looks like.
    struct Looked<'a> {
        sum: u8,
        xor: u8,
        sha: &'a str,
    }

    impl Looked<'_> {
        /// Whether eight bytes are the key, either way round.
        fn is(&self, w: &[u8]) -> bool {
            let (sum, xor) = w.iter().fold((0u8, 0u8), |(s, x), &b| (s.wrapping_add(b), x ^ b));
            if sum != self.sum || xor != self.xor {
                return false;
            }
            let mut key: [u8; 8] = w.try_into().expect("8 bytes");
            if matches(&key, self.sha) {
                return true;
            }
            key.reverse();
            matches(&key, self.sha)
        }

        /// Whether `bytes` hold the key, however written: as bytes; as hex
        /// digits, spaced or not, `0x`-prefixed, across lines, inside a
        /// longer dump (both alignments); as decimal bytes.
        fn in_file(&self, bytes: &[u8]) -> bool {
            let has = |seq: &[u8]| seq.windows(8).any(|w| self.is(w));
            if has(bytes) {
                return true;
            }
            let text = String::from_utf8_lossy(bytes);
            let tokens: Vec<&str> = text
                .split(|c: char| !c.is_ascii_alphanumeric())
                .map(|t| t.trim_start_matches("0x").trim_start_matches("0X"))
                .filter(|t| !t.is_empty())
                .collect();
            let mut nibbles: Vec<u8> = Vec::new();
            let mut decimals: Vec<u8> = Vec::new();
            // An integer literal's type suffix is not part of its digits.
            let digits_of = |t: &str| -> Option<Vec<u32>> {
                let end = t.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(t.len());
                let suffix = t[end..].to_ascii_lowercase();
                let typed = matches!(
                    suffix.as_str(),
                    "" | "u" | "l" | "ul" | "ull" | "ll" | "u8" | "i8" | "u16" | "u32" | "u64" | "u128" | "i64" | "usize"
                );
                typed.then(|| t[..end].chars().filter_map(|c| c.to_digit(16)).collect())
            };
            for t in tokens.iter().chain(std::iter::once(&"-")) {
                match digits_of(t) {
                    Some(digits) => nibbles.extend(digits.iter().map(|&d| d as u8)),
                    None => {
                        for start in 0..2.min(nibbles.len()) {
                            let bytes: Vec<u8> = nibbles[start..].chunks_exact(2).map(|p| p[0] << 4 | p[1]).collect();
                            if has(&bytes) {
                                return true;
                            }
                        }
                        nibbles.clear();
                    }
                }
                match t.parse::<u8>() {
                    Ok(b) if t.len() <= 3 => decimals.push(b),
                    _ => {
                        if has(&decimals) {
                            return true;
                        }
                        decimals.clear();
                    }
                }
            }
            false
        }
    }

    #[test]
    fn the_key_is_found_however_it_is_written() {
        let dummy = Looked {
            sum: DUMMY.iter().fold(0u8, |s, &b| s.wrapping_add(b)),
            xor: DUMMY.iter().fold(0, |x, &b| x ^ b),
            sha: &hex::encode(Sha256::digest(DUMMY)),
        };
        for written in [
            "const K: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];",
            "const K: [u8; 8] = [\n    0x01,\n    0x23,\n    0x45,\n    0x67,\n    0x89,\n    0xAB,\n    0xCD,\n    0xEF,\n];",
            "let k = [1, 35, 69, 103, 137, 171, 205, 239];",
            "frame a4094600 0123456789abcdef 7e",
            "a409460123456789abcdef7e",
            "0x0123_4567_89AB_CDEF",
            "0xEFCDAB8967452301u64",
            "[0x01u8, 0x23u8, 0x45u8, 0x67u8, 0x89u8, 0xABu8, 0xCDu8, 0xEFu8]",
        ] {
            assert!(dummy.in_file(written.as_bytes()), "{written}");
        }
        let raw = [&b"abc"[..], &DUMMY, b"xyz"].concat();
        assert!(dummy.in_file(&raw), "as bytes");
        for not in ["0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD", "1, 35, 69, 103, 137, 171, 205, 238", "nothing here", ""] {
            assert!(!dummy.in_file(not.as_bytes()), "{not}");
        }
    }

    /// The key must never land in the repository, written any way: only
    /// its hash is here.
    #[test]
    fn the_key_is_in_no_file_of_the_repository() {
        let key = Looked { sum: KEY_SUM, xor: KEY_XOR, sha: KEY_SHA256 };
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
        let listed = std::process::Command::new("git")
            .args(["ls-files", "--cached", "--others", "--exclude-standard"])
            .current_dir(root)
            .output()
            .expect("git");
        let files = String::from_utf8_lossy(&listed.stdout).into_owned();
        assert!(files.lines().count() > 100, "the repository's files are listed");
        for file in files.lines() {
            let path = std::path::Path::new(root).join(file);
            let Ok(bytes) = std::fs::read(&path) else { continue };
            assert!(!key.in_file(&bytes), "the ANT+ network key is in {file}");
        }
    }
}
