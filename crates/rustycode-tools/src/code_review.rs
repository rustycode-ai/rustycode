//! Code review and analysis tools
//!
//! This module provides automated code review capabilities including:
//! - Git diff analysis
//! - Bug detection patterns
//! - Security vulnerability scanning
//! - Code smell identification
//! - Performance optimization suggestions

use anyhow::{Context, Result};
use regex::Regex;
use std::path::Path;
use std::process::Command;

/// Code review result
#[derive(Debug, Clone)]
pub struct ReviewResult {
    pub issues: Vec<Issue>,
    pub suggestions: Vec<Suggestion>,
    pub score: ReviewScore,
    pub summary: String,
}

/// Individual issue found during review
#[derive(Debug, Clone)]
pub struct Issue {
    pub issue_type: IssueType,
    pub severity: Severity,
    pub file: String,
    pub line: usize,
    pub message: String,
    pub code_snippet: Option<String>,
}

/// Type of issue
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IssueType {
    Bug,
    SecurityVulnerability,
    CodeSmell,
    PerformanceIssue,
    StyleViolation,
    DocumentationIssue,
}

/// Severity level
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
#[non_exhaustive]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Improvement suggestion
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub category: SuggestionCategory,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub example: Option<String>,
}

/// Category of suggestion
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SuggestionCategory {
    Performance,
    Readability,
    Maintainability,
    Security,
    BestPractice,
}

/// Review score (A-F grading)
#[derive(Debug, Clone)]
pub struct ReviewScore {
    pub grade: char,
    pub issues_found: usize,
    pub suggestions_count: usize,
    pub lines_reviewed: usize,
}

/// Code review analyzer
pub struct CodeReviewAnalyzer {
    pub rules: Vec<ReviewRule>,
}

impl Default for CodeReviewAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeReviewAnalyzer {
    /// Create a new code review analyzer with default rules
    pub fn new() -> Self {
        let rules = vec![
            // Security patterns
            ReviewRule {
                name: "Hardcoded credentials".to_string(),
                pattern: Regex::new(r"(password|api_key|secret)\s*=").unwrap(),
                severity: Severity::Critical,
                issue_type: IssueType::SecurityVulnerability,
                message: "Possible hardcoded credential detected".to_string(),
            },
            // Bug patterns
            ReviewRule {
                name: "Unwrap usage".to_string(),
                pattern: Regex::new(r#"\.unwrap\(\)"#).unwrap(),
                severity: Severity::Warning,
                issue_type: IssueType::Bug,
                message: "Use of .unwrap() can cause panics".to_string(),
            },
            ReviewRule {
                name: "Expect usage".to_string(),
                pattern: Regex::new(r#"\.expect\("#).unwrap(),
                severity: Severity::Warning,
                issue_type: IssueType::Bug,
                message: "Use of .expect() will panic on error".to_string(),
            },
            // Performance patterns
            ReviewRule {
                name: "Clone in loop".to_string(),
                pattern: Regex::new(r#"for.*\.clone\(\)"#).unwrap(),
                severity: Severity::Info,
                issue_type: IssueType::PerformanceIssue,
                message: "Consider using references instead of cloning".to_string(),
            },
            // Style issues
            ReviewRule {
                name: "TODO comment".to_string(),
                pattern: Regex::new(r#"// TODO"#).unwrap(),
                severity: Severity::Info,
                issue_type: IssueType::DocumentationIssue,
                message: "TODO comment found, consider creating an issue".to_string(),
            },
            ReviewRule {
                name: "FIXME comment".to_string(),
                pattern: Regex::new(r#"// FIXME"#).unwrap(),
                severity: Severity::Warning,
                issue_type: IssueType::DocumentationIssue,
                message: "FIXME comment indicates unresolved issue".to_string(),
            },
        ];

        Self { rules }
    }

    /// Analyze git diff and generate review
    pub fn analyze_diff(&self, repo_path: &Path) -> Result<ReviewResult> {
        // Get git diff
        let diff = self.get_git_diff(repo_path)?;

        // Parse changed files
        let changed_files = self.parse_diff_files(&diff)?;

        // Analyze each file
        let mut issues = Vec::new();
        let mut suggestions = Vec::new();
        let mut lines_reviewed = 0;

        for file in &changed_files {
            let file_issues = self.analyze_file(&file.path, &file.content)?;
            lines_reviewed += file.content.lines().count();
            issues.extend(file_issues);
        }

        // Generate suggestions
        suggestions.extend(self.generate_suggestions(&issues));

        // Calculate score
        let score = self.calculate_score(&issues, lines_reviewed);

        // Generate summary
        let summary = self.generate_summary(&issues, &suggestions, &score);

        Ok(ReviewResult {
            issues,
            suggestions,
            score,
            summary,
        })
    }

    /// Analyze a given path (convenience method for async calls)
    pub async fn analyze_path(&self, path: &Path) -> Result<String> {
        let result = self.analyze_diff(path)?;
        Ok(CodeReviewAnalyzer::format_review_results(&result))
    }

    /// Format review results for display
    pub fn format_review_results(result: &ReviewResult) -> String {
        let mut output = String::new();

        output.push_str("📊 Code Review Results\n");

        // Convert ReviewScore to display value
        let score_display = result.score.grade.to_string();
        output.push_str(&format!("Score: {score_display}/10\n\n"));

        if !result.issues.is_empty() {
            output.push_str("🐛 Issues Found:\n");
            for issue in &result.issues {
                let icon = match issue.severity {
                    Severity::Critical => "🔴",
                    Severity::Error => "🟠",
                    Severity::Warning => "⚠️ ",
                    Severity::Info => "ℹ️ ",
                };
                output.push_str(&format!(
                    "{} {}:{} - {}\n",
                    icon, issue.file, issue.line, issue.message
                ));
            }
            output.push('\n');
        }

        if !result.suggestions.is_empty() {
            output.push_str("💡 Suggestions:\n");
            for suggestion in &result.suggestions {
                output.push_str(&format!(
                    "• [{}] {}\n",
                    match suggestion.category {
                        SuggestionCategory::Performance => "Performance",
                        SuggestionCategory::Readability => "Readability",
                        SuggestionCategory::Maintainability => "Maintainability",
                        SuggestionCategory::Security => "Security",
                        SuggestionCategory::BestPractice => "Best Practice",
                    },
                    suggestion.message
                ));
            }
        }

        output.push_str(&format!("\n{}\n", result.summary));
        output
    }

    /// Get git diff output
    fn get_git_diff(&self, repo_path: &Path) -> Result<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .arg("diff")
            .arg("--cached")
            .arg("--unified=5")
            .output()
            .context("Failed to run git diff")?;

        if !output.status.success() {
            anyhow::bail!(
                "git diff failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Parse diff output to extract changed files
    fn parse_diff_files(&self, diff: &str) -> Result<Vec<ChangedFile>> {
        let mut files = Vec::new();
        let mut current_file = ChangedFile {
            path: String::new(),
            content: String::new(),
        };

        for line in diff.lines() {
            if line.starts_with("+++ ") {
                // New file started
                if !current_file.path.is_empty() {
                    files.push(current_file);
                }
                let path = line.strip_prefix("+++ ").unwrap_or(line).to_string();
                current_file = ChangedFile {
                    path,
                    content: String::new(),
                };
            } else if line.starts_with('+') && !line.starts_with("+++") {
                // Added line
                current_file.content.push_str(&line[1..]);
                current_file.content.push('\n');
            }
        }

        if !current_file.path.is_empty() {
            files.push(current_file);
        }

        Ok(files)
    }

    /// Analyze a single file for issues
    fn analyze_file(&self, file_path: &str, content: &str) -> Result<Vec<Issue>> {
        let mut issues = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            for rule in &self.rules {
                if rule.pattern.is_match(line) {
                    issues.push(Issue {
                        issue_type: rule.issue_type.clone(),
                        severity: rule.severity.clone(),
                        file: file_path.to_string(),
                        line: line_num + 1,
                        message: rule.message.clone(),
                        code_snippet: Some(line.to_string()),
                    });
                }
            }
        }

        Ok(issues)
    }

    /// Generate improvement suggestions
    fn generate_suggestions(&self, issues: &[Issue]) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        // Count issues by type
        let issue_counts = self.count_issues_by_type(issues);

        // Generate suggestions based on issue patterns
        if issue_counts
            .get(&IssueType::SecurityVulnerability)
            .unwrap_or(&0)
            > &0
        {
            suggestions.push(Suggestion {
                category: SuggestionCategory::Security,
                message: "Consider using parameterized queries and input validation".to_string(),
                file: None,
                line: None,
                example: Some(
                    "Use prepared statements or ORM to prevent injection attacks".to_string(),
                ),
            });
        }

        if issue_counts.get(&IssueType::Bug).unwrap_or(&0) > &2 {
            suggestions.push(Suggestion {
                category: SuggestionCategory::BestPractice,
                message:
                    "Multiple potential panics detected. Consider using Result<T, E> propagation"
                        .to_string(),
                file: None,
                line: None,
                example: Some(
                    "Replace .unwrap() with proper error handling using ? operator".to_string(),
                ),
            });
        }

        if issue_counts.get(&IssueType::PerformanceIssue).unwrap_or(&0) > &0 {
            suggestions.push(Suggestion {
                category: SuggestionCategory::Performance,
                message: "Performance improvements available".to_string(),
                file: None,
                line: None,
                example: Some(
                    "Use references, avoid clones in loops, pre-allocate collections".to_string(),
                ),
            });
        }

        suggestions
    }

    /// Count issues by type
    fn count_issues_by_type(
        &self,
        issues: &[Issue],
    ) -> std::collections::HashMap<IssueType, usize> {
        let mut counts = std::collections::HashMap::new();
        for issue in issues {
            *counts.entry(issue.issue_type.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Calculate review score
    fn calculate_score(&self, issues: &[Issue], lines_reviewed: usize) -> ReviewScore {
        let critical_count = issues
            .iter()
            .filter(|i| i.severity == Severity::Critical)
            .count();
        let error_count = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count();
        let warning_count = issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count();

        let total_issues = issues.len();

        // Grade calculation
        let grade = if critical_count > 0 {
            'F'
        } else if error_count > 2 {
            'D'
        } else if error_count > 0 || warning_count > 5 {
            'C'
        } else if warning_count > 2 {
            'B'
        } else {
            'A'
        };

        ReviewScore {
            grade,
            issues_found: total_issues,
            suggestions_count: 0, // Will be updated later
            lines_reviewed,
        }
    }

    /// Generate review summary
    fn generate_summary(
        &self,
        issues: &[Issue],
        suggestions: &[Suggestion],
        score: &ReviewScore,
    ) -> String {
        format!(
            "Code Review Complete\n\
             ===================\n\
             Grade: {}\n\
             Issues Found: {}\n\
             Lines Reviewed: {}\n\
             Suggestions: {}\n\
             \n\
             Critical: {}\n\
             Errors: {}\n\
             Warnings: {}\n\
             Info: {}",
            score.grade,
            score.issues_found,
            score.lines_reviewed,
            suggestions.len(),
            issues
                .iter()
                .filter(|i| i.severity == Severity::Critical)
                .count(),
            issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count(),
            issues
                .iter()
                .filter(|i| i.severity == Severity::Warning)
                .count(),
            issues
                .iter()
                .filter(|i| i.severity == Severity::Info)
                .count(),
        )
    }
}

/// Review rule
#[derive(Debug, Clone)]
pub struct ReviewRule {
    pub name: String,
    pub pattern: Regex,
    pub severity: Severity,
    pub issue_type: IssueType,
    pub message: String,
}

/// Changed file from diff
#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_analyzer_creation() {
        let analyzer = CodeReviewAnalyzer::new();
        assert_eq!(analyzer.rules.len(), 6); // Should have 6 default rules
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical > Severity::Error);
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
    }

    #[test]
    fn test_score_calculation() {
        let analyzer = CodeReviewAnalyzer::new();

        // Test with no issues
        let score = analyzer.calculate_score(&[], 100);
        assert_eq!(score.grade, 'A');

        // Test with critical issue
        let issues = vec![Issue {
            issue_type: IssueType::SecurityVulnerability,
            severity: Severity::Critical,
            file: "test.rs".to_string(),
            line: 1,
            message: "Test".to_string(),
            code_snippet: None,
        }];
        let score = analyzer.calculate_score(&issues, 100);
        assert_eq!(score.grade, 'F');
    }

    // --- IssueType ---

    #[test]
    fn issue_type_variants_distinct() {
        let types = [
            IssueType::Bug,
            IssueType::SecurityVulnerability,
            IssueType::CodeSmell,
            IssueType::PerformanceIssue,
            IssueType::StyleViolation,
            IssueType::DocumentationIssue,
        ];
        for (i, a) in types.iter().enumerate() {
            for (j, b) in types.iter().enumerate() {
                assert_eq!(i == j, a == b);
            }
        }
    }

    // --- Severity ---

    #[test]
    fn severity_ordering_chain() {
        assert!(Severity::Critical > Severity::Error);
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
    }

    #[test]
    fn severity_equality() {
        assert_eq!(Severity::Info, Severity::Info);
        assert_ne!(Severity::Info, Severity::Warning);
    }

    // --- Issue ---

    #[test]
    fn issue_fields() {
        let issue = Issue {
            issue_type: IssueType::Bug,
            severity: Severity::Warning,
            file: "main.rs".into(),
            line: 42,
            message: "Use of .unwrap()".into(),
            code_snippet: Some("x.unwrap()".into()),
        };
        assert_eq!(issue.file, "main.rs");
        assert_eq!(issue.line, 42);
        assert!(issue.code_snippet.is_some());
    }

    // --- Suggestion ---

    #[test]
    fn suggestion_fields() {
        let s = Suggestion {
            category: SuggestionCategory::Performance,
            message: "Use references".into(),
            file: Some("lib.rs".into()),
            line: Some(10),
            example: Some("&x instead of x.clone()".into()),
        };
        assert_eq!(s.category, SuggestionCategory::Performance);
        assert!(s.file.is_some());
    }

    // --- ReviewScore ---

    #[test]
    fn review_score_fields() {
        let score = ReviewScore {
            grade: 'B',
            issues_found: 3,
            suggestions_count: 2,
            lines_reviewed: 100,
        };
        assert_eq!(score.grade, 'B');
        assert_eq!(score.lines_reviewed, 100);
    }

    // --- Score calculation with different severity combos ---

    #[test]
    fn score_grade_d_with_many_errors() {
        let analyzer = CodeReviewAnalyzer::new();
        let issues: Vec<Issue> = (0..3)
            .map(|i| Issue {
                issue_type: IssueType::Bug,
                severity: Severity::Error,
                file: format!("f{i}.rs"),
                line: i + 1,
                message: "err".into(),
                code_snippet: None,
            })
            .collect();
        let score = analyzer.calculate_score(&issues, 50);
        assert_eq!(score.grade, 'D');
    }

    #[test]
    fn score_grade_c_with_one_error() {
        let analyzer = CodeReviewAnalyzer::new();
        let issues = vec![Issue {
            issue_type: IssueType::Bug,
            severity: Severity::Error,
            file: "a.rs".into(),
            line: 1,
            message: "err".into(),
            code_snippet: None,
        }];
        let score = analyzer.calculate_score(&issues, 50);
        assert_eq!(score.grade, 'C');
    }

    #[test]
    fn score_grade_b_with_three_warnings() {
        let analyzer = CodeReviewAnalyzer::new();
        let issues: Vec<Issue> = (0..3)
            .map(|i| Issue {
                issue_type: IssueType::Bug,
                severity: Severity::Warning,
                file: format!("f{i}.rs"),
                line: i + 1,
                message: "warn".into(),
                code_snippet: None,
            })
            .collect();
        let score = analyzer.calculate_score(&issues, 50);
        assert_eq!(score.grade, 'B');
    }

    #[test]
    fn score_grade_c_with_six_warnings() {
        let analyzer = CodeReviewAnalyzer::new();
        let issues: Vec<Issue> = (0..6)
            .map(|i| Issue {
                issue_type: IssueType::CodeSmell,
                severity: Severity::Warning,
                file: format!("f{i}.rs"),
                line: i + 1,
                message: "warn".into(),
                code_snippet: None,
            })
            .collect();
        let score = analyzer.calculate_score(&issues, 50);
        assert_eq!(score.grade, 'C');
    }

    // --- analyze_file ---

    #[test]
    fn analyze_file_detects_unwrap() {
        let analyzer = CodeReviewAnalyzer::new();
        let issues = analyzer
            .analyze_file("test.rs", "let x = y.unwrap();\n")
            .unwrap();
        assert!(issues.iter().any(|i| i.message.contains("unwrap")));
        assert_eq!(issues[0].line, 1);
    }

    #[test]
    fn analyze_file_detects_hardcoded_password() {
        let analyzer = CodeReviewAnalyzer::new();
        let issues = analyzer
            .analyze_file("auth.rs", "let password = \"secret\";\n")
            .unwrap();
        assert!(issues
            .iter()
            .any(|i| i.issue_type == IssueType::SecurityVulnerability));
    }

    #[test]
    fn analyze_file_clean_code() {
        let analyzer = CodeReviewAnalyzer::new();
        let issues = analyzer
            .analyze_file("clean.rs", "fn main() { println!(\"hello\"); }\n")
            .unwrap();
        assert!(issues.is_empty());
    }

    // --- format_review_results ---

    #[test]
    fn format_results_with_issues() {
        let result = ReviewResult {
            issues: vec![Issue {
                issue_type: IssueType::Bug,
                severity: Severity::Warning,
                file: "a.rs".into(),
                line: 1,
                message: "unwrap used".into(),
                code_snippet: None,
            }],
            suggestions: vec![],
            score: ReviewScore {
                grade: 'B',
                issues_found: 1,
                suggestions_count: 0,
                lines_reviewed: 50,
            },
            summary: "Test summary".into(),
        };
        let output = CodeReviewAnalyzer::format_review_results(&result);
        assert!(output.contains("Code Review Results"));
        assert!(output.contains("a.rs"));
    }

    #[test]
    fn format_results_with_suggestions() {
        let result = ReviewResult {
            issues: vec![],
            suggestions: vec![Suggestion {
                category: SuggestionCategory::Security,
                message: "Validate inputs".into(),
                file: None,
                line: None,
                example: None,
            }],
            score: ReviewScore {
                grade: 'A',
                issues_found: 0,
                suggestions_count: 1,
                lines_reviewed: 100,
            },
            summary: "Clean".into(),
        };
        let output = CodeReviewAnalyzer::format_review_results(&result);
        assert!(output.contains("Suggestions"));
        assert!(output.contains("Validate inputs"));
    }

    // --- parse_diff_files ---

    #[test]
    fn parse_diff_files_basic() {
        let analyzer = CodeReviewAnalyzer::new();
        let diff = "+++ b/main.rs\n+fn main() {}\n+++ b/lib.rs\n+pub fn lib() {}\n";
        let files = analyzer.parse_diff_files(diff).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files[0].path.contains("main.rs"));
        assert!(files[1].content.contains("pub fn lib()"));
    }

    #[test]
    fn parse_diff_files_empty() {
        let analyzer = CodeReviewAnalyzer::new();
        let files = analyzer.parse_diff_files("").unwrap();
        assert!(files.is_empty());
    }
}
