//! Testing tools for `RustyCode`
//!
//! Provides tools for running and managing Rust tests:
//! - `RunTestsTool`: Execute cargo test with optional filter
//! - `RunTestTool`: Run a specific test
//! - `RunBenchTool`: Execute cargo bench with optional filter
//! - `CoverageTool`: Generate test coverage report
//!
//! # Features
//!
//! - Parse cargo test output for pass/fail counts
//! - Display compilation errors clearly
//! - Support workspace projects
//! - Show test execution time

use crate::{Checkpoint, Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

// ============================================================================
// TEST RESULT TYPES
// ============================================================================

/// Result of running tests
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Total tests run
    pub total: usize,
    /// Passed tests
    pub passed: usize,
    /// Failed tests
    pub failed: usize,
    /// Ignored tests
    pub ignored: usize,
    /// Measurement benchmarks run
    pub measured: usize,
    /// Individual test results
    pub test_results: Vec<IndividualTestResult>,
    /// Compilation errors if any
    pub compilation_errors: Vec<String>,
    /// Execution time in milliseconds
    pub duration_ms: u64,
    /// Whether the build succeeded
    pub build_success: bool,
}

/// Individual test result
#[derive(Debug, Clone)]
pub struct IndividualTestResult {
    /// Test name
    pub name: String,
    /// Whether the test passed
    pub passed: bool,
    /// Test output/error message
    pub message: Option<String>,
}

/// Test run configuration
#[derive(Debug, Clone, Default)]
pub struct TestConfig {
    /// Test package filter (e.g., "`my_crate`")
    pub package: Option<String>,
    /// Test name filter
    pub filter: Option<String>,
    /// Run with --ignored flag
    pub include_ignored: bool,
    /// Show output (stdout/stderr)
    pub show_output: bool,
    /// Number of threads (0 = default)
    pub threads: Option<usize>,
    /// Whether to run in release mode
    pub release_mode: bool,
}

// ============================================================================
// TEST EXECUTION
// ============================================================================

/// Run cargo test and parse the output
pub fn run_cargo_test(
    cwd: &PathBuf,
    config: &TestConfig,
    _checkpoint: &dyn Checkpoint,
) -> Result<TestResult> {
    let start = Instant::now();

    // Build cargo test command
    let mut cmd = Command::new("cargo");

    if config.release_mode {
        cmd.arg("test");
        cmd.arg("--release");
    } else {
        cmd.arg("test");
    }

    // Add package filter if specified
    if let Some(package) = &config.package {
        cmd.arg("-p").arg(package);
    }

    // Add test filter if specified
    if let Some(filter) = &config.filter {
        cmd.arg(filter);
    }

    if config.include_ignored {
        cmd.arg("--ignored");
    }

    if config.show_output {
        cmd.arg("--");
        cmd.arg("--show-output");
    }

    // Set thread count if specified
    if let Some(threads) = config.threads {
        if threads == 1 {
            cmd.arg("--test-threads=1");
        } else {
            cmd.env("RUST_TEST_THREADS", threads.to_string());
        }
    }

    // Capture output
    let output = cmd
        .current_dir(cwd)
        .output()
        .map_err(|e| anyhow!("Failed to execute cargo test: {e}"))?;

    let duration = start.elapsed();

    // Check if build succeeded
    let build_success = output.status.success();

    // Parse the output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check for compilation errors
    let compilation_errors = if !build_success {
        parse_compilation_errors(&stdout, &stderr)
    } else {
        Vec::new()
    };

    // Parse test results
    let test_results = if build_success {
        parse_test_results(&stdout)?
    } else {
        Vec::new()
    };

    // Count totals
    let total = test_results.len();
    let passed = test_results.iter().filter(|r| r.passed).count();
    let failed = test_results.iter().filter(|r| !r.passed).count();
    let ignored = 0; // Cargo test doesn't separately report ignored count in summary

    Ok(TestResult {
        total,
        passed,
        failed,
        ignored,
        measured: 0,
        test_results,
        compilation_errors,
        duration_ms: duration.as_millis() as u64,
        build_success,
    })
}

/// Run cargo bench and parse the output
pub fn run_cargo_bench(
    cwd: &PathBuf,
    filter: Option<&str>,
    _checkpoint: &dyn Checkpoint,
) -> Result<TestResult> {
    let start = Instant::now();

    // Build cargo bench command
    let mut cmd = Command::new("cargo");
    cmd.arg("bench");

    if let Some(filter) = filter {
        cmd.arg(filter);
    }

    // Capture output
    let output = cmd
        .current_dir(cwd)
        .output()
        .map_err(|e| anyhow!("Failed to execute cargo bench: {e}"))?;

    let duration = start.elapsed();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check for compilation errors
    let compilation_errors = if !output.status.success() {
        parse_compilation_errors(&stdout, &stderr)
    } else {
        Vec::new()
    };

    // Parse benchmark results
    let test_results = if output.status.success() {
        parse_test_results(&stdout)?
    } else {
        Vec::new()
    };

    let total = test_results.len();
    let passed = total; // Benchmarks don't "fail" the same way
    let failed = 0;
    let ignored = 0;

    Ok(TestResult {
        total,
        passed,
        failed,
        ignored,
        measured: total,
        test_results,
        compilation_errors,
        duration_ms: duration.as_millis() as u64,
        build_success: output.status.success(),
    })
}

/// Generate test coverage report using cargo tarpaulin
pub fn generate_coverage(
    cwd: &PathBuf,
    _config: &TestConfig,
    _checkpoint: &dyn Checkpoint,
) -> Result<String> {
    let start = Instant::now();

    // Check if tarpaulin is available
    let check_cmd = Command::new("cargo")
        .args(["tarpaulin", "--version"])
        .current_dir(cwd)
        .output();

    let check_output = match check_cmd {
        Ok(output) => output,
        Err(_) => {
            return Ok(
                "Coverage generation requires cargo-tarpaulin. Install with:\n\
                cargo install cargo-tarpaulin\n\n\
                Then run: cargo tarpaulin --out Html"
                    .to_string(),
            );
        }
    };

    if !check_output.status.success() {
        return Ok(
            "Coverage generation requires cargo-tarpaulin. Install with:\n\
                cargo install cargo-tarpaulin\n\n\
                Then run: cargo tarpaulin --out Html"
                .to_string(),
        );
    }

    // Run tarpaulin
    let output = Command::new("cargo")
        .args(["tarpaulin", "--out", "Html"])
        .current_dir(cwd)
        .output()
        .map_err(|e| anyhow!("Failed to run cargo tarpaulin: {e}"))?;

    let duration = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut result = "**Coverage Report**\n\n".to_string();
    result.push_str(&format!("Generated in {}ms\n\n", duration.as_millis()));

    // Parse tarpqlin output for coverage percentage
    if let Some(line) = stdout.lines().find(|l| l.contains("%")) {
        result.push_str(&format!("{line}\n"));
    }

    result.push_str("\n**Full output:**\n\n");
    result.push_str(&stdout);

    if !stderr.is_empty() {
        result.push_str("\n**Errors:**\n\n");
        result.push_str(&stderr);
    }

    Ok(result)
}

/// Parse compilation errors from cargo output
fn parse_compilation_errors(stdout: &str, stderr: &str) -> Vec<String> {
    let mut errors = Vec::new();

    // Common error patterns in cargo output
    let error_patterns = ["error:", "error[E", "could not compile"];

    for line in stdout.lines().chain(stderr.lines()) {
        for pattern in &error_patterns {
            if line.contains(pattern) {
                errors.push(line.trim().to_string());
                break;
            }
        }
    }

    errors
}

/// Parse test results from cargo test output
fn parse_test_results(output: &str) -> Result<Vec<IndividualTestResult>> {
    let mut results = Vec::new();

    for line in output.lines() {
        // Look for "test result: test_name ... ok" pattern
        if line.contains("test ") && line.contains("...") {
            // Parse test name and result
            let parts: Vec<&str> = line.split_whitespace().collect();

            if parts.len() >= 3 {
                let test_name = parts[1].to_string();
                let passed = line.contains(" ok");

                results.push(IndividualTestResult {
                    name: test_name,
                    passed,
                    message: None,
                });
            }
        }
    }

    // If we didn't find structured output, try summary parsing
    if results.is_empty() {
        // Look for summary line like "test result: ok. X passed; Y failed"
        for line in output.lines() {
            if line.contains("test result:") {
                // Check if this is a "0 tests" case - don't create a fake result
                if line.contains("0 passed") && line.contains("0 failed") {
                    // No tests were run, return empty results
                    continue;
                }
                let passed = line.contains(" ok.");
                results.push(IndividualTestResult {
                    name: "all".to_string(),
                    passed,
                    message: Some(line.trim().to_string()),
                });
            }
        }
    }

    Ok(results)
}

// ============================================================================
// TOOL IMPLEMENTATIONS
// ============================================================================

/// Tool for running cargo test
pub struct RunTestsTool;

impl Tool for RunTestsTool {
    fn name(&self) -> &'static str {
        "run_tests"
    }

    fn description(&self) -> &'static str {
        "Run Rust tests using cargo test. Supports test filtering, workspace packages, \
        release mode, and parallel execution. Returns test results with pass/fail counts \
        and detailed output for failed tests."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "package": {
                    "type": "string",
                    "description": "Specific package to test (default: all)"
                },
                "filter": {
                    "type": "string",
                    "description": "Test name filter (e.g., 'test::*')"
                },
                "include_ignored": {
                    "type": "boolean",
                    "description": "Include ignored tests",
                    "default": false
                },
                "show_output": {
                    "type": "boolean",
                    "description": "Show test output",
                    "default": false
                },
                "threads": {
                    "type": "integer",
                    "description": "Number of test threads (default: auto)"
                },
                "release_mode": {
                    "type": "boolean",
                    "description": "Run in release mode",
                    "default": false
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Build config
        let package = params.get("package").and_then(|v| v.as_str());
        let filter = params.get("filter").and_then(|v| v.as_str());
        let include_ignored = params
            .get("include_ignored")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let show_output = params
            .get("show_output")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let threads = params
            .get("threads")
            .and_then(Value::as_u64)
            .map(|v| v as usize);
        let release_mode = params
            .get("release_mode")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let config = TestConfig {
            package: package.map(ToString::to_string),
            filter: filter.map(ToString::to_string),
            include_ignored,
            show_output,
            threads,
            release_mode,
        };

        // Run tests
        let result = run_cargo_test(&ctx.cwd, &config, ctx)?;

        // Format output
        let mut output = String::new();

        if !result.build_success {
            output.push_str("**Build Failed**\n\n");
            for error in &result.compilation_errors {
                output.push_str(&format!("{error}\n"));
            }
            if result.compilation_errors.is_empty() {
                output.push_str("(No error details available)\n");
            }
        } else {
            output.push_str("**Build Successful**\n\n");
        }

        if result.build_success {
            output.push_str("**Test Results:**\n\n");
            output.push_str(&format!("**Total:** {} tests\n", result.total));
            output.push_str(&format!("**Passed:** {} tests\n", result.passed));
            output.push_str(&format!("**Failed:** {} tests\n", result.failed));
            if result.ignored > 0 {
                output.push_str(&format!("**Ignored:** {} tests\n", result.ignored));
            }
            output.push_str(&format!("**Time:** {}ms\n\n", result.duration_ms));

            // Show failed tests
            if result.failed > 0 {
                output.push_str("**Failed Tests:**\n\n");
                for test_result in &result.test_results {
                    if !test_result.passed {
                        output.push_str(&format!("✗ {}\n", test_result.name));
                        if let Some(msg) = &test_result.message {
                            output.push_str(&format!("  {msg}\n"));
                        }
                    }
                }
            }
        }

        let metadata = json!({
            "build_success": result.build_success,
            "total_tests": result.total,
            "passed": result.passed,
            "failed": result.failed,
            "ignored": result.ignored,
            "duration_ms": result.duration_ms,
            "package": package,
            "filter": filter,
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

/// Tool for running a specific test
pub struct RunTestTool;

impl Tool for RunTestTool {
    fn name(&self) -> &'static str {
        "run_test"
    }

    fn description(&self) -> &'static str {
        "Run a specific Rust test by exact name. Uses cargo test with the test name \
        as filter. Shows detailed output including test assertions and panic messages."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["test_name"],
            "properties": {
                "test_name": {
                    "type": "string",
                    "description": "Exact name of the test to run"
                },
                "package": {
                    "type": "string",
                    "description": "Package containing the test"
                },
                "show_output": {
                    "type": "boolean",
                    "description": "Show full test output",
                    "default": true
                },
                "release_mode": {
                    "type": "boolean",
                    "description": "Run in release mode",
                    "default": false
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let test_name = params
            .get("test_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing 'test_name' parameter"))?;

        let package = params.get("package").and_then(|v| v.as_str());
        let show_output = params
            .get("show_output")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let release_mode = params
            .get("release_mode")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Build config for single test
        let config = TestConfig {
            package: package.map(ToString::to_string),
            filter: Some(test_name.to_string()),
            include_ignored: false,
            show_output,
            threads: Some(1),
            release_mode,
        };

        let result = run_cargo_test(&ctx.cwd, &config, ctx)?;

        let mut output = String::new();

        output.push_str(&format!("**Running test:** {test_name}\n\n"));

        if !result.build_success {
            output.push_str("**Build Failed**\n\n");
            for error in &result.compilation_errors {
                output.push_str(&format!("{error}\n"));
            }
        } else {
            output.push_str(&format!("**Completed in:** {}ms\n\n", result.duration_ms));

            // Show test results
            for test_result in &result.test_results {
                if test_result.passed {
                    output.push_str(&format!("✓ **{}**\n", test_result.name));
                } else {
                    output.push_str(&format!("✗ **{}**\n", test_result.name));
                    if let Some(msg) = &test_result.message {
                        output.push_str(&format!("  {msg}\n"));
                    }
                }
            }

            // If no specific test results found, show raw output
            if result.test_results.is_empty() && result.compilation_errors.is_empty() {
                output.push_str(
                    "\n*(Test completed but result parsing failed - check output above)*\n",
                );
            }
        }

        let metadata = json!({
            "test_name": test_name,
            "build_success": result.build_success,
            "duration_ms": result.duration_ms,
            "package": package,
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

/// Tool for running benchmarks
pub struct RunBenchTool;

impl Tool for RunBenchTool {
    fn name(&self) -> &'static str {
        "run_bench"
    }

    fn description(&self) -> &'static str {
        "Run Rust benchmarks using cargo bench. Supports test filtering and workspace packages. \
        Returns benchmark results with timing information."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "filter": {
                    "type": "string",
                    "description": "Benchmark name filter"
                },
                "package": {
                    "type": "string",
                    "description": "Package containing the benchmarks"
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let filter = params.get("filter").and_then(|v| v.as_str());
        let package = params.get("package").and_then(|v| v.as_str());

        let result = run_cargo_bench(&ctx.cwd, filter, ctx)?;

        let mut output = String::new();

        output.push_str("**Benchmark Results**\n\n");

        if !result.build_success {
            output.push_str("**Build Failed**\n\n");
            for error in &result.compilation_errors {
                output.push_str(&format!("{error}\n"));
            }
        } else {
            output.push_str(&format!("**Completed:** {} benchmarks\n", result.measured));
            output.push_str(&format!("**Time:** {}ms\n", result.duration_ms));

            // Show individual results
            if !result.test_results.is_empty() {
                output.push_str("\n**Results:**\n\n");
                for test_result in &result.test_results {
                    output.push_str(&format!("✓ **{}**\n", test_result.name));
                }
            }
        }

        let metadata = json!({
            "build_success": result.build_success,
            "benchmarks_run": result.measured,
            "duration_ms": result.duration_ms,
            "filter": filter,
            "package": package,
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

/// Tool for generating test coverage report
pub struct CoverageTool;

impl Tool for CoverageTool {
    fn name(&self) -> &'static str {
        "test_coverage"
    }

    fn description(&self) -> &'static str {
        "Generate test coverage report using cargo-tarpaulin. Requires cargo-tarpaulin to be installed. \
        Generates HTML coverage report in target/tarpaulin/ directory."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "package": {
                    "type": "string",
                    "description": "Specific package to analyze coverage for"
                },
                "filter": {
                    "type": "string",
                    "description": "Test filter to run"
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let _package = params.get("package").and_then(|v| v.as_str());
        let _filter = params.get("filter").and_then(|v| v.as_str());

        let output = generate_coverage(&ctx.cwd, &TestConfig::default(), ctx)?;

        let metadata = json!({
            "generated": true,
            "tool": "cargo-tarpaulin",
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_config_default() {
        let config = TestConfig::default();
        assert!(config.package.is_none());
        assert!(config.filter.is_none());
        assert!(!config.include_ignored);
        assert!(!config.show_output);
        assert!(config.threads.is_none());
        assert!(!config.release_mode);
    }

    #[test]
    fn test_test_config_with_values() {
        let config = TestConfig {
            package: Some("my_crate".to_string()),
            filter: Some("test::*".to_string()),
            include_ignored: true,
            show_output: true,
            threads: Some(4),
            release_mode: true,
        };

        assert_eq!(config.package, Some("my_crate".to_string()));
        assert_eq!(config.filter, Some("test::*".to_string()));
        assert!(config.include_ignored);
        assert!(config.show_output);
        assert_eq!(config.threads, Some(4));
        assert!(config.release_mode);
    }

    #[test]
    fn test_parse_compilation_errors() {
        let output = "error[E0308]: expected identifier, found keyword\n\
                      error: aborting due to previous error\n\
                      For more information, try --help\n";

        let errors = parse_compilation_errors(output, "");
        assert!(!errors.is_empty());
        assert!(errors[0].contains("error[E0308]"));
    }

    #[test]
    fn test_parse_test_results_empty() {
        let output = "running 0 tests\n\n\
                      test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured";

        let results = parse_test_results(output).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_individual_test_result() {
        let result = IndividualTestResult {
            name: "tests::test_success".to_string(),
            passed: true,
            message: None,
        };

        assert_eq!(result.name, "tests::test_success");
        assert!(result.passed);
        assert!(result.message.is_none());
    }
}
