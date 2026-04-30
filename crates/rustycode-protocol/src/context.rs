//! Context types for RustyCode
//!
//! Context sections organize different types of information for the LLM,
//! allowing for fine-grained control over what's included in the prompt.

use serde::{Deserialize, Serialize};

/// Types of context sections that can be included in an LLM prompt.
///
/// Context sections organize different types of information for the LLM,
/// allowing for fine-grained control over what's included in the prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContextSectionKind {
    /// System instructions and behavior guidelines
    SystemInstructions,
    /// The current active task
    ActiveTask,
    /// Recent conversation turns
    RecentTurns,
    /// Tool schemas and descriptions
    ToolSchemas,
    /// Memory/knowledge base entries
    Memory,
    /// Git repository state
    GitState,
    /// Language server protocol state
    LspState,
    /// Available skills and their usage
    Skills,
    /// Code excerpts and snippets
    CodeExcerpts,
}

/// A section of context to be included in an LLM prompt.
///
/// Tracks how many tokens are reserved and used for each type of context,
/// enabling effective context window management.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextSection {
    /// The type of context section
    pub kind: ContextSectionKind,
    /// Number of tokens reserved for this section
    pub tokens_reserved: usize,
    /// Number of tokens actually used by this section
    pub tokens_used: usize,
    /// Items included in this section
    pub items: Vec<String>,
    /// Notes about this section
    pub note: String,
}

impl ContextSection {
    /// Create a new context section
    pub fn new(kind: ContextSectionKind, tokens_reserved: usize) -> Self {
        Self {
            kind,
            tokens_reserved,
            tokens_used: 0,
            items: Vec::new(),
            note: String::new(),
        }
    }

    /// Add an item to this section
    pub fn add_item(mut self, item: impl Into<String>) -> Self {
        self.items.push(item.into());
        self
    }

    /// Set the note for this section
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = note.into();
        self
    }

    /// Check if this section is within its token budget
    pub fn is_within_budget(&self) -> bool {
        self.tokens_used <= self.tokens_reserved
    }
}

/// A plan for assembling context for an LLM prompt.
///
/// Defines the total token budget and how it's allocated across different
/// types of context sections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextPlan {
    /// Total token budget for the prompt
    pub total_budget: usize,
    /// Total tokens reserved across all sections
    pub reserved_budget: usize,
    /// The sections that will be included
    pub sections: Vec<ContextSection>,
}

impl ContextPlan {
    /// Create a new context plan with the given total budget
    pub fn new(total_budget: usize) -> Self {
        Self {
            total_budget,
            reserved_budget: 0,
            sections: Vec::new(),
        }
    }

    /// Add a section to the plan
    pub fn add_section(mut self, section: ContextSection) -> Self {
        self.reserved_budget += section.tokens_reserved;
        self.sections.push(section);
        self
    }

    /// Check if the plan is within the total budget
    pub fn is_within_budget(&self) -> bool {
        self.reserved_budget <= self.total_budget
    }

    /// Get the remaining budget
    pub fn remaining_budget(&self) -> usize {
        self.total_budget.saturating_sub(self.reserved_budget)
    }

    /// Find a section by kind
    pub fn find_section(&self, kind: ContextSectionKind) -> Option<&ContextSection> {
        self.sections.iter().find(|s| s.kind == kind)
    }
}

impl Default for ContextPlan {
    fn default() -> Self {
        Self::new(128000) // Default to 128k tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_section_creation() {
        let section = ContextSection::new(ContextSectionKind::SystemInstructions, 1000);

        assert_eq!(section.kind, ContextSectionKind::SystemInstructions);
        assert_eq!(section.tokens_reserved, 1000);
        assert_eq!(section.tokens_used, 0);
        assert!(section.items.is_empty());
        assert!(section.is_within_budget());
    }

    #[test]
    fn test_context_section_builder() {
        let section = ContextSection::new(ContextSectionKind::CodeExcerpts, 500)
            .add_item("file1.rs")
            .add_item("file2.rs")
            .with_note("Main source files");

        assert_eq!(section.items.len(), 2);
        assert_eq!(section.note, "Main source files");
    }

    #[test]
    fn test_context_plan() {
        let plan = ContextPlan::new(10000)
            .add_section(ContextSection::new(
                ContextSectionKind::SystemInstructions,
                1000,
            ))
            .add_section(ContextSection::new(ContextSectionKind::CodeExcerpts, 5000));

        assert_eq!(plan.total_budget, 10000);
        assert_eq!(plan.reserved_budget, 6000);
        assert_eq!(plan.remaining_budget(), 4000);
        assert!(plan.is_within_budget());
    }

    #[test]
    fn test_context_plan_find_section() {
        let plan = ContextPlan::new(10000)
            .add_section(ContextSection::new(
                ContextSectionKind::SystemInstructions,
                1000,
            ))
            .add_section(ContextSection::new(ContextSectionKind::Memory, 2000));

        let system = plan.find_section(ContextSectionKind::SystemInstructions);
        assert!(system.is_some());
        assert_eq!(system.unwrap().tokens_reserved, 1000);

        let skills = plan.find_section(ContextSectionKind::Skills);
        assert!(skills.is_none());
    }

    #[test]
    fn test_context_plan_over_budget() {
        let plan = ContextPlan::new(1000)
            .add_section(ContextSection::new(ContextSectionKind::CodeExcerpts, 800))
            .add_section(ContextSection::new(ContextSectionKind::Memory, 500));

        assert!(!plan.is_within_budget());
        assert_eq!(plan.remaining_budget(), 0);
    }

    #[test]
    fn test_context_section_over_budget() {
        let mut section = ContextSection::new(ContextSectionKind::CodeExcerpts, 100);
        section.tokens_used = 200;
        assert!(!section.is_within_budget());
    }

    #[test]
    fn test_context_plan_default() {
        let plan = ContextPlan::default();
        assert_eq!(plan.total_budget, 128000);
        assert_eq!(plan.reserved_budget, 0);
        assert!(plan.sections.is_empty());
        assert!(plan.is_within_budget());
    }

    #[test]
    fn test_context_plan_remaining_budget_exact() {
        let plan = ContextPlan::new(5000)
            .add_section(ContextSection::new(ContextSectionKind::GitState, 5000));
        assert_eq!(plan.remaining_budget(), 0);
        assert!(plan.is_within_budget());
    }

    #[test]
    fn test_context_section_serde_roundtrip() {
        let section = ContextSection::new(ContextSectionKind::ToolSchemas, 2000)
            .add_item("Read tool")
            .add_item("Write tool")
            .with_note("Available tools");
        let json = serde_json::to_string(&section).unwrap();
        let back: ContextSection = serde_json::from_str(&json).unwrap();
        assert_eq!(section, back);
    }

    #[test]
    fn test_context_plan_serde_roundtrip() {
        let plan = ContextPlan::new(10000)
            .add_section(ContextSection::new(
                ContextSectionKind::SystemInstructions,
                1000,
            ))
            .add_section(ContextSection::new(ContextSectionKind::RecentTurns, 3000));
        let json = serde_json::to_string(&plan).unwrap();
        let back: ContextPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan, back);
    }

    #[test]
    fn test_context_section_kind_serde() {
        let kinds = vec![
            ContextSectionKind::SystemInstructions,
            ContextSectionKind::ActiveTask,
            ContextSectionKind::RecentTurns,
            ContextSectionKind::ToolSchemas,
            ContextSectionKind::Memory,
            ContextSectionKind::GitState,
            ContextSectionKind::LspState,
            ContextSectionKind::Skills,
            ContextSectionKind::CodeExcerpts,
        ];
        for k in &kinds {
            let json = serde_json::to_string(k).unwrap();
            let back: ContextSectionKind = serde_json::from_str(&json).unwrap();
            assert_eq!(*k, back);
        }
    }

    #[test]
    fn test_find_section_returns_first_match() {
        let plan = ContextPlan::new(10000)
            .add_section(ContextSection::new(ContextSectionKind::Memory, 1000))
            .add_section(ContextSection::new(ContextSectionKind::Memory, 2000));
        let found = plan.find_section(ContextSectionKind::Memory).unwrap();
        assert_eq!(found.tokens_reserved, 1000);
    }
}
