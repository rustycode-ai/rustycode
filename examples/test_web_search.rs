use rustycode_tools::Tool;
use rustycode_tools::{ToolContext, WebSearchTool};
use serde_json::json;

fn main() {
    let tool = WebSearchTool;
    let ctx = ToolContext::new(".");

    // Test 1: Factual question (should use Wikipedia)
    println!("=== Test 1: Factual Question ===");
    let params = json!({
        "query": "Prime Minister of Thailand",
        "num_results": 3,
        "source": "wikipedia"
    });

    match tool.execute(params, &ctx) {
        Ok(output) => println!("{}\n", output.text),
        Err(e) => println!("Error: {}\n", e),
    }

    // Test 2: Auto mode (should detect factual query and use Wikipedia)
    println!("=== Test 2: Auto Mode (Factual) ===");
    let params = json!({
        "query": "who is the CEO of NVIDIA",
        "num_results": 2,
        "source": "auto"
    });

    match tool.execute(params, &ctx) {
        Ok(output) => println!("{}\n", output.text),
        Err(e) => println!("Error: {}\n", e),
    }

    // Test 3: General web query (should use fallback or Exa if available)
    println!("=== Test 3: General Web Query ===");
    let params = json!({
        "query": "Rust programming language 2026",
        "num_results": 3,
        "source": "auto"
    });

    match tool.execute(params, &ctx) {
        Ok(output) => println!("{}\n", output.text),
        Err(e) => println!("Error: {}\n", e),
    }

    // Test 4: No API key fallback
    println!("=== Test 4: Fallback Mode ===");
    let params = json!({
        "query": "latest TypeScript version",
        "num_results": 5,
        "source": "web"
    });

    match tool.execute(params, &ctx) {
        Ok(output) => println!("{}", output.text),
        Err(e) => println!("Error: {}", e),
    }

    // Test 5: DuckDuckGo instant answers
    println!("=== Test 5: DuckDuckGo Instant Answer ===");
    let params = json!({
        "query": "NVIDIA CEO",
        "num_results": 3,
        "source": "auto"
    });

    match tool.execute(params, &ctx) {
        Ok(output) => println!("{}", output.text),
        Err(e) => println!("Error: {}", e),
    }
}
