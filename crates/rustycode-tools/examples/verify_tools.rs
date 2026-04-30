// Quick test to verify default_registry() loads tools
use rustycode_tools::default_registry;

fn main() {
    let registry = default_registry();
    let tools = registry.list();

    println!("Tool Registry Verification");
    println!("========================");
    println!("Total tools: {}", tools.len());
    println!();

    for tool in &tools {
        println!("✓ {} - {}", tool.name, tool.description);
    }

    println!();
    println!("Success! All {} tools registered.", tools.len());
}
