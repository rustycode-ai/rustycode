//! Team Learnings — Project-specific memory that persists across tasks.
//!
//! This module provides a simple, auditable memory system for the team:
//!
//! ## What It Stores
//!
//! - **User Preferences**: How the user likes to work (approval style, output format, etc.)
//! - **Codebase Quirks**: Project-specific patterns (test locations, auth requirements, etc.)
//! - **What's Worked**: Strategies that succeeded repeatedly
//! - **What's Failed**: Mistakes that happened multiple times
//!
//! ## Design Principles
//!
//! 1. **Auditable**: All learnings stored in human-readable markdown
//! 2. **Editable**: User can directly modify learnings
//! 3. **Dated**: Each entry shows when it was learned
//! 4. **Project-scoped**: Learnings never leak across projects
//! 5. **Minimal**: Only notable events are recorded (not every detail)
//!
//! ## File Format
//!
//! Learnings are stored in `TEAM_LEARNINGS.md` at the project root:
//!
//! ```markdown
//! # Team Learnings
//!
//! Last updated: 2026-04-04
//!
//! ## User Preferences
//! - Prefers concise answers over detailed explanations
//! - Requires approval for auth/security changes
//!
//! ## Codebase Quirks
//! - Tests live in /tests not alongside source (learned: 2026-04-04)
//! - /auth module requires extra review (learned: 2026-04-04)
//!
//! ## What's Worked
//! - Small PRs get faster feedback (learned: 2026-04-04, 3 times)
//!
//! ## What's Failed
//! - Assuming file paths exist without checking (learned: 2026-04-04, 2 times)
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use rustycode_core::team::team_learnings::{TeamLearnings, LearningEntry};
//!
//! // Load existing learnings
//! let mut learnings = TeamLearnings::load(&project_root)?;
//!
//! // Add a new learning
//! learnings.record(LearningEntry {
//!     category: LearningCategory::CodebaseQuirk,
//!     content: "Tests live in /tests directory".to_string(),
//!     source_task: "Refactor auth module".to_string(),
//!     confidence: 0.8,
//! });
//!
//! // Save back to file
//! learnings.save()?;
//!
//! // Read all learnings (for Architect briefing)
//! let all = learnings.get_all();
//! ```

use anyhow::{Context, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A single learning entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningEntry {
    /// When this was learned (YYYY-MM-DD format)
    pub learned_date: String,
    /// Which learning session this came from
    pub source_session: Option<String>,
    /// The learning content
    pub content: String,
    /// How many times this pattern has been observed
    pub occurrence_count: u32,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
}

/// Categories of learnings
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LearningCategory {
    UserPreference,
    CodebaseQuirk,
    WhatWorked,
    WhatFailed,
}

impl LearningCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            LearningCategory::UserPreference => "User Preferences",
            LearningCategory::CodebaseQuirk => "Codebase Quirks",
            LearningCategory::WhatWorked => "What's Worked",
            LearningCategory::WhatFailed => "What's Failed",
        }
    }

    /// Parse a category from its display name.
    /// Named `parse` to avoid confusion with the standard `FromStr` trait.
    pub fn parse_category(s: &str) -> Option<Self> {
        match s {
            "User Preferences" => Some(LearningCategory::UserPreference),
            "Codebase Quirks" => Some(LearningCategory::CodebaseQuirk),
            "What's Worked" => Some(LearningCategory::WhatWorked),
            "What's Failed" => Some(LearningCategory::WhatFailed),
            _ => None,
        }
    }
}

/// Team learnings stored in TEAM_LEARNINGS.md
#[derive(Debug, Default)]
pub struct TeamLearnings {
    /// Path to the learnings file
    file_path: PathBuf,
    /// Last updated date
    last_updated: String,
    /// Learnings by category
    entries: HashMap<LearningCategory, Vec<LearningEntry>>,
}

impl TeamLearnings {
    /// Create a new empty learnings store
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let project_root = project_root.into();
        let file_path = project_root.join("TEAM_LEARNINGS.md");
        Self {
            file_path,
            last_updated: Local::now().format("%Y-%m-%d").to_string(),
            entries: HashMap::new(),
        }
    }

    /// Load learnings from TEAM_LEARNINGS.md
    pub fn load(project_root: impl Into<PathBuf>) -> Result<Self> {
        let project_root = project_root.into();
        let file_path = project_root.join("TEAM_LEARNINGS.md");

        if !file_path.exists() {
            // Return empty learnings if file doesn't exist
            return Ok(Self::new(project_root));
        }

        let content = fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read {:?}", file_path))?;

        Self::parse(&content, file_path)
    }

    /// Parse markdown content into learnings
    fn parse(content: &str, file_path: PathBuf) -> Result<Self> {
        let mut entries: HashMap<LearningCategory, Vec<LearningEntry>> = HashMap::new();
        let mut last_updated = Local::now().format("%Y-%m-%d").to_string();
        let mut current_category: Option<LearningCategory> = None;

        for line in content.lines() {
            // Parse last updated date
            if line.starts_with("Last updated:") {
                last_updated = line
                    .strip_prefix("Last updated:")
                    .unwrap_or("")
                    .trim()
                    .to_string();
                continue;
            }

            // Parse category headers
            if line.starts_with("## ") {
                let category_name = line.strip_prefix("## ").unwrap_or("").trim();
                current_category = LearningCategory::parse_category(category_name);
                entries
                    .entry(
                        current_category
                            .clone()
                            .unwrap_or(LearningCategory::UserPreference),
                    )
                    .or_default();
                continue;
            }

            // Parse learning entries (lines starting with "- ")
            if let Some(category) = current_category.as_ref() {
                if let Some(content_raw) = line.strip_prefix("- ") {
                    let content = content_raw.trim().to_string();

                    // Try to extract date from "(learned: YYYY-MM-DD)" pattern
                    let (content, learned_date, session, count, confidence) =
                        Self::parse_learning_metadata(&content);

                    entries
                        .entry(category.clone())
                        .or_default()
                        .push(LearningEntry {
                            learned_date,
                            source_session: session,
                            content,
                            occurrence_count: count,
                            confidence,
                        });
                }
            }
        }

        Ok(Self {
            file_path,
            last_updated,
            entries,
        })
    }

    /// Parse metadata from learning content
    /// Returns: (clean_content, date, session, count, confidence)
    fn parse_learning_metadata(content: &str) -> (String, String, Option<String>, u32, f64) {
        let mut learned_date = Local::now().format("%Y-%m-%d").to_string();
        let mut session = None;
        let mut count = 1u32;
        let confidence = 0.5f64;
        let mut clean_content = content.to_string();

        // Extract "(learned: YYYY-MM-DD[, session_name][, N times])"
        if let Some(start) = content.find("(learned:") {
            if let Some(end) = content[start..].find(')') {
                let metadata = &content[start + 9..start + end];

                // Parse date (first comma-separated value)
                let parts: Vec<&str> = metadata.split(',').collect();
                if !parts.is_empty() {
                    learned_date = parts[0].trim().to_string();
                }

                // Parse session (second value)
                if parts.len() > 1 {
                    session = Some(parts[1].trim().to_string());
                }

                // Parse count (look for "N times" pattern)
                for part in parts {
                    let part = part.trim();
                    if let Some(count_str) = part.strip_suffix(" times") {
                        if let Ok(n) = count_str.parse::<u32>() {
                            count = n;
                        }
                    }
                }

                // Remove the metadata from content
                clean_content = content[..start].trim().to_string();
            }
        }

        (clean_content, learned_date, session, count, confidence)
    }

    /// Save learnings to TEAM_LEARNINGS.md
    pub fn save(&self) -> Result<()> {
        let content = self.to_markdown();
        fs::write(&self.file_path, &content)
            .with_context(|| format!("Failed to write {:?}", self.file_path))?;
        Ok(())
    }

    /// Convert learnings to markdown format
    pub fn to_markdown(&self) -> String {
        let mut lines = Vec::new();

        lines.push("# Team Learnings".to_string());
        lines.push(String::new());
        lines.push(format!("Last updated: {}", self.last_updated));
        lines.push(String::new());

        // Write categories in order
        let categories = [
            LearningCategory::UserPreference,
            LearningCategory::CodebaseQuirk,
            LearningCategory::WhatWorked,
            LearningCategory::WhatFailed,
        ];

        for category in &categories {
            if let Some(entries) = self.entries.get(category) {
                if !entries.is_empty() {
                    lines.push(format!("## {}", category.as_str()));

                    for entry in entries {
                        let mut line = format!("- {}", entry.content);

                        // Add metadata
                        let mut metadata = Vec::new();
                        metadata.push(format!("learned: {}", entry.learned_date));
                        if let Some(ref session) = entry.source_session {
                            metadata.push(session.clone());
                        }
                        if entry.occurrence_count > 1 {
                            metadata.push(format!("{} times", entry.occurrence_count));
                        }

                        if !metadata.is_empty() {
                            line.push_str(&format!(" ({})", metadata.join(", ")));
                        }

                        lines.push(line);
                    }

                    lines.push(String::new());
                }
            }
        }

        // Remove trailing empty line
        while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            lines.pop();
        }

        lines.join("\n")
    }

    /// Record a new learning
    pub fn record(
        &mut self,
        category: LearningCategory,
        content: String,
        source_session: Option<String>,
    ) {
        self.last_updated = Local::now().format("%Y-%m-%d").to_string();

        // Check if similar learning already exists
        let entries = self.entries.entry(category.clone()).or_default();

        // Look for similar content (simple string match)
        for existing in entries.iter_mut() {
            if existing.content == content {
                existing.occurrence_count += 1;
                existing.learned_date = self.last_updated.clone();
                return;
            }
        }

        // Add new learning
        entries.push(LearningEntry {
            learned_date: self.last_updated.clone(),
            source_session,
            content,
            occurrence_count: 1,
            confidence: 0.5,
        });
    }

    /// Get all learnings as formatted text (for briefing)
    pub fn get_all(&self) -> String {
        self.to_markdown()
    }

    /// Get learnings for a specific category
    pub fn get_category(&self, category: &LearningCategory) -> Vec<&LearningEntry> {
        self.entries
            .get(category)
            .map(|entries| entries.iter().collect())
            .unwrap_or_default()
    }

    /// Get high-confidence learnings (observed 2+ times)
    pub fn get_reliable(&self) -> Vec<&LearningEntry> {
        let mut reliable = Vec::new();
        for entries in self.entries.values() {
            for entry in entries {
                if entry.occurrence_count >= 2 {
                    reliable.push(entry);
                }
            }
        }
        reliable
    }

    /// Remove a learning by content match
    pub fn remove(&mut self, category: &LearningCategory, content: &str) -> bool {
        if let Some(entries) = self.entries.get_mut(category) {
            let initial_len = entries.len();
            entries.retain(|e| e.content != content);
            self.last_updated = Local::now().format("%Y-%m-%d").to_string();
            entries.len() != initial_len
        } else {
            false
        }
    }

    /// Clear all learnings (use with caution)
    pub fn clear(&mut self) {
        self.entries.clear();
        self.last_updated = Local::now().format("%Y-%m-%d").to_string();
    }

    /// Get the file path
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Check if learnings file exists
    pub fn exists(&self) -> bool {
        self.file_path.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_new_learnings() {
        let temp_dir = TempDir::new().unwrap();
        let learnings = TeamLearnings::new(temp_dir.path());

        assert_eq!(
            learnings.file_path(),
            temp_dir.path().join("TEAM_LEARNINGS.md")
        );
        assert!(!learnings.exists());
    }

    #[test]
    fn test_record_and_save() {
        let temp_dir = TempDir::new().unwrap();
        let mut learnings = TeamLearnings::new(temp_dir.path());

        learnings.record(
            LearningCategory::CodebaseQuirk,
            "Tests live in /tests directory".to_string(),
            Some("auth refactor".to_string()),
        );

        assert!(learnings.save().is_ok());
        assert!(learnings.exists());

        // Verify file content
        let content = fs::read_to_string(learnings.file_path()).unwrap();
        assert!(content.contains("Tests live in /tests directory"));
        assert!(content.contains("Codebase Quirks"));
    }

    #[test]
    fn test_load_existing() {
        let temp_dir = TempDir::new().unwrap();
        let mut learnings = TeamLearnings::new(temp_dir.path());

        learnings.record(
            LearningCategory::UserPreference,
            "Prefers concise answers".to_string(),
            None,
        );
        learnings.save().unwrap();

        // Load from file
        let loaded = TeamLearnings::load(temp_dir.path()).unwrap();
        let prefs = loaded.get_category(&LearningCategory::UserPreference);
        assert_eq!(prefs.len(), 1);
        assert!(prefs[0].content.contains("Prefers concise answers"));
    }

    #[test]
    fn test_occurrence_counting() {
        let temp_dir = TempDir::new().unwrap();
        let mut learnings = TeamLearnings::new(temp_dir.path());

        // Record same learning twice
        learnings.record(
            LearningCategory::WhatFailed,
            "Forgot to check file exists".to_string(),
            Some("task 1".to_string()),
        );
        learnings.record(
            LearningCategory::WhatFailed,
            "Forgot to check file exists".to_string(),
            Some("task 2".to_string()),
        );

        let failures = learnings.get_category(&LearningCategory::WhatFailed);
        assert_eq!(failures.len(), 1); // Should be deduplicated
        assert_eq!(failures[0].occurrence_count, 2);
    }

    #[test]
    fn test_get_reliable() {
        let temp_dir = TempDir::new().unwrap();
        let mut learnings = TeamLearnings::new(temp_dir.path());

        learnings.record(
            LearningCategory::WhatWorked,
            "Small PRs work better".to_string(),
            None,
        );
        learnings.record(
            LearningCategory::WhatWorked,
            "Small PRs work better".to_string(),
            None,
        );
        learnings.record(
            LearningCategory::WhatWorked,
            "New pattern".to_string(),
            None,
        );

        let reliable = learnings.get_reliable();
        assert_eq!(reliable.len(), 1); // Only the one with count >= 2
        assert!(reliable[0].content.contains("Small PRs"));
    }

    #[test]
    fn test_markdown_format() {
        let temp_dir = TempDir::new().unwrap();
        let mut learnings = TeamLearnings::new(temp_dir.path());

        learnings.record(
            LearningCategory::CodebaseQuirk,
            "Auth needs extra review".to_string(),
            Some("security audit".to_string()),
        );

        let markdown = learnings.to_markdown();
        assert!(markdown.contains("# Team Learnings"));
        assert!(markdown.contains("## Codebase Quirks"));
        assert!(markdown.contains("- Auth needs extra review"));
    }
}
