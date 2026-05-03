//! Configuration for result summarization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for result summarization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryConfig {
    /// Maximum chars before summarization kicks in.
    pub max_output_chars: usize,
    /// Whether to always preserve error lines.
    pub preserve_errors: bool,
    /// Whether to always preserve the final result line.
    pub preserve_final_result: bool,
    /// Maximum lines to keep in bash output summary.
    pub max_bash_lines: usize,
    /// JSON keys to always extract (e.g. `status`, `error`, `result`).
    pub json_extract_keys: Vec<String>,
    /// Custom extractors: `tool_type` -> regex pattern for lines to keep.
    pub custom_extractors: HashMap<String, String>,
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            max_output_chars: 2000,
            preserve_errors: true,
            preserve_final_result: true,
            max_bash_lines: 50,
            json_extract_keys: vec![
                "status".into(),
                "result".into(),
                "error".into(),
                "errors".into(),
                "success".into(),
                "data".into(),
            ],
            custom_extractors: HashMap::new(),
        }
    }
}
