#![allow(
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::cast_lossless,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::no_effect_underscore_binding
)]
// Example: Context Budgeting and Prioritization
//
// This example demonstrates how to use the context budgeting system
// to optimize token usage while maintaining high-value context.

use rustycode_core::context::{ContextBudget, TokenCounter};
use rustycode_core::context_prio::{select_best, sort_by, ContextItem, Priority, SortStrategy};

fn main() -> anyhow::Result<()> {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║         Context Budgeting & Prioritization Example            ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    // ── Example 1: Basic Budget Tracking ─────────────────────────────────────
    println!("📊 Example 1: Basic Budget Tracking");
    println!("─────────────────────────────────────────────────────────\n");

    let mut budget = ContextBudget::new(1000);
    println!("Created budget with {} tokens", budget.total());

    budget.reserve(300)?;
    println!("Reserved 300 tokens, remaining: {}", budget.remaining());

    budget.use_reserved(200)?;
    println!("Used 200 tokens, remaining: {}", budget.remaining());

    budget.release(50);
    println!("Released 50 tokens, remaining: {}", budget.remaining());

    println!("Budget utilization: {:.1}%", budget.utilization() * 100.0);
    println!();

    // ── Example 2: Token Counting ────────────────────────────────────────────
    println!("🔢 Example 2: Token Counting");
    println!("─────────────────────────────────────────────────────────\n");

    let texts = vec![
        "Hello, world!",
        "This is a longer piece of text that uses more tokens.",
        "Short",
    ];

    for text in &texts {
        let tokens = TokenCounter::estimate_tokens(text);
        println!("Text: {:?}", text);
        println!("  Estimated tokens: {}\n", tokens);
    }

    // ── Example 3: Context Prioritization ────────────────────────────────────
    println!("⭐ Example 3: Context Prioritization");
    println!("─────────────────────────────────────────────────────────\n");

    let items = vec![
        ContextItem::new("System instructions and rules", Priority::Critical)
            .with_tag("system")
            .with_score_multiplier(2.0),
        ContextItem::new("User's current task description", Priority::Critical).with_tag("active"),
        ContextItem::new("Recent conversation history", Priority::High)
            .with_tag("conversation")
            .with_usage_count(50),
        ContextItem::new("Old conversation from last week", Priority::Low)
            .with_tag("conversation")
            .with_usage_count(1),
        ContextItem::new("Verbose debug logs", Priority::Minimal).with_tag("logs"),
    ];

    println!("Created {} context items:\n", items.len());

    for (i, item) in items.iter().enumerate() {
        let tokens = item.token_count;
        let score = item.score();
        let score_per_token = item.score_per_token();
        println!(
            "  {}. Priority: {:?}, Tokens: {}, Score: {:.1} ({:.2}/token)",
            i + 1,
            item.priority,
            tokens,
            score,
            score_per_token
        );
        println!("     Content: {}", item.content);
        println!();
    }

    // ── Example 4: Budget Enforcement ────────────────────────────────────────
    println!("💰 Example 4: Budget Enforcement");
    println!("─────────────────────────────────────────────────────────\n");

    let total_tokens: usize = items.iter().map(|item| item.token_count).sum();
    println!("Total tokens in all items: {}", total_tokens);

    // Simulate 50% budget reduction
    let budget = total_tokens / 2;
    println!("Budget constraint: {} tokens ({}% reduction)", budget, 50);

    let selected = select_best(&items, budget);
    let selected_tokens: usize = selected.iter().map(|item| item.token_count).sum();

    println!(
        "Selected {} items ({} tokens)",
        selected.len(),
        selected_tokens
    );

    let savings = total_tokens - selected_tokens;
    let savings_percent = (savings as f64 / total_tokens as f64) * 100.0;
    println!(
        "💡 Token savings: {} tokens ({:.1}%)\n",
        savings, savings_percent
    );

    println!("Selected items (by priority):");
    for (i, item) in selected.iter().enumerate() {
        println!("  {}. {:?}: {}", i + 1, item.priority, item.content);
    }
    println!();

    // ── Example 5: Different Sorting Strategies ───────────────────────────────
    println!("🔄 Example 5: Different Sorting Strategies");
    println!("─────────────────────────────────────────────────────────\n");

    let mut items_by_score = items.clone();
    sort_by(&mut items_by_score, SortStrategy::ByScore);

    println!("Top 3 items by score:");
    for (i, item) in items_by_score.iter().take(3).enumerate() {
        println!(
            "  {}. Score: {:.1}, Priority: {:?}",
            i + 1,
            item.score(),
            item.priority
        );
    }
    println!();

    // ── Summary ───────────────────────────────────────────────────────────────
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║                          Summary                                ║");
    println!("╠════════════════════════════════════════════════════════════════╣");
    println!("║  ✓ Budget tracking prevents exceeding token limits            ║");
    println!("║  ✓ Token counting provides usage estimates                    ║");
    println!("║  ✓ Prioritization ensures high-value content is included      ║");
    println!("║  ✓ Budget enforcement achieves significant token savings      ║");
    println!("║  ✓ Multiple strategies optimize for different use cases       ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    Ok(())
}
