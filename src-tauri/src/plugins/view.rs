use serde::{Deserialize, Serialize};

/// A declarative view a plugin returns from a UI contribution. The host renders
/// it with a fixed set of safe primitives — a plugin never produces raw HTML or
/// touches the DOM, so it cannot inject markup or scripts into the app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewSpec {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub elements: Vec<ViewElement>,
    /// The action the host fires next, on its own, right after showing this
    /// view — a sync plugin's "show the progress, then call me again". Only
    /// the sync page honours it, and only after a user action; a widget, a
    /// planner page and the initial render ignore it, so no page opens into
    /// a running loop. `"continue"` on the wire; a short plain token
    /// (`validate`).
    #[serde(default, rename = "continue", skip_serializing_if = "Option::is_none")]
    pub continue_action: Option<String>,
}

/// Longest `continue` action; a button action is a short word.
pub const MAX_CONTINUE_ACTION: usize = 64;

impl ViewSpec {
    /// The `continue` action goes back to the plugin as the next call's
    /// `action`, through the frontend: keep it a short plain token, not a
    /// payload (a plugin sent 200 kB back through the IPC otherwise).
    pub fn validate(&self) -> Result<(), String> {
        let Some(action) = &self.continue_action else { return Ok(()) };
        let plain = !action.is_empty()
            && action.len() <= MAX_CONTINUE_ACTION
            && action.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':'));
        if plain {
            Ok(())
        } else {
            Err(format!(
                "invalid continue action: 1 to {MAX_CONTINUE_ACTION} characters of letters, digits, '_', '-', '.', ':'"
            ))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewElement {
    Heading { text: String },
    Text { text: String },
    Stat { label: String, value: String },
    StatGrid { stats: Vec<StatItem> },
    Table { headers: Vec<String>, rows: Vec<Vec<String>> },
    Divider,
    /// A callout the user should notice: a refused sign-in, a stopped
    /// sync, a success. `level` is "info" (default), "warning" or "error".
    Notice {
        text: String,
        #[serde(default)]
        level: String,
    },
    // Interactive elements. The host tracks input values and, on a button press,
    // re-invokes the plugin with `{ action, values, …context }`.
    Input {
        id: String,
        label: String,
        #[serde(default)]
        value: String,
        /// "text" (default), "number" or "password" (rendered masked; the
        /// value still travels through the action loop like any other).
        #[serde(default)]
        input_type: String,
    },
    Select {
        id: String,
        label: String,
        options: Vec<String>,
        #[serde(default)]
        value: String,
    },
    Button {
        label: String,
        action: String,
    },
    /// A map overlay: a polyline through `[lat, lon]` points.
    Map {
        #[serde(default)]
        points: Vec<[f64; 2]>,
        #[serde(default)]
        label: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatItem {
    pub label: String,
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_spec_roundtrips_tagged_elements() {
        let spec = ViewSpec {
            title: Some("Consistency".to_string()),
            elements: vec![
                ViewElement::Stat { label: "Streak".to_string(), value: "5 weeks".to_string() },
                ViewElement::Divider,
            ],
            continue_action: None,
        };
        let json = serde_json::to_string(&spec).unwrap();
        // tagged enum -> "type":"stat" / "divider"
        assert!(json.contains("\"type\":\"stat\""));
        assert!(json.contains("\"type\":\"divider\""));

        let back: ViewSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.elements.len(), 2);
        assert_eq!(back.title.as_deref(), Some("Consistency"));
    }

    #[test]
    fn missing_fields_default() {
        let spec: ViewSpec = serde_json::from_str("{}").unwrap();
        assert!(spec.title.is_none());
        assert!(spec.elements.is_empty());
        assert!(spec.continue_action.is_none());
    }

    /// `continue` rides through as the plugin wrote it, and is absent from
    /// the wire when unset — the frontend's optional field.
    #[test]
    fn continue_action_is_the_continue_key() {
        let spec: ViewSpec = serde_json::from_str(r#"{"continue":"sync","elements":[]}"#).unwrap();
        assert_eq!(spec.continue_action.as_deref(), Some("sync"));
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains(r#""continue":"sync""#), "{json}");
        let none = serde_json::to_string(&ViewSpec { title: None, elements: vec![], continue_action: None }).unwrap();
        assert!(!none.contains("continue"), "{none}");
    }

    #[test]
    fn continue_action_is_a_short_plain_token() {
        let with = |a: &str| ViewSpec { title: None, elements: vec![], continue_action: Some(a.to_string()) };
        assert!(ViewSpec { title: None, elements: vec![], continue_action: None }.validate().is_ok());
        assert!(with("sync").validate().is_ok());
        assert!(with("sync.activities:next-page_2").validate().is_ok());
        assert!(with("").validate().is_err());
        assert!(with("a b").validate().is_err());
        assert!(with("{\"x\":1}").validate().is_err());
        assert!(with(&"a".repeat(MAX_CONTINUE_ACTION + 1)).validate().is_err());
        assert!(with(&"a".repeat(MAX_CONTINUE_ACTION)).validate().is_ok());
    }

    #[test]
    fn deserializes_interactive_and_map_elements() {
        let json = r#"{"elements":[
            {"type":"notice","text":"Signed in.","level":"info"},
            {"type":"notice","text":"HTTP 429"},
            {"type":"input","id":"dist","label":"Distance","value":"8","input_type":"number"},
            {"type":"select","id":"sport","label":"Sport","options":["run","ride"],"value":"run"},
            {"type":"button","label":"Plan","action":"plan"},
            {"type":"map","points":[[52.5,13.4],[52.6,13.5]],"label":"Route"}
        ]}"#;
        let spec: ViewSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.elements.len(), 6);
        match &spec.elements[5] {
            ViewElement::Map { points, .. } => assert_eq!(points.len(), 2),
            other => panic!("expected map, got {other:?}"),
        }
        match &spec.elements[1] {
            ViewElement::Notice { text, level } => assert_eq!((text.as_str(), level.as_str()), ("HTTP 429", "")),
            other => panic!("expected notice, got {other:?}"),
        }
    }
}
