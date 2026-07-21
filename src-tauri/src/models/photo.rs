use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Photo {
    pub id: String,
    pub activity_id: String,
    pub path_in_vault: String,
    pub thumbnail_path: Option<String>,
    pub original_path: Option<String>,
    pub mime_type: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub size_bytes: i64,
    pub hash_sha256: String,
    pub taken_at: Option<String>,
    pub caption: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
}
