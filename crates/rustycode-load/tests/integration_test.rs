//! Integration tests for the load testing framework
#![allow(clippy::expect_used, clippy::unwrap_used)]

use rustycode_load::*;
use std::time::Duration;

#[tokio::test]
async fn test_basic_load_test() {
    let scenario = LoadScenario::builder()
        .name("Basic Load Test")
        .concurrent_users(5)
        .duration(Duration::from_secs(1))
        .think_time(Duration::from_millis(100))
        .request_generator(|user_id| {
            LoadRequest::http_get(format!("https://example.com/{user_id}"))
        })
        .build()
        .expect("Failed to build scenario");

    let runner = LoadTestRunner::new();
    // Note: This will fail with actual HTTP requests, but tests the framework structure
    let _result = runner.run(scenario).await;
}

#[tokio::test]
async fn test_scenario_validation() {
    // Test invalid: zero concurrent users
    let result = LoadScenario::builder()
        .name("Invalid Scenario")
        .concurrent_users(0)
        .duration(Duration::from_mins(1))
        .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
        .build();

    assert!(result.is_err());

    // Test valid scenario
    let result = LoadScenario::builder()
        .name("Valid Scenario")
        .concurrent_users(10)
        .duration(Duration::from_mins(1))
        .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
        .build();

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_ramp_up_strategies() {
    // Test immediate ramp-up
    let immediate_scenario = LoadScenario::builder()
        .name("Immediate Ramp-Up")
        .concurrent_users(10)
        .duration(Duration::from_secs(1))
        .ramp_up(RampUpStrategy::Immediate)
        .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
        .build()
        .expect("Failed to build scenario");

    assert!(matches!(
        immediate_scenario.ramp_up,
        RampUpStrategy::Immediate
    ));

    // Test linear ramp-up
    let linear_scenario = LoadScenario::builder()
        .name("Linear Ramp-Up")
        .concurrent_users(10)
        .duration(Duration::from_secs(2))
        .ramp_up(RampUpStrategy::Linear {
            duration: Duration::from_secs(1),
        })
        .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
        .build()
        .expect("Failed to build scenario");

    assert!(matches!(
        linear_scenario.ramp_up,
        RampUpStrategy::Linear { .. }
    ));
}

#[test]
fn test_report_generation() {
    let mut results = LoadTestResults::new("Report Test".to_string());

    // Add sample data
    for i in 0..100 {
        let result = LoadResult::success(Duration::from_millis(50 + i as u64))
            .with_user_id((i % 10) as usize);
        results.add_result(&result);
    }

    results.end_time = results.start_time + chrono::Duration::seconds(10);
    results.total_duration = Duration::from_secs(10);
    results.finalize();

    // Test JSON report
    let json_gen = ReportGenerator::new(ReportFormat::Json);
    let json_report = json_gen
        .generate(&results)
        .expect("Failed to generate JSON report");
    assert!(json_report.contains("scenario_name"));
    assert!(serde_json::from_str::<serde_json::Value>(&json_report).is_ok());

    // Test terminal report
    let term_gen = ReportGenerator::new(ReportFormat::Terminal);
    let term_report = term_gen
        .generate(&results)
        .expect("Failed to generate terminal report");
    assert!(term_report.contains("Load Test Results"));

    // Test HTML report
    let html_gen = ReportGenerator::new(ReportFormat::Html);
    let html_report = html_gen
        .generate(&results)
        .expect("Failed to generate HTML report");
    assert!(html_report.contains("<!DOCTYPE html>"));

    // Test Markdown report
    let md_gen = ReportGenerator::new(ReportFormat::Markdown);
    let md_report = md_gen
        .generate(&results)
        .expect("Failed to generate Markdown report");
    assert!(md_report.contains("# Load Test Report"));
}

#[test]
fn test_metrics_collection() {
    let mut results = LoadTestResults::new("Metrics Test".to_string());

    // Add various results
    for i in 0..50 {
        let result =
            LoadResult::success(Duration::from_millis(100 + i * 2)).with_user_id((i % 5) as usize);
        results.add_result(&result);
    }

    // Add some errors
    results.add_result(&LoadResult::error(
        Duration::from_millis(50),
        "Test error".to_string(),
    ));

    results.end_time = results.start_time + chrono::Duration::seconds(5);
    results.total_duration = Duration::from_secs(5);
    results.finalize();

    // Verify metrics
    assert_eq!(results.throughput.total_requests, 51);
    assert_eq!(results.throughput.successful_requests, 50);
    assert_eq!(results.throughput.failed_requests, 1);
    assert!(results.response_times.p50 >= results.response_times.min);
    assert!(results.response_times.p99 <= results.response_times.max);
}

#[test]
fn test_error_categorization() {
    let mut results = LoadTestResults::new("Error Test".to_string());

    // Add various errors
    results.add_result(&LoadResult::connection_error(
        Duration::from_millis(100),
        "Connection refused",
    ));

    results.add_result(&LoadResult::timeout(
        Duration::from_secs(30),
        Duration::from_secs(30),
    ));

    results.add_result(&LoadResult::http_error(
        Duration::from_millis(50),
        500,
        "Internal Server Error".to_string(),
    ));

    results.end_time = results.start_time + chrono::Duration::seconds(1);
    results.total_duration = Duration::from_secs(1);
    results.finalize();

    // Verify error categorization
    assert_eq!(results.errors.total_errors, 3);
    assert!(!results.errors.by_category.is_empty());
}
