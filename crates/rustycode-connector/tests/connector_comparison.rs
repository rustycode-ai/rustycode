//! Integration tests for comparing tmux and iTerm2 connectors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::cast_precision_loss,
    clippy::ignored_unit_patterns
)]

#[allow(unused_imports)]
use rustycode_connector::{ITermConnector, SplitDirection, TerminalConnector, TmuxConnector};

/// Test helper to verify connector capabilities
struct CapabilityTest {
    name: &'static str,
    test_fn: fn(&mut dyn TerminalConnector) -> Result<(), String>,
}

impl CapabilityTest {
    fn new(
        name: &'static str,
        test_fn: fn(&mut dyn TerminalConnector) -> Result<(), String>,
    ) -> Self {
        Self { name, test_fn }
    }

    fn run(&self, connector: &mut dyn TerminalConnector) -> TestResult {
        match (self.test_fn)(connector) {
            Ok(()) => TestResult::Passed,
            Err(e) => TestResult::Failed { reason: e },
        }
    }
}

enum TestResult {
    Passed,
    Failed {
        reason: String,
    },
    #[allow(dead_code)]
    Skipped {
        reason: String,
    },
}

fn test_create_session(connector: &mut dyn TerminalConnector) -> Result<(), String> {
    let session = connector
        .create_session("test-session")
        .map_err(|e| format!("Failed to create session: {e}"))?;

    // Verify session was created by getting info
    let info = connector
        .session_info(&session)
        .map_err(|e| format!("Failed to get session info: {e}"))?;

    assert_eq!(info.id, session);
    assert!(!info.panes.is_empty());

    // Cleanup
    connector
        .close_session(&session)
        .map_err(|e| format!("Failed to close session: {e}"))?;

    Ok(())
}

fn test_split_horizontal(connector: &mut dyn TerminalConnector) -> Result<(), String> {
    let session = connector
        .create_session("split-test")
        .map_err(|e| format!("Failed to create session: {e}"))?;

    let new_pane = connector
        .split_pane(&session, 0, SplitDirection::Horizontal)
        .map_err(|e| format!("Failed to split horizontally: {e}"))?;

    let info = connector
        .session_info(&session)
        .map_err(|e| format!("Failed to get session info: {e}"))?;

    assert_eq!(info.panes.len(), 2, "Should have 2 panes after split");
    assert_eq!(new_pane, 1, "New pane should have index 1");

    connector
        .close_session(&session)
        .map_err(|e| format!("Failed to close session: {e}"))?;

    Ok(())
}

fn test_split_vertical(connector: &mut dyn TerminalConnector) -> Result<(), String> {
    let session = connector
        .create_session("split-v-test")
        .map_err(|e| format!("Failed to create session: {e}"))?;

    let _new_pane = connector
        .split_pane(&session, 0, SplitDirection::Vertical)
        .map_err(|e| format!("Failed to split vertically: {e}"))?;

    let info = connector
        .session_info(&session)
        .map_err(|e| format!("Failed to get session info: {e}"))?;

    assert_eq!(info.panes.len(), 2, "Should have 2 panes after split");

    connector
        .close_session(&session)
        .map_err(|e| format!("Failed to close session: {e}"))?;

    Ok(())
}

fn test_send_keys(connector: &mut dyn TerminalConnector) -> Result<(), String> {
    let session = connector
        .create_session("keys-test")
        .map_err(|e| format!("Failed to create session: {e}"))?;

    connector
        .send_keys(&session, 0, "echo test")
        .map_err(|e| format!("Failed to send keys: {e}"))?;

    // Note: We can't verify the output was actually sent without capture_output
    // This test just verifies the API call succeeds

    connector
        .close_session(&session)
        .map_err(|e| format!("Failed to close session: {e}"))?;

    Ok(())
}

fn test_capture_output(connector: &mut dyn TerminalConnector) -> Result<(), String> {
    let session = connector
        .create_session("capture-test")
        .map_err(|e| format!("Failed to create session: {e}"))?;

    // Send some output
    connector
        .send_keys(&session, 0, "echo hello")
        .map_err(|e| format!("Failed to send keys: {e}"))?;

    // Give it a moment to execute
    std::thread::sleep(std::time::Duration::from_millis(100));

    let content = connector
        .capture_output(&session, 0)
        .map_err(|e| format!("Failed to capture output: {e}"))?;

    // tmux should capture something, iTerm2 may not support this
    println!("Captured {} characters", content.text.len());

    connector
        .close_session(&session)
        .map_err(|e| format!("Failed to close session: {e}"))?;

    Ok(())
}

fn test_set_pane_title(connector: &mut dyn TerminalConnector) -> Result<(), String> {
    let session = connector
        .create_session("title-test")
        .map_err(|e| format!("Failed to create session: {e}"))?;

    connector
        .set_pane_title(&session, 0, "Test Title")
        .map_err(|e| format!("Failed to set pane title: {e}"))?;

    connector
        .close_session(&session)
        .map_err(|e| format!("Failed to close session: {e}"))?;

    Ok(())
}

fn test_select_pane(connector: &mut dyn TerminalConnector) -> Result<(), String> {
    let session = connector
        .create_session("select-test")
        .map_err(|e| format!("Failed to create session: {e}"))?;

    // First split to create a second pane
    connector
        .split_pane(&session, 0, SplitDirection::Horizontal)
        .map_err(|e| format!("Failed to split: {e}"))?;

    // Select the new pane
    connector
        .select_pane(&session, 1)
        .map_err(|e| format!("Failed to select pane: {e}"))?;

    connector
        .close_session(&session)
        .map_err(|e| format!("Failed to close session: {e}"))?;

    Ok(())
}

fn test_kill_pane(connector: &mut dyn TerminalConnector) -> Result<(), String> {
    let session = connector
        .create_session("kill-test")
        .map_err(|e| format!("Failed to create session: {e}"))?;

    // Split to create a second pane
    let new_pane = connector
        .split_pane(&session, 0, SplitDirection::Horizontal)
        .map_err(|e| format!("Failed to split: {e}"))?;

    // Kill the new pane
    connector
        .kill_pane(&session, new_pane)
        .map_err(|e| format!("Failed to kill pane: {e}"))?;

    let info = connector
        .session_info(&session)
        .map_err(|e| format!("Failed to get session info: {e}"))?;

    assert_eq!(info.panes.len(), 1, "Should have 1 pane after kill");

    connector
        .close_session(&session)
        .map_err(|e| format!("Failed to close session: {e}"))?;

    Ok(())
}

fn test_multiple_splits(connector: &mut dyn TerminalConnector) -> Result<(), String> {
    let session = connector
        .create_session("multi-split-test")
        .map_err(|e| format!("Failed to create session: {e}"))?;

    // Create a 2x2 grid
    let p1 = connector
        .split_pane(&session, 0, SplitDirection::Horizontal)
        .map_err(|e| format!("Failed horizontal split: {e}"))?;

    let _p2 = connector
        .split_pane(&session, 0, SplitDirection::Vertical)
        .map_err(|e| format!("Failed vertical split: {e}"))?;

    let _p3 = connector
        .split_pane(&session, p1, SplitDirection::Vertical)
        .map_err(|e| format!("Failed second vertical split: {e}"))?;

    let info = connector
        .session_info(&session)
        .map_err(|e| format!("Failed to get session info: {e}"))?;

    assert!(
        info.panes.len() >= 4,
        "Should have at least 4 panes, got {}",
        info.panes.len()
    );

    connector
        .close_session(&session)
        .map_err(|e| format!("Failed to close session: {e}"))?;

    Ok(())
}

fn run_connector_tests<C: TerminalConnector + 'static>(
    connector: &mut C,
    connector_name: &str,
) -> Vec<(&'static str, TestResult)> {
    let mut results = Vec::new();

    let tests: Vec<CapabilityTest> = vec![
        CapabilityTest::new("create_session", test_create_session),
        CapabilityTest::new("split_horizontal", test_split_horizontal),
        CapabilityTest::new("split_vertical", test_split_vertical),
        CapabilityTest::new("send_keys", test_send_keys),
        CapabilityTest::new("capture_output", test_capture_output),
        CapabilityTest::new("set_pane_title", test_set_pane_title),
        CapabilityTest::new("select_pane", test_select_pane),
        CapabilityTest::new("kill_pane", test_kill_pane),
        CapabilityTest::new("multiple_splits", test_multiple_splits),
    ];

    println!("\n=== Testing {connector_name} ===");

    for test in tests {
        print!("  {}... ", test.name);
        let result = test.run(connector);
        match &result {
            TestResult::Passed => {
                println!("PASSED");
                results.push((test.name, result));
            }
            TestResult::Failed { reason } => {
                println!("FAILED: {reason}");
                results.push((test.name, result));
            }
            TestResult::Skipped { reason } => {
                println!("SKIPPED: {reason}");
                results.push((test.name, result));
            }
        }
    }

    results
}

#[test]
#[ignore = "requires live terminal"] // Opens tmux sessions — run with `cargo test -- --ignored` explicitly
fn test_tmux_connector_capabilities() {
    let mut tmux = TmuxConnector::new("test");

    if !tmux.is_available() {
        println!("tmux not available, skipping tests");
        return;
    }

    let results = run_connector_tests(&mut tmux, "tmux");

    let passed = results
        .iter()
        .filter(|(_, r)| matches!(r, TestResult::Passed))
        .count();
    let failed = results
        .iter()
        .filter(|(_, r)| matches!(r, TestResult::Failed { .. }))
        .count();

    println!("\ntmux results: {passed} passed, {failed} failed");

    // tmux should support all operations
    assert!(passed >= 7, "tmux should pass at least 7 tests");
}

#[test]
#[ignore = "requires live terminal"] // Opens iTerm2 windows — run with `cargo test -- --ignored` explicitly
#[cfg(target_os = "macos")]
fn test_iterm_connector_capabilities() {
    let mut iterm = ITermConnector::new();

    if !ITermConnector::is_available() {
        println!("iTerm2 not available, skipping tests");
        return;
    }

    let results = run_connector_tests(&mut iterm, "iTerm2");

    let passed = results
        .iter()
        .filter(|(_, r)| matches!(r, TestResult::Passed))
        .count();
    let failed = results
        .iter()
        .filter(|(_, r)| matches!(r, TestResult::Failed { .. }))
        .count();

    println!("\niTerm2 results: {passed} passed, {failed} failed");

    // iTerm2 has limitations - at minimum should support create, split, send
    assert!(passed >= 3, "iTerm2 should pass at least basic operations");
}

/// Comparison test that runs the same operations on both connectors
#[test]
#[ignore = "requires live terminal"] // Opens terminal sessions — run with `cargo test -- --ignored` explicitly
fn compare_connectors() {
    let tmux = TmuxConnector::new("compare");
    let tmux_available = tmux.is_available();

    #[cfg(target_os = "macos")]
    let iterm = ITermConnector::new();
    #[cfg(target_os = "macos")]
    let iterm_available = iterm.is_available();

    #[cfg(not(target_os = "macos"))]
    let iterm_available = false;

    println!("\n{}", "=".repeat(60));
    println!("CONNECTOR COMPARISON");
    println!("{}", "=".repeat(60));
    println!("tmux available: {tmux_available}");
    println!("iTerm2 available: {iterm_available}");

    if !tmux_available && !iterm_available {
        println!("No connectors available for comparison");
        return;
    }

    // Compare basic operations
    if tmux_available {
        println!("\n--- tmux ---");
        let mut tmux = TmuxConnector::new("compare");
        let tmux_results = run_connector_tests(&mut tmux, "tmux");
        print_summary("tmux", &tmux_results);
    }

    #[cfg(target_os = "macos")]
    if iterm_available {
        println!("\n--- iTerm2 ---");
        let mut iterm = ITermConnector::new();
        let iterm_results = run_connector_tests(&mut iterm, "iTerm2");
        print_summary("iTerm2", &iterm_results);
    }
}

fn print_summary(name: &str, results: &[(&'static str, TestResult)]) {
    let passed = results
        .iter()
        .filter(|(_, r)| matches!(r, TestResult::Passed))
        .count();
    let total = results.len();
    println!(
        "{}: {}/{} tests passed ({:.0}%)",
        name,
        passed,
        total,
        (passed as f64 / total as f64) * 100.0
    );

    let failed: Vec<_> = results
        .iter()
        .filter_map(|(name, r)| match r {
            TestResult::Failed { reason } => Some((*name, reason)),
            _ => None,
        })
        .collect();

    if !failed.is_empty() {
        println!("  Failed operations:");
        for (op, reason) in &failed {
            println!("    - {op}: {reason}");
        }
    }
}
