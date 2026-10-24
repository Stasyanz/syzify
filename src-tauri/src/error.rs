//! The error a Tauri command rejects with (#236).
//!
//! The frontend translates a rejection by its `code` (the dictionaries'
//! `error.<code>`, filled with `params`); `message` is the English text, the
//! fallback for a code it does not know and the whole error when there is no
//! code. Only the app's own words get a code: what a library says (io,
//! SQLite, zip, HTTP) and what is meant for a developer stays an uncoded
//! message, shown as it is. A plugin never sees this type — what crosses into
//! a plugin is the English message (`From<CommandError> for String`).

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use serde_json::Value;

/// Serialized as `{ code, params, message }`: `code` is `null` for an
/// uncoded error and `params` is then empty.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommandError {
    pub code: Option<&'static str>,
    /// Named values for the translation's `{name}`s: a number stays a number,
    /// so the frontend writes it the language's way; a path or a name is a
    /// string.
    pub params: BTreeMap<&'static str, Value>,
    pub message: String,
}

impl CommandError {
    /// A user-facing error the frontend translates by `code`. `message` is
    /// what English says — the same text the dictionary's English entry has,
    /// so a code the frontend does not know reads as before. The code is a
    /// literal: `errorCodes.test.ts` reads every `coded("…")` in the Rust
    /// sources and fails on one without a dictionary key.
    pub fn coded(code: &'static str, message: impl Into<String>) -> Self {
        CommandError { code: Some(code), params: BTreeMap::new(), message: message.into() }
    }

    /// Name a value the translation fills in.
    pub fn param(mut self, name: &'static str, value: impl Into<Value>) -> Self {
        self.params.insert(name, value.into());
        self
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Every existing `Err(String)` and `.map_err(|e| e.to_string())?` is an
/// uncoded error: shown as it says.
impl From<String> for CommandError {
    fn from(message: String) -> Self {
        CommandError { code: None, params: BTreeMap::new(), message }
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

/// The plugin and Host SDK boundary: a plugin reads strings, so a coded
/// error reaches it as its English message.
impl From<CommandError> for String {
    fn from(e: CommandError) -> Self {
        e.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_coded_error_serializes_its_code_params_and_english_message() {
        let e = CommandError::coded("import.extractFailed", "Failed to extract a.gpx: disk full")
            .param("name", "a.gpx")
            .param("detail", "disk full");
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({
                "code": "import.extractFailed",
                "params": { "name": "a.gpx", "detail": "disk full" },
                "message": "Failed to extract a.gpx: disk full",
            })
        );
        // A number stays a number, for the frontend to write the language's way.
        let e = CommandError::coded("import.tooManyFiles", "export has too many files (limit 10000)").param("limit", 10000);
        assert_eq!(serde_json::to_value(&e).unwrap()["params"], serde_json::json!({ "limit": 10000 }));
    }

    #[test]
    fn an_uncoded_error_serializes_a_null_code_and_no_params() {
        let e = CommandError::from("disk full");
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({ "code": null, "params": {}, "message": "disk full" })
        );
    }

    #[test]
    fn strings_convert_in_and_the_message_converts_out() {
        let from_string: CommandError = String::from("boom").into();
        let from_str: CommandError = "boom".into();
        assert_eq!(from_string, from_str);
        assert_eq!(from_string.code, None);
        let coded = CommandError::coded("vault.wrongPassword", "Wrong password");
        assert_eq!(coded.to_string(), "Wrong password");
        assert_eq!(String::from(coded), "Wrong password");
    }

    #[test]
    fn the_question_mark_converts_a_string_error() {
        fn inner() -> Result<(), String> {
            Err("inner".into())
        }
        fn outer() -> Result<(), CommandError> {
            Ok(inner()?)
        }
        assert_eq!(outer(), Err(CommandError::from("inner")));
    }
}
