//! /review slash command implementation
//!
//! Provides automated code review functionality including:
//! - Git diff analysis
//! - Issue detection (bugs, security, performance, code smells)
//! - Improvement suggestions
//! - Code quality grading

use crate::info_log;
use anyhow::Result;
pub use rustycode_tools::code_review::{CodeReviewAnalyzer, Issue, Severity};
use std::path::PathBuf;

/// Handle /review slash command
pub async fn handle_review_command(workspace_path: Option<PathBuf>) -> Result<String> {
    info_log!("Starting code review analysis...");

    // Determine repository path
    let repo_path = workspace_path
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Check if we're in a git repository
    if !repo_path.join(".git").exists() {
        return Ok(
            "⚠️  Not a git repository. Please run /review from within a git repo.".to_string(),
        );
    }

    // Create analyzer
    let analyzer = CodeReviewAnalyzer::new();

    // Run analysis
    info_log!("Analyzing git diff...");
    let review_result = analyzer.analyze_diff(&repo_path)?;

    // Format results
    let output = format_review_results(&review_result);

    info_log!("Code review complete: Grade {}", review_result.score.grade);

    Ok(output)
}

/// Format review results for display
fn format_review_results(result: &rustycode_tools::code_review::ReviewResult) -> String {
    let mut output = String::new();

    // Header
    output.push_str("╔══════════════════════════════════════════════════════════════╗\n");
    output.push_str("║                    CODE REVIEW RESULTS                          ║\n");
    output.push_str("╚══════════════════════════════════════════════════════════════╝\n\n");

    // Summary
    output.push_str(&format!("{}\n\n", result.summary));

    // Issues by severity
    if !result.issues.is_empty() {
        output.push_str("📋 ISSUES FOUND:\n\n");

        // Critical issues
        let critical: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Critical)
            .collect();

        if !critical.is_empty() {
            output.push_str("🔴 CRITICAL:\n");
            for issue in critical {
                output.push_str(&format!(
                    "  • {}: {} ({}:{})\n",
                    issue.issue_type_as_str(),
                    issue.message,
                    issue.file,
                    issue.line
                ));
                if let Some(snippet) = &issue.code_snippet {
                    output.push_str(&format!("    Code: {}\n", snippet));
                }
            }
            output.push('\n');
        }

        // Errors
        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();

        if !errors.is_empty() {
            output.push_str("❌ ERRORS:\n");
            for issue in errors {
                output.push_str(&format!(
                    "  • {}: {} ({}:{})\n",
                    issue.issue_type_as_str(),
                    issue.message,
                    issue.file,
                    issue.line
                ));
            }
            output.push('\n');
        }

        // Warnings
        let warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .collect();

        if !warnings.is_empty() {
            output.push_str("⚠️  WARNINGS:\n");
            for issue in warnings {
                output.push_str(&format!(
                    "  • {}: {} ({}:{})\n",
                    issue.issue_type_as_str(),
                    issue.message,
                    issue.file,
                    issue.line
                ));
            }
            output.push('\n');
        }

        // Info
        let info: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Info)
            .collect();

        if !info.is_empty() {
            output.push_str("ℹ️  INFO:\n");
            for issue in info {
                output.push_str(&format!(
                    "  • {}: {} ({}:{})\n",
                    issue.issue_type_as_str(),
                    issue.message,
                    issue.file,
                    issue.line
                ));
            }
            output.push('\n');
        }
    } else {
        output.push_str("✅ No issues found! Code looks great.\n\n");
    }

    // Suggestions
    if !result.suggestions.is_empty() {
        output.push_str("💡 SUGGESTIONS:\n\n");
        for suggestion in &result.suggestions {
            output.push_str(&format!(
                "  • [{}] {}\n",
                suggestion.category_as_str(),
                suggestion.message
            ));
            if let Some(example) = &suggestion.example {
                output.push_str(&format!("    Example: {}\n", example));
            }
        }
        output.push('\n');
    }

    // Grade indicator
    let grade_emoji = match result.score.grade {
        'A' => "🌟",
        'B' => "👍",
        'C' => "⚠️",
        'D' => "❌",
        'F' => "🚨",
        _ => "?",
    };

    output.push_str(&format!(
        "FINAL GRADE: {} {} ({} issues, {} lines reviewed)\n",
        grade_emoji, result.score.grade, result.score.issues_found, result.score.lines_reviewed
    ));

    output
}

// Helper trait for formatting
trait IssueTypeDisplay {
    fn issue_type_as_str(&self) -> &'static str;
}

impl IssueTypeDisplay for Issue {
    fn issue_type_as_str(&self) -> &'static str {
        match self.issue_type {
            rustycode_tools::code_review::IssueType::Bug => "Bug",
            rustycode_tools::code_review::IssueType::SecurityVulnerability => "Security",
            rustycode_tools::code_review::IssueType::CodeSmell => "Code Smell",
            rustycode_tools::code_review::IssueType::PerformanceIssue => "Performance",
            rustycode_tools::code_review::IssueType::StyleViolation => "Style",
            rustycode_tools::code_review::IssueType::DocumentationIssue => "Documentation",
            #[allow(unreachable_patterns)]
            _ => "Other",
        }
    }
}

trait SuggestionCategoryDisplay {
    fn category_as_str(&self) -> &'static str;
}

impl SuggestionCategoryDisplay for rustycode_tools::code_review::Suggestion {
    fn category_as_str(&self) -> &'static str {
        match self.category {
            rustycode_tools::code_review::SuggestionCategory::Performance => "Performance",
            rustycode_tools::code_review::SuggestionCategory::Readability => "Readability",
            rustycode_tools::code_review::SuggestionCategory::Maintainability => "Maintainability",
            rustycode_tools::code_review::SuggestionCategory::Security => "Security",
            rustycode_tools::code_review::SuggestionCategory::BestPractice => "Best Practice",
            #[allow(unreachable_patterns)]
            _ => "Other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_review_command_not_a_repo() {
        // Test with non-git directory
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        let result = rt.block_on(async {
            handle_review_command(Some(PathBuf::from("/tmp/nonexistent"))).await
        });

        assert!(result.is_ok());
        let msg = result.expect("Expected Ok result");
        assert!(
            msg.contains("Not a git repository"),
            "Unexpected message: {}",
            msg
        );
    }
}
