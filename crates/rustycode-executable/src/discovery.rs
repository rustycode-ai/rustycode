//! Tool search and discovery service with relevance scoring

use crate::registry::{ExecutableRegistry, UnitMetadata};
use crate::ToolSchema;

/// Search options for tool discovery
#[derive(Clone)]
pub struct ToolSearchOptions {
    pub include_full_definitions: bool,
    pub limit: usize,
}

impl Default for ToolSearchOptions {
    fn default() -> Self {
        Self {
            include_full_definitions: false,
            limit: 10,
        }
    }
}

/// A single search result
pub struct ToolSearchResult {
    pub id: String,
    pub name: String,
    pub description: String,
    pub full_definition: Option<ToolSchema>,
    pub relevance_score: f32,
}

/// Search service for finding executable units
pub struct ToolSearchService {
    registry: std::sync::Arc<ExecutableRegistry>,
}

impl ToolSearchService {
    pub const fn new(registry: std::sync::Arc<ExecutableRegistry>) -> Self {
        Self { registry }
    }

    /// Search for tools matching query, respecting `defer_loading`
    pub async fn search(
        &self,
        query: &str,
        options: ToolSearchOptions,
    ) -> Result<Vec<ToolSearchResult>, crate::ExecutableError> {
        let metadata_list = self.registry.discover(query, None).await;

        let mut results = Vec::new();
        for metadata in metadata_list {
            let relevance_score = Self::calculate_relevance(&metadata, query);

            let full_definition = if options.include_full_definitions {
                self.registry
                    .get(&metadata.id)
                    .await
                    .and_then(|unit| unit.schema)
            } else {
                None
            };

            let result = ToolSearchResult {
                id: metadata.id,
                name: metadata.name,
                description: metadata.description,
                full_definition,
                relevance_score,
            };

            results.push(result);
        }

        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results.into_iter().take(options.limit).collect())
    }

    #[allow(clippy::cast_precision_loss)]
    fn calculate_relevance(metadata: &UnitMetadata, query: &str) -> f32 {
        let query_lower = query.to_lowercase();

        let name_match = if metadata.name.to_lowercase() == query_lower {
            2.0
        } else {
            0.0
        };
        let hint_match = metadata
            .search_hints
            .iter()
            .filter(|hint| hint.to_lowercase().contains(&query_lower))
            .count() as f32
            * 0.5;
        let desc_match = if metadata.description.to_lowercase().contains(&query_lower) {
            0.3
        } else {
            0.0
        };

        name_match + hint_match + desc_match
    }
}
