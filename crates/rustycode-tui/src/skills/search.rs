//! Fuzzy search for skills
//!
//! Provides fuzzy matching with relevance ranking for skill search.

use crate::skills::loader::Skill;

/// Search result with relevance score
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The matching skill
    pub skill: Skill,

    /// Relevance score (higher = better match)
    pub score: i32,
}

/// Fuzzy search skills by query
///
/// Matches query against skill name and description with relevance ranking:
/// - Exact match: 100 points
/// - Prefix match: 80 points
/// - Substring match: 60 points
/// - Fuzzy match: 40 points
///
/// # Arguments
///
/// * `query` - Search query string
/// * `skills` - List of skills to search
///
/// # Returns
///
/// Vector of search results sorted by relevance (highest score first)
pub fn fuzzy_match(query: &str, skills: &[Skill]) -> Vec<Skill> {
    if query.is_empty() {
        // Return all skills if no query
        return skills.to_vec();
    }

    let query_lower = query.to_lowercase();

    let mut results: Vec<SearchResult> = skills
        .iter()
        .filter_map(|skill| {
            let score = calculate_score(&query_lower, skill);
            if score > 0 {
                Some(SearchResult {
                    skill: skill.clone(),
                    score,
                })
            } else {
                None
            }
        })
        .collect();

    // Sort by score (highest first)
    results.sort_by_key(|a| std::cmp::Reverse(a.score));

    // Extract skills in order
    results.into_iter().map(|r| r.skill).collect()
}

/// Calculate relevance score for a skill
///
/// Score calculation:
/// - Exact name match: 100
/// - Prefix name match: 80
/// - Substring name match: 60
/// - Exact description match: 50
/// - Prefix description match: 40
/// - Substring description match: 30
/// - Fuzzy name match: 20
/// - Fuzzy description match: 10
fn calculate_score(query: &str, skill: &Skill) -> i32 {
    let name_lower = skill.name.to_lowercase();
    let desc_lower = skill.description.to_lowercase();

    // Check for exact matches
    if name_lower == query {
        return 100;
    }

    // Check for prefix matches
    if name_lower.starts_with(query) {
        return 80;
    }

    // Check for substring matches in name
    if name_lower.contains(query) {
        return 60;
    }

    // Check for exact matches in description
    if desc_lower == query {
        return 50;
    }

    // Check for prefix matches in description
    if desc_lower.starts_with(query) {
        return 40;
    }

    // Check for substring matches in description
    if desc_lower.contains(query) {
        return 30;
    }

    // Check for fuzzy matches in name
    if fuzzy_match_chars(query, &name_lower) {
        return 20;
    }

    // Check for fuzzy matches in description
    if fuzzy_match_chars(query, &desc_lower) {
        return 10;
    }

    0
}

/// Fuzzy character matching
///
/// Returns true if all characters in query appear in text in order
/// (not necessarily contiguous)
fn fuzzy_match_chars(query: &str, text: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    let mut query_chars = query.chars().peekable();
    let mut text_chars = text.chars();

    while let Some(qc) = query_chars.peek() {
        loop {
            match text_chars.next() {
                Some(tc) if tc == *qc => {
                    query_chars.next();
                    break;
                }
                Some(_) => continue,
                None => return false,
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::loader::{Skill, SkillCategory};

    fn create_test_skill(name: &str, description: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: description.to_string(),
            category: SkillCategory::Tools,
            parameters: vec![],
            commands: vec![],
            instructions: format!("Use this skill for: {}", description),
            path: std::path::PathBuf::from("/test"),
        }
    }

    #[test]
    fn test_exact_name_match() {
        let skills = vec![
            create_test_skill("code-review", "Review code"),
            create_test_skill("tdd-guide", "TDD workflow"),
        ];

        let results = fuzzy_match("code-review", &skills);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "code-review");
    }

    #[test]
    fn test_prefix_match() {
        let skills = vec![
            create_test_skill("code-review", "Review code"),
            create_test_skill("tdd-guide", "TDD workflow"),
        ];

        let results = fuzzy_match("code", &skills);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "code-review");
    }

    #[test]
    fn test_description_match() {
        let skills = vec![
            create_test_skill("code-review", "Review code for quality"),
            create_test_skill("tdd-guide", "Test-driven development"),
        ];

        let results = fuzzy_match("quality", &skills);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "code-review");
    }

    #[test]
    fn test_fuzzy_match() {
        let skills = vec![
            create_test_skill("code-review", "Review code"),
            create_test_skill("security-review", "Security analysis"),
        ];

        let results = fuzzy_match("cod", &skills);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "code-review");
    }

    #[test]
    fn test_empty_query() {
        let skills = vec![
            create_test_skill("code-review", "Review code"),
            create_test_skill("tdd-guide", "TDD workflow"),
        ];

        let results = fuzzy_match("", &skills);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_no_match() {
        let skills = vec![
            create_test_skill("code-review", "Review code"),
            create_test_skill("tdd-guide", "TDD workflow"),
        ];

        let results = fuzzy_match("xyz", &skills);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_fuzzy_match_chars() {
        assert!(fuzzy_match_chars("cr", "code-review"));
        assert!(fuzzy_match_chars("cr", "code review"));
        assert!(fuzzy_match_chars("tdg", "tdd-guide"));
        assert!(!fuzzy_match_chars("xyz", "code-review"));
        assert!(fuzzy_match_chars("", "anything"));
    }

    #[test]
    fn test_relevance_ordering() {
        let skills = vec![
            create_test_skill("code-review", "Review code"),
            create_test_skill("code-quality", "Code quality checks"),
            create_test_skill("tdd-guide", "TDD workflow"),
        ];

        let results = fuzzy_match("code", &skills);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "code-review"); // prefix match
        assert_eq!(results[1].name, "code-quality"); // substring match
    }
}
