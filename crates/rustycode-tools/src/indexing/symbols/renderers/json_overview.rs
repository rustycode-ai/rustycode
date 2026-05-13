use crate::indexing::symbols::FileOutline;

pub fn render_json_overview(outline: &FileOutline) -> String {
    serde_json::to_string_pretty(outline).unwrap_or_else(|_| "{ \"error\": \"failed to serialize\" }".to_string())
}
