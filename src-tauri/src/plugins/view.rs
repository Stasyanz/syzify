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
    // Interactive elements. The host tracks input values and, on a button press,
    // re-invokes the plugin with `{ action, values, …context }`.
    Input {
        id: String,
        label: String,
        #[serde(default)]
        value: String,
        /// "text" (default) or "number".
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
    }

    #[test]
    fn deserializes_interactive_and_map_elements() {
        let json = r#"{"elements":[
            {"type":"input","id":"dist","label":"Distance","value":"8","input_type":"number"},
            {"type":"select","id":"sport","label":"Sport","options":["run","ride"],"value":"run"},
            {"type":"button","label":"Plan","action":"plan"},
            {"type":"map","points":[[52.5,13.4],[52.6,13.5]],"label":"Route"}
        ]}"#;
        let spec: ViewSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.elements.len(), 4);
        match &spec.elements[3] {
            ViewElement::Map { points, .. } => assert_eq!(points.len(), 2),
            other => panic!("expected map, got {other:?}"),
        }
    }
}
