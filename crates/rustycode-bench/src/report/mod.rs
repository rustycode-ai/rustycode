//! Output formatters for benchmark results and diffs.

use crate::history::RunDiff;
use crate::job::BenchmarkResults;

/// Trait for formatting benchmark output.
pub trait ReportFormatter {
    fn format_results(&self, results: &BenchmarkResults) -> String;
    fn format_diff(&self, diff: &RunDiff) -> String;
}

/// Human-readable terminal output (default).
pub struct PrettyFormatter;

impl ReportFormatter for PrettyFormatter {
    fn format_results(&self, results: &BenchmarkResults) -> String {
        let mut out = String::new();
        out.push_str("\n=== Benchmark Results ===\n");
        out.push_str(&format!("Total: {}\n", results.total));
        out.push_str(&format!("Passed: {}\n", results.passed));
        out.push_str(&format!("Failed: {}\n", results.failed));
        out.push_str(&format!("Errors: {}\n", results.errors));
        out.push_str(&format!("Pass rate: {:.1}%\n", results.accuracy * 100.0));
        out.push_str(&format!("Mean reward: {:.3}\n", results.mean_reward));

        if !results.task_results.is_empty() {
            out.push_str("\n=== Task Results ===\n");
            for tr in &results.task_results {
                let status = if tr.passed { "PASS" } else { "FAIL" };
                let error_suffix = tr
                    .error
                    .as_ref()
                    .map(|e| format!(" [{e}]"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- {}: {} (reward: {:.2}, {:.1}s){error_suffix}\n",
                    tr.task_name, status, tr.reward, tr.duration_secs
                ));
            }
        }

        out.push('\n');
        out
    }

    fn format_diff(&self, diff: &RunDiff) -> String {
        crate::history::format_diff(diff)
    }
}

/// JSON output.
pub struct JsonFormatter;

impl ReportFormatter for JsonFormatter {
    fn format_results(&self, results: &BenchmarkResults) -> String {
        serde_json::to_string_pretty(results).unwrap_or_default()
    }

    fn format_diff(&self, diff: &RunDiff) -> String {
        serde_json::to_string_pretty(diff).unwrap_or_default()
    }
}

/// CSV output for spreadsheet import.
pub struct CsvFormatter;

impl ReportFormatter for CsvFormatter {
    fn format_results(&self, results: &BenchmarkResults) -> String {
        let mut out = String::from("task_name,reward,passed,duration_secs,error\n");
        for tr in &results.task_results {
            let error = tr.error.as_deref().unwrap_or("");
            let escaped_error = if error.contains(',') || error.contains('"') {
                format!("\"{}\"", error.replace('"', "\"\""))
            } else {
                error.to_string()
            };
            out.push_str(&format!(
                "{},{},{:.2},{:.1},{}\n",
                tr.task_name, tr.reward, tr.passed, tr.duration_secs, escaped_error
            ));
        }
        out
    }

    fn format_diff(&self, diff: &RunDiff) -> String {
        let mut out = String::from("task_name,baseline_reward,comparison_reward,delta,change\n");
        for t in &diff.improved {
            out.push_str(&format!(
                "{},{:.2},{:.2},{:.2},improved\n",
                t.task_name, t.baseline_reward, t.comparison_reward, t.delta
            ));
        }
        for t in &diff.regressed {
            out.push_str(&format!(
                "{},{:.2},{:.2},{:.2},regressed\n",
                t.task_name, t.baseline_reward, t.comparison_reward, t.delta
            ));
        }
        for name in &diff.unchanged {
            out.push_str(&format!("{},,,0.00,unchanged\n", name));
        }
        out
    }
}

/// Markdown summary for PRs and commit messages.
pub struct MarkdownFormatter;

impl ReportFormatter for MarkdownFormatter {
    fn format_results(&self, results: &BenchmarkResults) -> String {
        let mut out = String::from("## Benchmark Results\n\n");
        out.push_str("| Metric | Value |\n|--------|-------|\n");
        out.push_str(&format!("| Total | {} |\n", results.total));
        out.push_str(&format!("| Passed | {} |\n", results.passed));
        out.push_str(&format!("| Failed | {} |\n", results.failed));
        out.push_str(&format!(
            "| Accuracy | {:.1}% |\n",
            results.accuracy * 100.0
        ));
        out.push_str(&format!("| Mean reward | {:.3} |\n\n", results.mean_reward));

        if !results.task_results.is_empty() {
            out.push_str("| Task | Status | Reward | Duration |\n");
            out.push_str("|------|--------|--------|----------|\n");
            for tr in &results.task_results {
                let status = if tr.passed {
                    ":white_check_mark:"
                } else {
                    ":x:"
                };
                out.push_str(&format!(
                    "| {} | {} | {:.2} | {:.1}s |\n",
                    tr.task_name, status, tr.reward, tr.duration_secs
                ));
            }
        }

        out
    }

    fn format_diff(&self, diff: &RunDiff) -> String {
        let acc_arrow = if diff.accuracy_delta > 0.0 { "+" } else { "" };
        let mut out = format!(
            "## Diff: {} vs {}\n\n",
            diff.baseline_run, diff.comparison_run
        );
        out.push_str("| Metric | Delta |\n|--------|-------|\n");
        out.push_str(&format!(
            "| Accuracy | {acc_arrow}{:.1}% |\n",
            diff.accuracy_delta * 100.0
        ));
        out.push_str(&format!(
            "| Mean reward | {acc_arrow}{:.3} |\n\n",
            diff.mean_reward_delta
        ));

        if !diff.improved.is_empty() {
            out.push_str("### Improved\n\n");
            out.push_str("| Task | Baseline | Comparison | Delta |\n");
            out.push_str("|------|----------|------------|-------|\n");
            for t in &diff.improved {
                out.push_str(&format!(
                    "| {} | {:.2} | {:.2} | +{:.2} |\n",
                    t.task_name, t.baseline_reward, t.comparison_reward, t.delta
                ));
            }
            out.push('\n');
        }

        if !diff.regressed.is_empty() {
            out.push_str("### Regressed\n\n");
            out.push_str("| Task | Baseline | Comparison | Delta |\n");
            out.push_str("|------|----------|------------|-------|\n");
            for t in &diff.regressed {
                out.push_str(&format!(
                    "| {} | {:.2} | {:.2} | {:.2} |\n",
                    t.task_name, t.baseline_reward, t.comparison_reward, t.delta
                ));
            }
            out.push('\n');
        }

        out.push_str(&format!(
            "Unchanged: {}/{} tasks\n",
            diff.unchanged.len(),
            diff.improved.len() + diff.regressed.len() + diff.unchanged.len()
        ));

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::TaskDiff;
    use crate::job::TaskResult;

    fn make_results() -> BenchmarkResults {
        BenchmarkResults {
            total: 2,
            passed: 1,
            failed: 1,
            errors: 0,
            accuracy: 0.5,
            mean_reward: 0.5,
            trials: Vec::new(),
            task_results: vec![
                TaskResult {
                    task_name: "task-a".to_string(),
                    agent_name: "oracle".to_string(),
                    reward: 1.0,
                    passed: true,
                    error: None,
                    duration_secs: 1.5,
                },
                TaskResult {
                    task_name: "task-b".to_string(),
                    agent_name: "oracle".to_string(),
                    reward: 0.0,
                    passed: false,
                    error: Some("timeout".to_string()),
                    duration_secs: 30.0,
                },
            ],
            pass_at_k: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn pretty_formatter_results() {
        let f = PrettyFormatter;
        let output = f.format_results(&make_results());
        assert!(output.contains("Benchmark Results"));
        assert!(output.contains("task-a"));
        assert!(output.contains("PASS"));
        assert!(output.contains("FAIL"));
    }

    #[test]
    fn json_formatter_results() {
        let f = JsonFormatter;
        let output = f.format_results(&make_results());
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["total"], 2);
    }

    #[test]
    fn csv_formatter_results() {
        let f = CsvFormatter;
        let output = f.format_results(&make_results());
        assert!(output.starts_with("task_name,reward"));
        assert!(output.contains("task-a"));
        assert!(output.contains("task-b"));
        // Verify no unquoted commas in error field
        assert_eq!(output.lines().count(), 3);
    }

    #[test]
    fn csv_formatter_escapes_commas() {
        let f = CsvFormatter;
        let mut results = make_results();
        results.task_results[1].error = Some("err, with comma".to_string());
        let output = f.format_results(&results);
        assert!(output.contains("\"err, with comma\""));
    }

    #[test]
    fn markdown_formatter_results() {
        let f = MarkdownFormatter;
        let output = f.format_results(&make_results());
        assert!(output.contains("## Benchmark Results"));
        assert!(output.contains("| task-a"));
    }

    #[test]
    fn markdown_formatter_diff() {
        let f = MarkdownFormatter;
        let diff = RunDiff {
            baseline_run: "run-1".to_string(),
            comparison_run: "run-2".to_string(),
            accuracy_delta: 0.2,
            mean_reward_delta: 0.1,
            improved: vec![TaskDiff {
                task_name: "task-a".to_string(),
                baseline_reward: 0.0,
                comparison_reward: 1.0,
                delta: 1.0,
            }],
            regressed: vec![],
            unchanged: vec!["task-b".to_string()],
            new_tasks: vec![],
            removed_tasks: vec![],
        };
        let output = f.format_diff(&diff);
        assert!(output.contains("## Diff"));
        assert!(output.contains("task-a"));
        assert!(output.contains("Improved"));
    }
}
