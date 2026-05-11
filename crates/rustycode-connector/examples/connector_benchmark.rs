//! Connector Benchmark Tool
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::unnecessary_wraps
)]
//!
//! Compares performance of tmux, it2 (iTerm2 CLI), iTerm2 (`AppleScript`), and iTerm2 Native connectors.

#[allow(unused_imports)]
use rustycode_connector::{
    ITerm2NativeConnector, ITermConnector, It2Connector, SplitDirection, TerminalConnector,
    TmuxConnector,
};
use std::time::Instant;

/// Benchmark results for a connector
#[derive(Debug, Clone)]
struct BenchmarkResults {
    connector_name: &'static str,
    session_create_ms: Option<u128>,
    split_pane_ms: Option<u128>,
    send_keys_ms: Option<u128>,
    capture_output_ms: Option<u128>,
    set_title_ms: Option<u128>,
    select_pane_ms: Option<u128>,
    kill_pane_ms: Option<u128>,
    close_session_ms: Option<u128>,
    errors: Vec<String>,
}

impl BenchmarkResults {
    const fn new(connector_name: &'static str) -> Self {
        Self {
            connector_name,
            session_create_ms: None,
            split_pane_ms: None,
            send_keys_ms: None,
            capture_output_ms: None,
            set_title_ms: None,
            select_pane_ms: None,
            kill_pane_ms: None,
            close_session_ms: None,
            errors: Vec::new(),
        }
    }

    fn total_time(&self) -> Option<u128> {
        Some(
            self.session_create_ms.unwrap_or(0)
                + self.split_pane_ms.unwrap_or(0)
                + self.send_keys_ms.unwrap_or(0)
                + self.capture_output_ms.unwrap_or(0)
                + self.set_title_ms.unwrap_or(0)
                + self.select_pane_ms.unwrap_or(0)
                + self.kill_pane_ms.unwrap_or(0)
                + self.close_session_ms.unwrap_or(0),
        )
    }

    fn success_rate(&self) -> f64 {
        let total_ops = 8;
        let failed_ops = self.errors.len();
        ((total_ops - failed_ops) as f64 / total_ops as f64) * 100.0
    }
}

fn benchmark_connector<C: TerminalConnector>(connector: &mut C) -> BenchmarkResults {
    let mut results = BenchmarkResults::new(connector.name());

    println!("\n=== Benchmarking {} ===", connector.name());

    // Check availability first
    if !connector.is_available() {
        results.errors.push("Connector not available".to_string());
        println!("  {} is not available, skipping...", connector.name());
        return results;
    }

    // 1. Create session
    print!("  Creating session... ");
    let start = Instant::now();
    match connector.create_session("benchmark-test") {
        Ok(session) => {
            results.session_create_ms = Some(start.elapsed().as_millis());
            println!("{}ms", results.session_create_ms.unwrap());

            // 2. Split pane
            print!("  Splitting pane horizontally... ");
            let start = Instant::now();
            match connector.split_pane(&session, 0, SplitDirection::Horizontal) {
                Ok(pane_idx) => {
                    results.split_pane_ms = Some(start.elapsed().as_millis());
                    println!(
                        "{}ms (new pane: {})",
                        results.split_pane_ms.unwrap(),
                        pane_idx
                    );

                    // 3. Split again (vertical)
                    print!("  Splitting pane vertically... ");
                    let start = Instant::now();
                    match connector.split_pane(&session, 0, SplitDirection::Vertical) {
                        Ok(_) => {
                            let elapsed = start.elapsed().as_millis();
                            println!("{elapsed}ms");
                        }
                        Err(e) => {
                            println!("Error: {e}");
                            results.errors.push(format!("split_pane (vertical): {e}"));
                        }
                    }

                    // 3. Send keys
                    print!("  Sending keys... ");
                    let start = Instant::now();
                    match connector.send_keys(&session, 0, "echo 'Hello from pane 0'") {
                        Ok(()) => {
                            results.send_keys_ms = Some(start.elapsed().as_millis());
                            println!("{}ms", results.send_keys_ms.unwrap());
                        }
                        Err(e) => {
                            println!("Error: {e}");
                            results.errors.push(format!("send_keys: {e}"));
                        }
                    }

                    // 4. Capture output
                    print!("  Capturing output... ");
                    let start = Instant::now();
                    match connector.capture_output(&session, 0) {
                        Ok(content) => {
                            results.capture_output_ms = Some(start.elapsed().as_millis());
                            println!(
                                "{}ms ({} chars)",
                                results.capture_output_ms.unwrap(),
                                content.text.len()
                            );
                        }
                        Err(e) => {
                            println!("Error: {e}");
                            results.errors.push(format!("capture_output: {e}"));
                        }
                    }

                    // 5. Set pane title
                    print!("  Setting pane title... ");
                    let start = Instant::now();
                    match connector.set_pane_title(&session, 0, "Benchmark Pane") {
                        Ok(()) => {
                            results.set_title_ms = Some(start.elapsed().as_millis());
                            println!("{}ms", results.set_title_ms.unwrap());
                        }
                        Err(e) => {
                            println!("Error: {e}");
                            results.errors.push(format!("set_pane_title: {e}"));
                        }
                    }

                    // 6. Select pane
                    print!("  Selecting pane... ");
                    let start = Instant::now();
                    match connector.select_pane(&session, 1) {
                        Ok(()) => {
                            results.select_pane_ms = Some(start.elapsed().as_millis());
                            println!("{}ms", results.select_pane_ms.unwrap());
                        }
                        Err(e) => {
                            println!("Error: {e}");
                            results.errors.push(format!("select_pane: {e}"));
                        }
                    }

                    // 7. Kill pane
                    print!("  Killing pane... ");
                    let start = Instant::now();
                    match connector.kill_pane(&session, 1) {
                        Ok(()) => {
                            results.kill_pane_ms = Some(start.elapsed().as_millis());
                            println!("{}ms", results.kill_pane_ms.unwrap());
                        }
                        Err(e) => {
                            println!("Error: {e}");
                            results.errors.push(format!("kill_pane: {e}"));
                        }
                    }
                }
                Err(e) => {
                    println!("Error: {e}");
                    results.errors.push(format!("split_pane: {e}"));
                }
            }

            // 8. Close session
            print!("  Closing session... ");
            let start = Instant::now();
            match connector.close_session(&session) {
                Ok(()) => {
                    results.close_session_ms = Some(start.elapsed().as_millis());
                    println!("{}ms", results.close_session_ms.unwrap());
                }
                Err(e) => {
                    println!("Error: {e}");
                    results.errors.push(format!("close_session: {e}"));
                }
            }
        }
        Err(e) => {
            println!("Error: {e}");
            results.errors.push(format!("create_session: {e}"));
        }
    }

    results
}

fn print_comparison(results: &[BenchmarkResults]) {
    println!("\n{}", "=".repeat(70));
    println!("BENCHMARK COMPARISON RESULTS");
    println!("{}", "=".repeat(70));

    // Table header
    println!(
        "\n{:<15} {:>15} {:>15} {:>15} {:>15}",
        "Connector", "Session", "Split", "Send Keys", "Total"
    );
    println!("{}", "-".repeat(75));

    for result in results {
        if result.errors.is_empty() || result.total_time().is_some() {
            println!(
                "{:<15} {:>12}ms {:>12}ms {:>12}ms {:>12}ms",
                result.connector_name,
                result.session_create_ms.unwrap_or(0),
                result.split_pane_ms.unwrap_or(0),
                result.send_keys_ms.unwrap_or(0),
                result.total_time().unwrap_or(0)
            );
        } else {
            println!(
                "{:<15} {:>15}",
                result.connector_name, "N/A (not available)"
            );
        }
    }

    println!("\n{}", "-".repeat(75));
    println!("Detailed breakdown:");
    println!("{}", "-".repeat(75));

    for result in results {
        println!("\n{}:", result.connector_name);
        println!(
            "  Session create:    {:>8} ms",
            result
                .session_create_ms
                .map_or_else(|| "N/A".to_string(), |v| v.to_string())
        );
        println!(
            "  Split pane:        {:>8} ms",
            result
                .split_pane_ms
                .map_or_else(|| "N/A".to_string(), |v| v.to_string())
        );
        println!(
            "  Send keys:         {:>8} ms",
            result
                .send_keys_ms
                .map_or_else(|| "N/A".to_string(), |v| v.to_string())
        );
        println!(
            "  Capture output:    {:>8} ms",
            result
                .capture_output_ms
                .map_or_else(|| "N/A".to_string(), |v| v.to_string())
        );
        println!(
            "  Set pane title:    {:>8} ms",
            result
                .set_title_ms
                .map_or_else(|| "N/A".to_string(), |v| v.to_string())
        );
        println!(
            "  Select pane:       {:>8} ms",
            result
                .select_pane_ms
                .map_or_else(|| "N/A".to_string(), |v| v.to_string())
        );
        println!(
            "  Kill pane:         {:>8} ms",
            result
                .kill_pane_ms
                .map_or_else(|| "N/A".to_string(), |v| v.to_string())
        );
        println!(
            "  Close session:     {:>8} ms",
            result
                .close_session_ms
                .map_or_else(|| "N/A".to_string(), |v| v.to_string())
        );
        println!("  Success rate:      {:.1}%", result.success_rate());

        if !result.errors.is_empty() {
            println!("  Errors:");
            for err in &result.errors {
                println!("    - {err}");
            }
        }
    }

    // Performance comparison
    println!("\n{}", "=".repeat(70));
    println!("PERFORMANCE ANALYSIS");
    println!("{}", "=".repeat(70));

    let available: Vec<_> = results
        .iter()
        .filter(|r| r.total_time().is_some_and(|t| t > 0))
        .collect();

    if available.len() >= 2 {
        let fastest = available.iter().min_by_key(|r| r.total_time()).unwrap();

        println!(
            "\nFastest connector: {} ({}ms total)",
            fastest.connector_name,
            fastest.total_time().unwrap()
        );

        for result in &available {
            if result.connector_name != fastest.connector_name {
                let diff = result.total_time().unwrap() - fastest.total_time().unwrap();
                let pct = (diff as f64 / fastest.total_time().unwrap() as f64) * 100.0;
                println!(
                    "  {} is {}ms ({:.1}%) slower",
                    result.connector_name, diff, pct
                );
            }
        }
    } else if available.len() == 1 {
        println!(
            "\nOnly {} connector available for benchmarking",
            available[0].connector_name
        );
    } else {
        println!("\nNo connectors available for benchmarking");
    }

    // Recommendations
    println!("\n{}", "=".repeat(70));
    println!("RECOMMENDATIONS");
    println!("{}", "=".repeat(70));

    for result in &available {
        println!("\n{} improvements needed:", result.connector_name);

        if result.capture_output_ms.is_none() {
            println!("  - CRITICAL: capture_output not implemented - blocks output verification");
        }
        if result.kill_pane_ms.is_none() {
            println!("  - HIGH: kill_pane not implemented - cannot clean up individual panes");
        }
        if result.session_create_ms.is_some_and(|v| v > 100) {
            println!(
                "  - MEDIUM: Session creation is slow ({}ms) - consider caching",
                result.session_create_ms.unwrap()
            );
        }
        if result.send_keys_ms.is_some_and(|v| v > 50) {
            println!(
                "  - MEDIUM: Send keys is slow ({}ms) - consider batching",
                result.send_keys_ms.unwrap()
            );
        }

        if result.errors.is_empty() && result.total_time().unwrap() < 50 {
            println!("  - All operations working well!");
        }
    }
}

fn main() {
    println!("{}", "=".repeat(70));
    println!("CONNECTOR BENCHMARK TOOL");
    println!("Comparing terminal connector performance");
    println!("{}", "=".repeat(70));

    let mut results = Vec::new();

    // Benchmark tmux
    let mut tmux = TmuxConnector::new("benchmark");
    results.push(benchmark_connector(&mut tmux));

    // Benchmark it2 CLI (macOS only)
    #[cfg(target_os = "macos")]
    {
        let mut it2 = It2Connector::new();
        results.push(benchmark_connector(&mut it2));
    }

    // Benchmark iTerm2 AppleScript (macOS only)
    #[cfg(target_os = "macos")]
    {
        let mut iterm = ITermConnector::new();
        results.push(benchmark_connector(&mut iterm));
    }

    // Benchmark iTerm2 Native (macOS only)
    #[cfg(target_os = "macos")]
    {
        let mut iterm_native = ITerm2NativeConnector::new();
        results.push(benchmark_connector(&mut iterm_native));
    }

    // Print comparison
    print_comparison(&results);
}
