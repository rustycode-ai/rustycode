//! Report generation for load test results

use crate::error::{LoadTestError, Result};
use crate::metrics::LoadTestResults;
use serde_json;
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::path::Path;

/// Report format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReportFormat {
    /// JSON format
    Json,

    /// Terminal output
    Terminal,

    /// HTML report
    Html,

    /// Markdown report
    Markdown,
}

/// Report generator
pub struct ReportGenerator {
    /// Output format
    format: ReportFormat,

    /// Whether to include per-user metrics
    include_per_user: bool,

    /// Whether to include time series data
    include_time_series: bool,
}

impl ReportGenerator {
    /// Create a new report generator
    pub const fn new(format: ReportFormat) -> Self {
        Self {
            format,
            include_per_user: true,
            include_time_series: true,
        }
    }

    /// Set whether to include per-user metrics
    pub const fn with_per_user_metrics(mut self, include: bool) -> Self {
        self.include_per_user = include;
        self
    }

    /// Set whether to include time series data
    pub const fn with_time_series(mut self, include: bool) -> Self {
        self.include_time_series = include;
        self
    }

    /// Generate a report from results
    pub fn generate(&self, results: &LoadTestResults) -> Result<String> {
        match self.format {
            ReportFormat::Json => self.generate_json(results),
            ReportFormat::Html => Ok(Self::generate_html(results)),
            ReportFormat::Markdown => Ok(self.generate_markdown(results)),
            _ => Ok(self.generate_terminal(results)),
        }
    }

    /// Generate a JSON report
    fn generate_json(&self, results: &LoadTestResults) -> Result<String> {
        let mut report = serde_json::to_value(results)
            .map_err(|e| LoadTestError::ReportError(format!("JSON serialization failed: {e}")))?;

        // Optionally exclude detailed data
        if !self.include_per_user {
            if let Some(obj) = report.as_object_mut() {
                obj.remove("per_user_metrics");
            }
        }

        if !self.include_time_series {
            if let Some(obj) = report.as_object_mut() {
                obj.remove("time_series");
            }
        }

        serde_json::to_string_pretty(&report)
            .map_err(|e| LoadTestError::ReportError(format!("JSON formatting failed: {e}")))
    }

    /// Generate a terminal report
    #[allow(clippy::too_many_lines)]
    fn generate_terminal(&self, results: &LoadTestResults) -> String {
        // Pre-allocate output string with estimated capacity for terminal report
        let mut output = String::with_capacity(2048);

        // Header
        output.push_str("╔════════════════════════════════════════════╗\n");
        output.push_str("║       Load Test Results Report           ║\n");
        output.push_str("╚════════════════════════════════════════════╝\n");
        output.push('\n');

        // Scenario info
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!("Scenario: {}\n", results.scenario_name),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!("Duration: {:.2}s\n", results.total_duration.as_secs_f64()),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "Time: {} to {}\n",
                results.start_time.format("%Y-%m-%d %H:%M:%S"),
                results.end_time.format("%Y-%m-%d %H:%M:%S")
            ),
        );
        output.push('\n');

        // Response times
        output.push_str("┌─ Response Times ─────────────────────┐\n");
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ Min: {:>10.2?}                 │\n",
                results.response_times.min
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ Max: {:>10.2?}                 │\n",
                results.response_times.max
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ Mean: {:>10.2?}                 │\n",
                results.response_times.mean
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ Median (p50): {:>7.2?}             │\n",
                results.response_times.p50
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ p90: {:>10.2?}                 │\n",
                results.response_times.p90
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ p95: {:>10.2?}                 │\n",
                results.response_times.p95
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ p99: {:>10.2?}                 │\n",
                results.response_times.p99
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ p999: {:>9.2?}                 │\n",
                results.response_times.p999
            ),
        );
        output.push_str("└──────────────────────────────────────┘\n");
        output.push('\n');

        // Throughput
        output.push_str("┌─ Throughput ──────────────────────────┐\n");
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ Total Requests: {:>12}        │\n",
                results.throughput.total_requests
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ Successful: {:>17}        │\n",
                results.throughput.successful_requests
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ Failed: {:>20}        │\n",
                results.throughput.failed_requests
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ Error Rate: {:>18.2}%        │\n",
                results.throughput.error_rate * 100.0
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut output,
            format_args!(
                "│ Throughput: {:>16.2} req/s  │\n",
                results.throughput.throughput_per_second
            ),
        );
        output.push_str("└──────────────────────────────────────┘\n");
        output.push('\n');

        // Errors
        if results.errors.total_errors > 0 {
            output.push_str("┌─ Errors ──────────────────────────────┐\n");
            let _ = FmtWrite::write_fmt(
                &mut output,
                format_args!(
                    "│ Total Errors: {:>17}        │\n",
                    results.errors.total_errors
                ),
            );
            for (category, count) in &results.errors.by_category {
                let _ = FmtWrite::write_fmt(
                    &mut output,
                    format_args!("│   {}: {:>22}        │\n", category.name(), count),
                );
            }
            output.push_str("└──────────────────────────────────────┘\n");
            output.push('\n');
        }

        // Per-user metrics (optional)
        if self.include_per_user && !results.per_user_metrics.is_empty() {
            output.push_str("┌─ Per-User Metrics ────────────────────┐\n");
            for (user_id, metrics) in &results.per_user_metrics {
                let _ = FmtWrite::write_fmt(
                    &mut output,
                    format_args!(
                        "│ User {:>3}: {:>4} req, {:>6.2?} avg    │\n",
                        user_id, metrics.total_requests, metrics.avg_response_time
                    ),
                );
            }
            output.push_str("└──────────────────────────────────────┘\n");
            output.push('\n');
        }

        output
    }

    /// Generate an HTML report
    #[allow(clippy::too_many_lines)]
    fn generate_html(results: &LoadTestResults) -> String {
        // Pre-allocate HTML string with estimated capacity
        let mut html = String::with_capacity(4096);

        html.push_str("<!DOCTYPE html>\n");
        html.push_str("<html>\n");
        html.push_str("<head>\n");
        html.push_str("    <title>Load Test Report</title>\n");
        html.push_str("    <style>\n");
        html.push_str("        body { font-family: Arial, sans-serif; margin: 20px; }\n");
        html.push_str("        h1 { color: #333; }\n");
        html.push_str(
            "        h2 { color: #666; border-bottom: 2px solid #ddd; padding-bottom: 5px; }\n",
        );
        html.push_str("        .metric { margin: 10px 0; }\n");
        html.push_str(
            "        .metric-label { font-weight: bold; display: inline-block; width: 200px; }\n",
        );
        html.push_str("        .metric-value { color: #333; }\n");
        html.push_str("        .success { color: green; }\n");
        html.push_str("        .error { color: red; }\n");
        html.push_str(
            "        table { border-collapse: collapse; width: 100%; margin: 20px 0; }\n",
        );
        html.push_str(
            "        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }\n",
        );
        html.push_str("        th { background-color: #4CAF50; color: white; }\n");
        html.push_str("        tr:nth-child(even) { background-color: #f2f2f2; }\n");
        html.push_str("        .container { max-width: 1200px; margin: 0 auto; }\n");
        html.push_str("    </style>\n");
        html.push_str("</head>\n");
        html.push_str("<body>\n");
        html.push_str("    <div class=\"container\">\n");

        // Header
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "        <h1>Load Test Report: {}</h1>\n",
                results.scenario_name
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "        <p><strong>Duration:</strong> {:.2}s</p>\n",
                results.total_duration.as_secs_f64()
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "        <p><strong>Time:</strong> {} to {}</p>\n",
                results.start_time.format("%Y-%m-%d %H:%M:%S"),
                results.end_time.format("%Y-%m-%d %H:%M:%S")
            ),
        );

        // Summary metrics
        html.push_str("        <h2>Summary</h2>\n");
        html.push_str("        <div class=\"metric\">\n");
        html.push_str("            <span class=\"metric-label\">Total Requests:</span>\n");
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <span class=\"metric-value\">{}</span>\n",
                results.throughput.total_requests
            ),
        );
        html.push_str("        </div>\n");

        html.push_str("        <div class=\"metric\">\n");
        html.push_str("            <span class=\"metric-label\">Successful:</span>\n");
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <span class=\"metric-value success\">{}</span>\n",
                results.throughput.successful_requests
            ),
        );
        html.push_str("        </div>\n");

        html.push_str("        <div class=\"metric\">\n");
        html.push_str("            <span class=\"metric-label\">Failed:</span>\n");
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <span class=\"metric-value error\">{}</span>\n",
                results.throughput.failed_requests
            ),
        );
        html.push_str("        </div>\n");

        html.push_str("        <div class=\"metric\">\n");
        html.push_str("            <span class=\"metric-label\">Error Rate:</span>\n");
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <span class=\"metric-value\">{:.2}%</span>\n",
                results.throughput.error_rate * 100.0
            ),
        );
        html.push_str("        </div>\n");

        html.push_str("        <div class=\"metric\">\n");
        html.push_str("            <span class=\"metric-label\">Throughput:</span>\n");
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <span class=\"metric-value\">{:.2} req/s</span>\n",
                results.throughput.throughput_per_second
            ),
        );
        html.push_str("        </div>\n");

        // Response times
        html.push_str("        <h2>Response Times</h2>\n");
        html.push_str("        <table>\n");
        html.push_str("            <tr><th>Metric</th><th>Value</th></tr>\n");
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <tr><td>Min</td><td>{:.2?}</td></tr>\n",
                results.response_times.min
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <tr><td>Max</td><td>{:.2?}</td></tr>\n",
                results.response_times.max
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <tr><td>Mean</td><td>{:.2?}</td></tr>\n",
                results.response_times.mean
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <tr><td>Median (p50)</td><td>{:.2?}</td></tr>\n",
                results.response_times.p50
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <tr><td>p90</td><td>{:.2?}</td></tr>\n",
                results.response_times.p90
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <tr><td>p95</td><td>{:.2?}</td></tr>\n",
                results.response_times.p95
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut html,
            format_args!(
                "            <tr><td>p99</td><td>{:.2?}</td></tr>\n",
                results.response_times.p99
            ),
        );
        html.push_str("        </table>\n");

        // Errors
        if results.errors.total_errors > 0 {
            html.push_str("        <h2>Errors</h2>\n");
            html.push_str("        <table>\n");
            html.push_str("            <tr><th>Category</th><th>Count</th></tr>\n");
            for (category, count) in &results.errors.by_category {
                let _ = FmtWrite::write_fmt(
                    &mut html,
                    format_args!(
                        "            <tr><td>{}</td><td>{}</td></tr>\n",
                        category.name(),
                        count
                    ),
                );
            }
            html.push_str("        </table>\n");
        }

        html.push_str("    </div>\n");
        html.push_str("</body>\n");
        html.push_str("</html>\n");

        html
    }

    /// Generate a Markdown report
    #[allow(clippy::too_many_lines)]
    fn generate_markdown(&self, results: &LoadTestResults) -> String {
        // Pre-allocate markdown string with estimated capacity
        let mut md = String::with_capacity(2048);

        // Header
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!("# Load Test Report: {}\n\n", results.scenario_name),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!(
                "**Duration:** {:.2}s  \n",
                results.total_duration.as_secs_f64()
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!(
                "**Time:** {} to {}  \n\n",
                results.start_time.format("%Y-%m-%d %H:%M:%S"),
                results.end_time.format("%Y-%m-%d %H:%M:%S")
            ),
        );

        // Summary
        md.push_str("## Summary\n\n");
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!(
                "- **Total Requests:** {}\n",
                results.throughput.total_requests
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!(
                "- **Successful:** {}\n",
                results.throughput.successful_requests
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!("- **Failed:** {}\n", results.throughput.failed_requests),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!(
                "- **Error Rate:** {:.2}%\n",
                results.throughput.error_rate * 100.0
            ),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!(
                "- **Throughput:** {:.2} req/s\n\n",
                results.throughput.throughput_per_second
            ),
        );

        // Response times
        md.push_str("## Response Times\n\n");
        md.push_str("| Metric | Value |\n");
        md.push_str("|--------|-------|\n");
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!("| Min | {:.2?} |\n", results.response_times.min),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!("| Max | {:.2?} |\n", results.response_times.max),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!("| Mean | {:.2?} |\n", results.response_times.mean),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!("| Median (p50) | {:.2?} |\n", results.response_times.p50),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!("| p90 | {:.2?} |\n", results.response_times.p90),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!("| p95 | {:.2?} |\n", results.response_times.p95),
        );
        let _ = FmtWrite::write_fmt(
            &mut md,
            format_args!("| p99 | {:.2?} |\n", results.response_times.p99),
        );
        md.push('\n');

        // Errors
        if results.errors.total_errors > 0 {
            md.push_str("## Errors\n\n");
            md.push_str("| Category | Count |\n");
            md.push_str("|----------|-------|\n");
            for (category, count) in &results.errors.by_category {
                let _ = FmtWrite::write_fmt(
                    &mut md,
                    format_args!("| {} | {} |\n", category.name(), count),
                );
            }
            md.push('\n');
        }

        // Per-user metrics
        if self.include_per_user && !results.per_user_metrics.is_empty() {
            md.push_str("## Per-User Metrics\n\n");
            md.push_str("| User | Requests | Success | Failed | Avg Response |\n");
            md.push_str("|------|----------|---------|--------|--------------|\n");
            for (user_id, metrics) in &results.per_user_metrics {
                let _ = FmtWrite::write_fmt(
                    &mut md,
                    format_args!(
                        "| {} | {} | {} | {} | {:.2?} |\n",
                        user_id,
                        metrics.total_requests,
                        metrics.successful_requests,
                        metrics.failed_requests,
                        metrics.avg_response_time
                    ),
                );
            }
            md.push('\n');
        }

        md
    }

    /// Save report to file
    pub fn save_to_file(&self, results: &LoadTestResults, path: impl AsRef<Path>) -> Result<()> {
        use std::io::Write as IoWrite;
        let report = self.generate(results)?;
        let mut file = File::create(path)
            .map_err(|e| LoadTestError::ReportError(format!("Failed to create file: {e}")))?;
        file.write_all(report.as_bytes())
            .map_err(|e| LoadTestError::ReportError(format!("Failed to write file: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_report_generator_creation() {
        let gen = ReportGenerator::new(ReportFormat::Terminal);
        assert_eq!(gen.format, ReportFormat::Terminal);
        assert!(gen.include_per_user);
        assert!(gen.include_time_series);
    }

    #[test]
    fn test_report_generator_configuration() {
        let gen = ReportGenerator::new(ReportFormat::Json)
            .with_per_user_metrics(false)
            .with_time_series(false);

        assert_eq!(gen.format, ReportFormat::Json);
        assert!(!gen.include_per_user);
        assert!(!gen.include_time_series);
    }

    #[test]
    fn test_terminal_report() {
        let mut results = LoadTestResults::new("Test Scenario".to_string());

        // Add some sample data
        for i in 0..10 {
            let result = crate::request::LoadResult::success(Duration::from_millis(100 + i * 10))
                .with_user_id((i % 3) as usize);
            results.add_result(&result);
        }

        results.end_time = results.start_time + chrono::Duration::seconds(10);
        results.total_duration = Duration::from_secs(10);
        results.finalize();

        let gen = ReportGenerator::new(ReportFormat::Terminal);
        let report = gen.generate(&results).unwrap();

        assert!(report.contains("Load Test Results Report"));
        assert!(report.contains("Test Scenario"));
        assert!(report.contains("Response Times"));
        assert!(report.contains("Throughput"));
    }

    #[test]
    fn test_json_report() {
        let mut results = LoadTestResults::new("Test Scenario".to_string());
        results.end_time = results.start_time + chrono::Duration::seconds(10);
        results.total_duration = Duration::from_secs(10);
        results.finalize();

        let reporter = ReportGenerator::new(ReportFormat::Json);
        let report = reporter.generate(&results).unwrap();

        // Should be valid JSON
        assert!(serde_json::from_str::<serde_json::Value>(&report).is_ok());
        assert!(report.contains("scenario_name"));
        assert!(report.contains("Test Scenario"));
    }

    #[test]
    fn test_html_report() {
        let mut results = LoadTestResults::new("Test Scenario".to_string());
        results.end_time = results.start_time + chrono::Duration::seconds(10);
        results.total_duration = Duration::from_secs(10);
        results.finalize();

        let reporter = ReportGenerator::new(ReportFormat::Html);
        let report = reporter.generate(&results).unwrap();

        assert!(report.contains("<!DOCTYPE html>"));
        assert!(report.contains("<title>Load Test Report</title>"));
        assert!(report.contains("Test Scenario"));
    }

    #[test]
    fn test_markdown_report() {
        let mut results = LoadTestResults::new("Test Scenario".to_string());
        results.end_time = results.start_time + chrono::Duration::seconds(10);
        results.total_duration = Duration::from_secs(10);
        results.finalize();

        let reporter = ReportGenerator::new(ReportFormat::Markdown);
        let report = reporter.generate(&results).unwrap();

        assert!(report.contains("# Load Test Report"));
        assert!(report.contains("Test Scenario"));
        assert!(report.contains("## Summary"));
    }

    #[test]
    fn test_json_report_excludes_per_user() {
        let mut results = LoadTestResults::new("Test".to_string());
        results.add_result(
            &crate::request::LoadResult::success(Duration::from_millis(10)).with_user_id(1),
        );
        results.end_time = results.start_time + chrono::Duration::seconds(5);
        results.total_duration = Duration::from_secs(5);
        results.finalize();

        let gen = ReportGenerator::new(ReportFormat::Json).with_per_user_metrics(false);
        let report = gen.generate(&results).unwrap();
        assert!(!report.contains("per_user_metrics"));
    }

    #[test]
    fn test_json_report_excludes_time_series() {
        let mut results = LoadTestResults::new("Test".to_string());
        results.end_time = results.start_time + chrono::Duration::seconds(5);
        results.total_duration = Duration::from_secs(5);
        results.finalize();

        let gen = ReportGenerator::new(ReportFormat::Json).with_time_series(false);
        let report = gen.generate(&results).unwrap();
        assert!(!report.contains("time_series"));
    }

    #[test]
    fn test_terminal_report_with_errors() {
        let mut results = LoadTestResults::new("Error Test".to_string());
        results.add_result(&crate::request::LoadResult::error(
            Duration::from_millis(5),
            "Connection refused".to_string(),
        ));
        results.end_time = results.start_time + chrono::Duration::seconds(5);
        results.total_duration = Duration::from_secs(5);
        results.finalize();

        let gen = ReportGenerator::new(ReportFormat::Terminal);
        let report = gen.generate(&results).unwrap();
        assert!(report.contains("Errors"));
    }

    #[test]
    fn test_html_report_contains_all_sections() {
        let mut results = LoadTestResults::new("HTML Test".to_string());
        results.add_result(&crate::request::LoadResult::success(Duration::from_millis(
            100,
        )));
        results.end_time = results.start_time + chrono::Duration::seconds(5);
        results.total_duration = Duration::from_secs(5);
        results.finalize();

        let gen = ReportGenerator::new(ReportFormat::Html);
        let report = gen.generate(&results).unwrap();
        assert!(report.contains("Response Times"));
        assert!(report.contains("Summary"));
        assert!(report.contains("</html>"));
    }

    #[test]
    fn test_markdown_report_with_per_user() {
        let mut results = LoadTestResults::new("Per User MD".to_string());
        results.add_result(
            &crate::request::LoadResult::success(Duration::from_millis(10)).with_user_id(1),
        );
        results.add_result(
            &crate::request::LoadResult::success(Duration::from_millis(20)).with_user_id(2),
        );
        results.end_time = results.start_time + chrono::Duration::seconds(5);
        results.total_duration = Duration::from_secs(5);
        results.finalize();

        let gen = ReportGenerator::new(ReportFormat::Markdown).with_per_user_metrics(true);
        let report = gen.generate(&results).unwrap();
        assert!(report.contains("Per-User Metrics"));
    }

    #[test]
    fn test_markdown_report_with_errors() {
        let mut results = LoadTestResults::new("Errors MD".to_string());
        results.add_result(&crate::request::LoadResult::error(
            Duration::from_millis(5),
            "HTTP 500 error".to_string(),
        ));
        results.end_time = results.start_time + chrono::Duration::seconds(5);
        results.total_duration = Duration::from_secs(5);
        results.finalize();

        let gen = ReportGenerator::new(ReportFormat::Markdown);
        let report = gen.generate(&results).unwrap();
        assert!(report.contains("Errors"));
    }

    #[test]
    fn test_report_format_variants() {
        assert_eq!(ReportFormat::Json, ReportFormat::Json);
        assert_ne!(ReportFormat::Json, ReportFormat::Terminal);
    }

    #[test]
    fn test_save_to_file_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.json");

        let mut results = LoadTestResults::new("File Test".to_string());
        results.end_time = results.start_time + chrono::Duration::seconds(5);
        results.total_duration = Duration::from_secs(5);
        results.finalize();

        let gen = ReportGenerator::new(ReportFormat::Json);
        gen.save_to_file(&results, &path).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("File Test"));
    }
}
