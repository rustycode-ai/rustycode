//! Model Cost Table — Cross-Provider Cost Comparison
//!
//! Static cost reference for known models:
//! * Cost per 1K tokens (input/output) in USD
//! * Cross-provider cost comparison
//! * Model lookup by ID
//!
//! Critical for cost-aware model routing in autonomous systems.

use std::collections::HashMap;

// ─── Types ────────────────────────────────────────────────────────────────────

/// Model cost entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelCostEntry {
    /// Model ID (bare, without provider prefix)
    pub id: String,

    /// Approximate cost per 1K input tokens in USD
    pub input_per_1k: f64,

    /// Approximate cost per 1K output tokens in USD
    pub output_per_1k: f64,

    /// Last updated date
    pub updated_at: String,
}

/// Comparison result
pub type ModelCostComparison = f64;

// ─── Bundled Cost Table ───────────────────────────────────────────────────────

/// Build the bundled cost table for known models
#[allow(clippy::too_many_lines)]
fn build_bundled_cost_table() -> HashMap<String, ModelCostEntry> {
    let mut table = HashMap::new();

    // Anthropic
    table.insert(
        "claude-opus-4-6".to_string(),
        ModelCostEntry {
            id: "claude-opus-4-6".to_string(),
            input_per_1k: 0.015,
            output_per_1k: 0.075,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "claude-sonnet-4-6".to_string(),
        ModelCostEntry {
            id: "claude-sonnet-4-6".to_string(),
            input_per_1k: 0.003,
            output_per_1k: 0.015,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "claude-haiku-4-5".to_string(),
        ModelCostEntry {
            id: "claude-haiku-4-5".to_string(),
            input_per_1k: 0.0008,
            output_per_1k: 0.004,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "claude-sonnet-4-5-20250514".to_string(),
        ModelCostEntry {
            id: "claude-sonnet-4-5-20250514".to_string(),
            input_per_1k: 0.003,
            output_per_1k: 0.015,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "claude-sonnet-4-6".to_string(),
        ModelCostEntry {
            id: "claude-sonnet-4-6".to_string(),
            input_per_1k: 0.003,
            output_per_1k: 0.015,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "claude-3-5-haiku-latest".to_string(),
        ModelCostEntry {
            id: "claude-3-5-haiku-latest".to_string(),
            input_per_1k: 0.0008,
            output_per_1k: 0.004,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "claude-3-opus-latest".to_string(),
        ModelCostEntry {
            id: "claude-3-opus-latest".to_string(),
            input_per_1k: 0.015,
            output_per_1k: 0.075,
            updated_at: "2025-03-15".to_string(),
        },
    );

    // OpenAI
    table.insert(
        "gpt-4o".to_string(),
        ModelCostEntry {
            id: "gpt-4o".to_string(),
            input_per_1k: 0.0025,
            output_per_1k: 0.01,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "gpt-4o-mini".to_string(),
        ModelCostEntry {
            id: "gpt-4o-mini".to_string(),
            input_per_1k: 0.00015,
            output_per_1k: 0.0006,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "o1".to_string(),
        ModelCostEntry {
            id: "o1".to_string(),
            input_per_1k: 0.015,
            output_per_1k: 0.06,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "o3".to_string(),
        ModelCostEntry {
            id: "o3".to_string(),
            input_per_1k: 0.015,
            output_per_1k: 0.06,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "gpt-4-turbo".to_string(),
        ModelCostEntry {
            id: "gpt-4-turbo".to_string(),
            input_per_1k: 0.01,
            output_per_1k: 0.03,
            updated_at: "2025-03-15".to_string(),
        },
    );

    // Google
    table.insert(
        "gemini-2.0-flash".to_string(),
        ModelCostEntry {
            id: "gemini-2.0-flash".to_string(),
            input_per_1k: 0.0001,
            output_per_1k: 0.0004,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "gemini-flash-2.0".to_string(),
        ModelCostEntry {
            id: "gemini-flash-2.0".to_string(),
            input_per_1k: 0.0001,
            output_per_1k: 0.0004,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table.insert(
        "gemini-2.5-pro".to_string(),
        ModelCostEntry {
            id: "gemini-2.5-pro".to_string(),
            input_per_1k: 0.00125,
            output_per_1k: 0.005,
            updated_at: "2025-03-15".to_string(),
        },
    );

    // DeepSeek
    table.insert(
        "deepseek-chat".to_string(),
        ModelCostEntry {
            id: "deepseek-chat".to_string(),
            input_per_1k: 0.00014,
            output_per_1k: 0.00028,
            updated_at: "2025-03-15".to_string(),
        },
    );

    table
}

// ─── Cost Table Access ─────────────────────────────────────────────────────────

/// Get the bundled cost table
pub fn bundled_cost_table() -> &'static HashMap<String, ModelCostEntry> {
    use std::sync::OnceLock;
    static COST_TABLE: OnceLock<HashMap<String, ModelCostEntry>> = OnceLock::new();
    COST_TABLE.get_or_init(build_bundled_cost_table)
}

/// Get all cost entries as a vector
pub fn all_cost_entries() -> Vec<ModelCostEntry> {
    bundled_cost_table().values().cloned().collect()
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Lookup cost for a model ID.
///
/// Extracts the bare model ID if a provider prefix is present, then
/// performs exact and partial matching against the bundled table.
pub fn lookup_model_cost(model_id: &str) -> Option<ModelCostEntry> {
    let table = bundled_cost_table();

    // Extract bare ID if provider prefix present
    let bare_id = if model_id.contains('/') {
        model_id.split('/').next_back().unwrap_or(model_id)
    } else {
        model_id
    };

    // Exact match first
    if let Some(entry) = table.get(bare_id) {
        return Some(entry.clone());
    }

    // Partial match (bare_id contains entry id or vice versa)
    for (id, entry) in table {
        if bare_id.contains(id) || id.contains(bare_id) {
            return Some(entry.clone());
        }
    }

    None
}

/// Compare two models by input cost.
///
/// Returns negative if `model_id_a` is cheaper, positive if `model_id_b`
/// is cheaper, zero if equal or both unknown.
pub fn compare_model_cost(model_id_a: &str, model_id_b: &str) -> ModelCostComparison {
    let cost_a = lookup_model_cost(model_id_a).map_or(999.0, |e| e.input_per_1k);

    let cost_b = lookup_model_cost(model_id_b).map_or(999.0, |e| e.input_per_1k);

    cost_a - cost_b
}

/// Calculate total cost for a request.
///
/// Returns the total cost in USD, or `None` if the model is not found.
#[allow(clippy::cast_precision_loss)]
pub fn calculate_cost(model_id: &str, input_tokens: usize, output_tokens: usize) -> Option<f64> {
    let entry = lookup_model_cost(model_id)?;

    let input_cost = (input_tokens as f64 / 1000.0) * entry.input_per_1k;
    let output_cost = (output_tokens as f64 / 1000.0) * entry.output_per_1k;

    Some(input_cost + output_cost)
}

/// Find cheapest model from a list by input cost.
///
/// Returns the cheapest model ID, or `None` if the list is empty.
pub fn find_cheapest_model(model_ids: &[&str]) -> Option<String> {
    if model_ids.is_empty() {
        return None;
    }

    model_ids
        .iter()
        .min_by(|a, b| {
            let cost_a = lookup_model_cost(a).map_or(f64::MAX, |e| e.input_per_1k);
            let cost_b = lookup_model_cost(b).map_or(f64::MAX, |e| e.input_per_1k);
            cost_a
                .partial_cmp(&cost_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(ToString::to_string)
}

/// Check if a model is known in the cost table.
pub fn is_model_known(model_id: &str) -> bool {
    lookup_model_cost(model_id).is_some()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_anthropic_model() {
        let cost = lookup_model_cost("claude-sonnet-4-6");
        assert!(cost.is_some());
        assert_eq!(cost.unwrap().id, "claude-sonnet-4-6");
    }

    #[test]
    fn test_lookup_openai_model() {
        let cost = lookup_model_cost("gpt-4o");
        assert!(cost.is_some());
        assert_eq!(cost.unwrap().id, "gpt-4o");
    }

    #[test]
    fn test_lookup_with_provider_prefix() {
        let cost = lookup_model_cost("anthropic/claude-sonnet-4-6");
        assert!(cost.is_some());
        assert_eq!(cost.unwrap().id, "claude-sonnet-4-6");
    }

    #[test]
    fn test_lookup_unknown_model() {
        let cost = lookup_model_cost("unknown-model-x");
        assert!(cost.is_none());
    }

    #[test]
    fn test_compare_model_cost_cheaper() {
        let comparison = compare_model_cost("claude-haiku-4-5", "claude-sonnet-4-6");
        assert!(comparison < 0.0); // Haiku is cheaper
    }

    #[test]
    fn test_compare_model_cost_more_expensive() {
        let comparison = compare_model_cost("claude-opus-4-6", "claude-sonnet-4-6");
        assert!(comparison > 0.0); // Opus is more expensive
    }

    #[test]
    fn test_compare_model_cost_equal() {
        let comparison = compare_model_cost("claude-sonnet-4-6", "claude-sonnet-4-6");
        assert_eq!(comparison, 0.0);
    }

    #[test]
    fn test_compare_model_cost_unknown() {
        let comparison = compare_model_cost("unknown-model-a", "unknown-model-b");
        assert_eq!(comparison, 0.0); // Both unknown
    }

    #[test]
    fn test_calculate_cost() {
        let cost = calculate_cost("claude-sonnet-4-6", 1000, 500);
        assert!(cost.is_some());
        // (1000 * 0.003 + 500 * 0.015) / 1000 = 0.003 + 0.0075 = 0.0105
        let cost_value = cost.unwrap();
        assert!((cost_value - 0.0105).abs() < 0.0001);
    }

    #[test]
    fn test_calculate_cost_unknown_model() {
        let cost = calculate_cost("unknown-model", 1000, 500);
        assert!(cost.is_none());
    }

    #[test]
    fn test_find_cheapest_model() {
        let models = vec!["claude-sonnet-4-6", "claude-opus-4-6", "claude-haiku-4-5"];
        let cheapest = find_cheapest_model(&models);
        assert_eq!(cheapest, Some("claude-haiku-4-5".to_string()));
    }

    #[test]
    fn test_find_cheapest_model_empty() {
        let models: Vec<&str> = vec![];
        let cheapest = find_cheapest_model(&models);
        assert!(cheapest.is_none());
    }

    #[test]
    fn test_is_model_known() {
        assert!(is_model_known("claude-sonnet-4-6"));
        assert!(is_model_known("gpt-4o"));
        assert!(!is_model_known("unknown-model"));
    }

    #[test]
    fn test_get_all_cost_entries() {
        let entries = all_cost_entries();
        assert!(entries.len() > 10);

        // Check that known models are present
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"claude-sonnet-4-6"));
        assert!(ids.contains(&"gpt-4o"));
        assert!(ids.contains(&"gemini-2.0-flash"));
    }

    #[test]
    fn test_cost_entry_structure() {
        let cost = lookup_model_cost("claude-haiku-4-5").unwrap();
        assert_eq!(cost.id, "claude-haiku-4-5");
        assert_eq!(cost.input_per_1k, 0.0008);
        assert_eq!(cost.output_per_1k, 0.004);
        assert_eq!(cost.updated_at, "2025-03-15");
    }

    #[test]
    fn test_cross_provider_comparison() {
        // Anthropic vs OpenAI
        let comparison = compare_model_cost("claude-haiku-4-5", "gpt-4o-mini");
        // Haiku: 0.0008, GPT-4o-mini: 0.00015
        // Haiku is more expensive than 4o-mini
        assert!(comparison > 0.0);
    }

    #[test]
    fn test_deepseek_cost() {
        let cost = lookup_model_cost("deepseek-chat");
        assert!(cost.is_some());
        assert_eq!(cost.unwrap().input_per_1k, 0.00014);
    }

    #[test]
    fn test_gemini_costs() {
        let flash = lookup_model_cost("gemini-2.0-flash");
        assert!(flash.is_some());
        assert_eq!(flash.unwrap().input_per_1k, 0.0001);

        let pro = lookup_model_cost("gemini-2.5-pro");
        assert!(pro.is_some());
        assert_eq!(pro.unwrap().input_per_1k, 0.00125);
    }

    #[test]
    fn test_calculate_cost_large_tokens() {
        let cost = calculate_cost("claude-opus-4-6", 100_000, 50_000);
        assert!(cost.is_some());
        // (100000 * 0.015 + 50000 * 0.075) / 1000 = 1.5 + 3.75 = 5.25
        assert_eq!(cost.unwrap(), 5.25);
    }
}
