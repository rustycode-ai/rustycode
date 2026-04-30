// ── Context Prioritization Scoring Functions ───────────────────────────────────

#[allow(unused_imports)]
use super::types::{ContextItem, Priority};
use std::cmp::Ordering;

/// Strategy for sorting context items.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortStrategy {
    /// Sort by raw score (highest first)
    ByScore,
    /// Sort by score per token (efficiency)
    ByScorePerToken,
    /// Sort by token count (smallest first - good for coverage)
    ByTokenCount,
    /// Sort by priority level (highest first)
    ByPriority,
}

/// Sort context items by the specified strategy.
///
/// # Arguments
///
/// * `items` - Items to sort (will be modified in place)
/// * `strategy` - Sorting strategy to use
pub fn sort_by<T>(items: &mut [ContextItem<T>], strategy: SortStrategy)
where
    T: AsRef<str>,
{
    match strategy {
        SortStrategy::ByScore => {
            items.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(Ordering::Equal));
        }
        SortStrategy::ByScorePerToken => {
            items.sort_by(|a, b| {
                b.score_per_token()
                    .partial_cmp(&a.score_per_token())
                    .unwrap_or(Ordering::Equal)
            });
        }
        SortStrategy::ByTokenCount => {
            items.sort_by_key(|a| a.token_count);
        }
        SortStrategy::ByPriority => {
            items.sort_by_key(|a| std::cmp::Reverse(a.priority));
        }
    }
}

/// Select the best items given a token budget.
///
/// This function prioritizes items by score and selects as many as will
/// fit within the budget.
///
/// # Arguments
///
/// * `items` - Items to select from
/// * `budget` - Token budget
///
/// # Returns
///
/// Selected items that fit within the budget
pub fn select_best<T>(items: &[ContextItem<T>], budget: usize) -> Vec<&ContextItem<T>>
where
    T: AsRef<str>,
{
    let mut sorted: Vec<_> = items.iter().collect();

    // Sort by score (highest first)
    sorted.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(Ordering::Equal));

    // Select items that fit in budget
    let mut result = Vec::new();
    let mut used: usize = 0;

    for item in sorted {
        let new_total = used.saturating_add(item.token_count);
        if new_total <= budget {
            result.push(item);
            used = new_total;
        } else {
            // Budget exhausted, stop adding items
            break;
        }
    }

    result
}

/// Select items with knapsack-style optimization.
///
/// This is a more sophisticated approach that considers both score and
/// token cost to maximize total value within the budget.
///
/// # Arguments
///
/// * `items` - Items to select from
/// * `budget` - Token budget
///
/// # Returns
///
/// Selected items that maximize value within budget
///
/// # Note
///
/// This is a greedy approximation (not optimal but fast). For optimal
/// results with small item counts, use dynamic programming.
pub fn select_knapsack<T>(items: &[ContextItem<T>], budget: usize) -> Vec<&ContextItem<T>>
where
    T: AsRef<str>,
{
    // Calculate value density (score per token)
    let mut with_density: Vec<_> = items
        .iter()
        .map(|item| (item, item.score_per_token()))
        .collect();

    // Sort by density (highest first) - greedy knapsack
    with_density.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

    // Select items that fit
    let mut result = Vec::new();
    let mut used: usize = 0;

    for (item, _density) in with_density {
        let new_total = used.saturating_add(item.token_count);
        if new_total <= budget {
            result.push(item);
            used = new_total;
        }
    }

    result
}

/// Calculate a relevance score based on keyword matching.
///
/// # Arguments
///
/// * `content` - Content to score
/// * `keywords` - Keywords to look for
///
/// # Returns
///
/// Relevance score (higher is more relevant)
pub fn keyword_relevance_score(content: &str, keywords: &[&str]) -> f64 {
    if keywords.is_empty() {
        return 0.0;
    }

    let content_lower = content.to_lowercase();
    let mut matches = 0;

    for keyword in keywords {
        let keyword_lower = keyword.to_lowercase();
        if content_lower.contains(&keyword_lower) {
            matches += 1;
        }
    }

    // Calculate score as ratio of matches to total keywords
    (matches as f64 / keywords.len() as f64) * 100.0
}

/// Calculate a recency score based on timestamp.
///
/// More recent items get higher scores. The score decays exponentially
/// over time with a half-life of 24 hours.
///
/// # Arguments
///
/// * `timestamp` - Timestamp to score
///
/// # Returns
///
/// Recency score (0.0 to 100.0, higher is more recent)
pub fn recency_score(timestamp: chrono::DateTime<chrono::Utc>) -> f64 {
    let age_hours = (chrono::Utc::now() - timestamp).num_hours().max(0) as f64;

    // Exponential decay with half-life of 24 hours
    let half_life = 24.0;
    let decay_factor = 0.5_f64.powf(age_hours / half_life);

    100.0 * decay_factor
}

/// Calculate a frequency score based on usage count.
///
/// More frequently used items get higher scores. The score uses
/// logarithmic scaling to avoid excessive bias.
///
/// # Arguments
///
/// * `usage_count` - Number of times the item was used
///
/// # Returns
///
/// Frequency score (0.0 to 100.0, higher is more frequent)
pub fn frequency_score(usage_count: usize) -> f64 {
    if usage_count == 0 {
        return 0.0;
    }

    // Logarithmic scaling: ln(count) * constant
    let base = (usage_count as f64).ln();
    (base * 20.0).min(100.0) // Cap at 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_sort_by_score() {
        let mut items = vec![
            ContextItem::new("low", Priority::Low),
            ContextItem::new("high priority with more content", Priority::High),
            ContextItem::new("medium content here", Priority::Medium),
        ];

        sort_by(&mut items, SortStrategy::ByScore);

        let high_pos = items
            .iter()
            .position(|i| i.priority == Priority::High)
            .unwrap();
        let low_pos = items
            .iter()
            .position(|i| i.priority == Priority::Low)
            .unwrap();

        assert!(
            high_pos < low_pos,
            "High priority ({}) should come before low priority ({})",
            high_pos,
            low_pos
        );
    }

    #[test]
    fn test_sort_by_token_count() {
        let mut items = vec![
            ContextItem::new("small", Priority::High),
            ContextItem::new("this is a much larger piece of text content", Priority::Low),
        ];

        sort_by(&mut items, SortStrategy::ByTokenCount);

        assert!(items[0].token_count < items[1].token_count);
    }

    #[test]
    fn test_sort_by_priority() {
        let mut items = vec![
            ContextItem::new("low", Priority::Low),
            ContextItem::new("critical", Priority::Critical),
            ContextItem::new("medium", Priority::Medium),
        ];

        sort_by(&mut items, SortStrategy::ByPriority);

        assert_eq!(items[0].priority, Priority::Critical);
        assert_eq!(items[2].priority, Priority::Low);
    }

    #[test]
    fn test_select_best() {
        let items = vec![
            ContextItem::new("low priority item with some content", Priority::Low),
            ContextItem::new("critical", Priority::Critical),
            ContextItem::new("medium priority content here", Priority::Medium),
        ];

        let budget = 50;
        let selected = select_best(&items, budget);

        assert!(!selected.is_empty());
        assert!(selected
            .iter()
            .any(|item| item.priority == Priority::Critical));
    }

    #[test]
    fn test_select_best_empty() {
        let items: Vec<ContextItem<&str>> = vec![];
        let selected = select_best(&items, 100);

        assert!(selected.is_empty());
    }

    #[test]
    fn test_select_knapsack() {
        let items = vec![
            ContextItem::new(
                "large low priority content that uses many tokens",
                Priority::Low,
            ),
            ContextItem::new("small", Priority::Critical),
            ContextItem::new("medium", Priority::Medium),
        ];

        let budget = 50;
        let selected = select_knapsack(&items, budget);

        assert!(!selected.is_empty());
    }

    #[test]
    fn test_keyword_relevance_score() {
        let content = "This is about Rust programming and async code";

        let score1 = keyword_relevance_score(content, &["rust", "programming"]);
        let score2 = keyword_relevance_score(content, &["python", "javascript"]);

        assert!(score1 > score2);
    }

    #[test]
    fn test_recency_score() {
        let now = Utc::now();
        let old = now - chrono::Duration::days(30);

        let score_now = recency_score(now);
        let score_old = recency_score(old);

        assert!(score_now > score_old);
        assert!(score_now <= 100.0);
    }

    #[test]
    fn test_frequency_score() {
        let score1 = frequency_score(0);
        let score10 = frequency_score(10);
        let score100 = frequency_score(100);

        assert_eq!(score1, 0.0);
        assert!(score10 > 0.0);
        assert!(score100 > score10);
        assert!(score100 <= 100.0);
    }
}
