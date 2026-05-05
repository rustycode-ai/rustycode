// ── Context Assembler ───────────────────────────────────────────────────────────

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::context_management::{
    compression::{compress_context, CompressionStrategy},
    window::ContextWindow,
};
use crate::context_prio::{select_best, ContextItem, SortStrategy};

/// Metrics tracking for context assembly operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyMetrics {
    /// Timestamp of assembly
    pub timestamp: DateTime<Utc>,
    /// Total items available
    pub total_items: usize,
    /// Items selected for context
    pub selected_items: usize,
    /// Total tokens available
    pub total_tokens: usize,
    /// Tokens selected
    pub selected_tokens: usize,
    /// Quality score of assembled context
    pub quality_score: f64,
    /// Assembly duration in milliseconds
    pub duration_ms: u64,
}

/// Smart context assembly with prioritization and optimization.
///
/// The ContextAssembler takes a collection of context items and assembles
/// an optimal context window within token constraints.
#[derive(Debug, Clone)]
pub struct ContextAssembler {
    /// Default sorting strategy
    sort_strategy: SortStrategy,
    /// Enable quality tracking
    track_quality: bool,
    /// Assembly history (for analytics)
    history: VecDeque<AssemblyMetrics>,
}

impl Default for ContextAssembler {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextAssembler {
    pub fn new() -> Self {
        Self {
            sort_strategy: SortStrategy::ByScore,
            track_quality: true,
            history: VecDeque::with_capacity(100),
        }
    }

    /// Set the sorting strategy for assembly.
    pub fn with_sort_strategy(mut self, strategy: SortStrategy) -> Self {
        self.sort_strategy = strategy;
        self
    }

    /// Enable or disable quality tracking.
    pub fn with_quality_tracking(mut self, enabled: bool) -> Self {
        self.track_quality = enabled;
        self
    }

    /// Assemble an optimized context window from items.
    ///
    pub fn assemble_from_items<T>(
        &mut self,
        items: &[ContextItem<T>],
        max_tokens: usize,
    ) -> Result<ContextWindow>
    where
        T: AsRef<str> + Clone,
    {
        let start = std::time::Instant::now();

        // Convert to String items
        let string_items: Vec<ContextItem<String>> = items
            .iter()
            .map(|item| ContextItem {
                content: item.content.as_ref().to_string(),
                priority: item.priority,
                token_count: item.token_count,
                metadata: item.metadata.clone(),
            })
            .collect();

        // Create window
        let mut window = ContextWindow::with_reserved(max_tokens);
        window.metadata.assembly_count = window.metadata.assembly_count.saturating_add(1);

        // Select best items within budget
        let available = window.available_tokens();
        let selected = select_best(&string_items, available);

        // Add selected items to window
        for item in &selected {
            window.add_item((*item).clone())?;
        }

        // Track metrics
        let duration = start.elapsed();
        if self.track_quality {
            window.update_quality();

            let metrics = AssemblyMetrics {
                timestamp: Utc::now(),
                total_items: items.len(),
                selected_items: selected.len(),
                total_tokens: max_tokens,
                selected_tokens: window.used_tokens(),
                quality_score: window.quality_score(),
                duration_ms: duration.as_millis() as u64,
            };

            self.history.push_back(metrics);

            // Keep only recent history
            if self.history.len() > 100 {
                self.history.pop_front();
            }
        }

        Ok(window)
    }

    /// Assemble context from an existing window, optimizing if needed.
    ///
    pub fn assemble(
        &mut self,
        window: &ContextWindow,
        strategy: CompressionStrategy,
    ) -> Result<ContextWindow> {
        let mut optimized = window.clone();

        // Compress if needed
        if optimized.usage_percentage() > 0.8 {
            let target = (optimized.max_tokens() as f64 * 0.7) as usize;
            compress_context(&mut optimized, strategy, target)?;
        }

        // Sort content by strategy
        crate::context_prio::sort_by(optimized.content_mut(), self.sort_strategy);

        optimized.metadata.assembly_count += 1;
        if self.track_quality {
            optimized.update_quality();
        }

        Ok(optimized)
    }

    /// Get assembly history metrics.
    pub fn metrics(&self) -> Vec<&AssemblyMetrics> {
        self.history.iter().collect()
    }

    /// Get average quality score across all assemblies.
    pub fn average_quality(&self) -> f64 {
        if self.history.is_empty() {
            return 1.0;
        }

        let sum: f64 = self.history.iter().map(|m| m.quality_score).sum();
        sum / self.history.len() as f64
    }

    /// Clear assembly history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_prio::Priority;

    #[test]
    fn test_assembler_creation() {
        let assembler = ContextAssembler::new();
        assert_eq!(assembler.sort_strategy, SortStrategy::ByScore);
        assert!(assembler.track_quality);
    }

    #[test]
    fn test_assemble_from_items() {
        let mut assembler = ContextAssembler::new();

        let items = vec![
            ContextItem::new("low priority", Priority::Low),
            ContextItem::new("critical content", Priority::Critical),
            ContextItem::new("medium priority", Priority::Medium),
        ];

        let window = assembler.assemble_from_items(&items, 1000).unwrap();

        assert!(!window.is_empty());
        // Should include critical items
        assert!(window
            .content()
            .iter()
            .any(|item| item.priority == Priority::Critical));
    }

    #[test]
    fn test_assembler_metrics() {
        let mut assembler = ContextAssembler::new();

        let items = vec![ContextItem::new("test", Priority::High)];
        let _ = assembler.assemble_from_items(&items, 1000);

        let metrics = assembler.metrics();
        assert_eq!(metrics.len(), 1);
        assert!(metrics[0].quality_score > 0.0);
    }

    #[test]
    fn test_assembler_average_quality() {
        let mut assembler = ContextAssembler::new();

        let items = vec![ContextItem::new("test", Priority::High)];
        let _ = assembler.assemble_from_items(&items, 1000);

        let avg_quality = assembler.average_quality();
        assert!(avg_quality > 0.0 && avg_quality <= 1.0);
    }
}
