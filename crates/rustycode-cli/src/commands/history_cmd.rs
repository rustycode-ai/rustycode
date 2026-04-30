//! History command implementation for conversation history management
//!
//! Provides CLI commands for:
//! - Listing recent conversations
//! - Searching conversations by query, tags, model, date range
//! - Showing full conversation details
//! - Exporting conversations to JSON or Markdown

use super::cli_args::HistoryCommand;
use anyhow::Result;
use rustycode_storage::conversation_history::{
    ConversationFilter, ConversationHistory, ExportFormat,
};
use std::path::PathBuf;

/// Execute history command
pub fn execute(cmd: HistoryCommand) -> Result<()> {
    let history = ConversationHistory::default_dir()?;

    match cmd {
        HistoryCommand::List { limit } => {
            let conversations = history.list(limit)?;
            if conversations.is_empty() {
                println!("No conversations found.");
                return Ok(());
            }

            println!("Recent conversations (showing {}):\n", conversations.len());
            for conv in &conversations {
                print_conversation_summary(conv);
            }
        }
        HistoryCommand::Search {
            query,
            limit,
            model,
            tags,
            since,
            until,
        } => {
            let mut filter = ConversationFilter {
                query: Some(query),
                limit,
                ..Default::default()
            };

            if let Some(model_name) = model {
                filter.model = Some(model_name);
            }

            if let Some(tags_str) = tags {
                filter.tags = tags_str.split(',').map(|s| s.trim().to_string()).collect();
            }

            if let Some(since_str) = since {
                filter.since = Some(parse_timestamp(&since_str)?);
            }

            if let Some(until_str) = until {
                filter.until = Some(parse_timestamp(&until_str)?);
            }

            let results = history.search(&filter)?;
            if results.is_empty() {
                println!("No conversations found matching the criteria.");
                return Ok(());
            }

            println!(
                "Found {} conversation(s) matching the criteria:\n",
                results.len()
            );
            for conv in &results {
                print_conversation_summary(conv);
            }
        }
        HistoryCommand::Show { id } => {
            let conv = history.load(&id)?;
            print_conversation_full(&conv);
        }
        HistoryCommand::Export { id, format, output } => {
            let export_format = match format.as_str() {
                "markdown" | "md" => ExportFormat::Markdown,
                "json" => ExportFormat::Json,
                _ => {
                    anyhow::bail!(
                        "Invalid format '{}'. Use 'json', 'markdown', or 'md'.",
                        format
                    );
                }
            };

            let output_path = if let Some(path) = output {
                PathBuf::from(path)
            } else {
                let ext = match export_format {
                    ExportFormat::Json => "json",
                    ExportFormat::Markdown => "md",
                    _ => "txt",
                };
                PathBuf::from(format!("{}.{}", id, ext))
            };

            history.export(&id, export_format, &output_path)?;
            println!("Exported conversation {} to {}", id, output_path.display());
        }
        HistoryCommand::Delete { id, force } => {
            if !force {
                println!("Are you sure you want to delete conversation {}? (y/N)", id);
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Delete cancelled.");
                    return Ok(());
                }
            }

            history.delete(&id)?;
            println!("Deleted conversation {}", id);
        }
    }

    Ok(())
}

/// Print a conversation summary line
fn print_conversation_summary(conv: &rustycode_storage::conversation_history::ConversationSummary) {
    let timestamp = format_timestamp(conv.updated_at);
    let id_short = if conv.id.len() > 8 {
        &conv.id[..8]
    } else {
        &conv.id
    };

    println!(
        "  {} | {} | {} msgs | {}",
        id_short, conv.title, conv.message_count, timestamp
    );

    if !conv.tags.is_empty() {
        println!("      Tags: {}", conv.tags.join(", "));
    }

    if conv.total_cost_cents > 0 {
        println!("      Cost: ${:.2}", conv.total_cost_cents as f64 / 100.0);
    }
}

/// Print full conversation details
fn print_conversation_full(conv: &rustycode_storage::conversation_history::Conversation) {
    println!("Title: {}", conv.title);
    println!("ID: {}", conv.id);
    println!("Model: {}", conv.model);
    println!("Provider: {}", conv.provider);
    println!("Created: {}", format_timestamp(conv.created_at));
    println!("Updated: {}", format_timestamp(conv.updated_at));
    println!("Messages: {}", conv.messages.len());

    if conv.total_tokens > 0 {
        println!("Total Tokens: {}", conv.total_tokens);
    }

    if conv.total_cost_cents > 0 {
        println!("Total Cost: ${:.2}", conv.total_cost_cents as f64 / 100.0);
    }

    if !conv.tags.is_empty() {
        println!("Tags: {}", conv.tags.join(", "));
    }

    if let Some(workspace) = &conv.workspace_path {
        println!("Workspace: {}", workspace);
    }

    println!("\n--- Messages ---\n");

    for msg in &conv.messages {
        println!(
            "## {} ({})",
            capitalize(&msg.role),
            format_timestamp(msg.timestamp)
        );
        println!("{}", msg.content);

        if let Some(tokens) = msg.tokens {
            println!("\n*Tokens: {}*", tokens);
        }

        println!();
    }
}

/// Format Unix timestamp as human-readable date
fn format_timestamp(ts: u64) -> String {
    match chrono::DateTime::from_timestamp(ts as i64, 0) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => ts.to_string(),
    }
}

/// Parse timestamp string (RFC3339 or Unix timestamp)
fn parse_timestamp(s: &str) -> Result<u64> {
    // Try Unix timestamp first
    if let Ok(ts) = s.parse::<u64>() {
        return Ok(ts);
    }

    // Try RFC3339
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.timestamp() as u64);
    }

    anyhow::bail!(
        "Invalid timestamp format: '{}'. Use RFC3339 or Unix timestamp.",
        s
    );
}

/// Capitalize first character of a string
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}
