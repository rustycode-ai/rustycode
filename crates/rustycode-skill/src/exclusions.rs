//! Exclusion clauses for skill activation accuracy.
//!
//! Skills can declare phrases that should prevent them from being suggested
//! when the user's request clearly belongs elsewhere.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExclusionClauseSet {
    clauses: Vec<String>,
}

impl ExclusionClauseSet {
    pub fn from_list(raw: &[String]) -> Self {
        let clauses = raw
            .iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        Self { clauses }
    }

    pub fn matches_any(&self, context: &str) -> bool {
        let context_terms = normalize_terms(context);
        self.clauses
            .iter()
            .map(|clause| normalize_terms(clause))
            .any(|clause_terms| {
                clause_terms
                    .iter()
                    .all(|term| term_matches(&context_terms, term))
            })
    }

    pub const fn len(&self) -> usize {
        self.clauses.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }
}

fn normalize_terms(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(normalize_term)
        .collect()
}

fn normalize_term(term: &str) -> String {
    let trimmed = term.trim().to_lowercase();
    if trimmed.len() > 3 && trimmed.ends_with('s') && !trimmed.ends_with("ss") {
        trimmed.trim_end_matches('s').to_string()
    } else {
        trimmed
    }
}

fn term_matches(context_terms: &[String], term: &str) -> bool {
    context_terms.iter().any(|context_term| {
        context_term == term
            || context_term.starts_with(term)
            || term.starts_with(context_term)
            || common_prefix_len(context_term, term) >= 5
    })
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars()
        .zip(b.chars())
        .take_while(|(left, right)| left == right)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_exclusions() {
        let clauses = ExclusionClauseSet::from_list(&[]);
        assert!(clauses.is_empty());
        assert!(!clauses.matches_any("deploy to production"));
    }

    #[test]
    fn parse_string_exclusions() {
        let clauses = ExclusionClauseSet::from_list(&[
            "blog articles".to_string(),
            "documentation generation".to_string(),
            "newsletter".to_string(),
        ]);
        assert_eq!(clauses.len(), 3);
        assert!(clauses.matches_any("write a blog article about Rust"));
        assert!(clauses.matches_any("generate documentation for this API"));
        assert!(clauses.matches_any("send a newsletter"));
        assert!(!clauses.matches_any("implement a sorting algorithm"));
    }

    #[test]
    fn exclusion_matching_is_case_insensitive() {
        let clauses = ExclusionClauseSet::from_list(&["BLOG ARTICLES".to_string()]);
        assert!(clauses.matches_any("write a blog article"));
    }

    #[test]
    fn exclusion_matches_partial_words() {
        let clauses = ExclusionClauseSet::from_list(&["newsletter".to_string()]);
        assert!(clauses.matches_any("send newsletters"));
    }

    #[test]
    fn from_list_with_whitespace() {
        let clauses = ExclusionClauseSet::from_list(&[
            "  blog articles  ".to_string(),
            String::new(),
            "documentation  ".to_string(),
        ]);
        assert_eq!(clauses.len(), 2);
    }
}
