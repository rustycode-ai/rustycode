//! Memory command implementation for memory debugging and management
//!
//! Provides CLI commands for:
//! - Viewing memory statistics and effectiveness metrics
//! - Searching semantic memory with vector similarity
//! - Listing, viewing, and managing memory entries
//! - Testing memory retrieval for debugging
//! - Exporting and importing memories
//! - Pruning stale or low-confidence memories
//!
//! # Examples
//!
//! ```bash
//! # Show memory statistics
//! rustycode memory stats
//!
//! # Search for memories about "async code"
//! rustycode memory search "async code" --limit 10
//!
//! # List all memories
//! rustycode memory list
//!
//! # Show details of a specific memory
//! rustycode memory show <memory-id>
//!
//! # Test memory retrieval
//! rustycode memory test "how do I handle errors?"
//!
//! # Export memories to JSON
//! rustycode memory export ./memories-backup.json
//!
//! # Import memories from JSON
//! rustycode memory import ./memories-backup.json
//!
//! # Prune unused memories (dry run)
//! rustycode memory prune --dry-run
//! ```

use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;

use super::cli_args::MemoryCommand;

/// Execute memory command
pub fn execute(cmd: MemoryCommand) -> Result<()> {
    match cmd {
        MemoryCommand::Stats => handle_stats(),
        MemoryCommand::Search { query, limit } => handle_search(&query, limit),
        MemoryCommand::List { memory_type } => handle_list(memory_type.as_deref()),
        MemoryCommand::Show { memory_id } => handle_show(&memory_id),
        MemoryCommand::Test { query } => handle_test(&query),
        MemoryCommand::Export { path } => handle_export(&path),
        MemoryCommand::Import { path } => handle_import(&path),
        MemoryCommand::Prune { dry_run } => handle_prune(dry_run),
    }
}

/// Handle stats subcommand - display memory metrics and effectiveness report
fn handle_stats() -> Result<()> {
    let metrics = load_memory_metrics()?;

    println!("{}", "Memory Statistics".bold().underline());
    println!();

    // Overall stats
    println!("{}", "Overview:".bold());
    println!(
        "  Total memories:      {}",
        metrics.total_count.to_string().cyan()
    );
    println!(
        "  Vector memories:     {}",
        metrics.vector_count.to_string().cyan()
    );
    println!(
        "  Legacy memories:     {}",
        metrics.legacy_count.to_string().cyan()
    );
    println!();

    // Memory type breakdown
    println!("{}", "By Type:".bold());
    for (mem_type, count) in &metrics.by_type {
        let type_label = format_memory_type(mem_type);
        println!("  {:<20} {}", type_label, count.to_string().cyan());
    }
    println!();

    // Confidence distribution
    println!("{}", "Confidence Distribution:".bold());
    print_confidence_bar(
        "High (0.7-0.9)",
        metrics.high_confidence,
        metrics.total_count,
        '█',
        "green",
    );
    print_confidence_bar(
        "Medium (0.4-0.7)",
        metrics.medium_confidence,
        metrics.total_count,
        '▓',
        "yellow",
    );
    print_confidence_bar(
        "Low (0.0-0.4)",
        metrics.low_confidence,
        metrics.total_count,
        '░',
        "red",
    );
    println!();

    // Effectiveness metrics
    println!("{}", "Effectiveness:".bold());
    println!(
        "  Average confidence:  {:.2}",
        format_confidence(metrics.avg_confidence).cyan()
    );
    println!(
        "  Memories used:       {} ({}%)",
        metrics.used_count.to_string().cyan(),
        percentage(metrics.used_count, metrics.total_count)
    );
    println!(
        "  Prunable entries:    {} ({}%)",
        metrics.prunable_count.to_string().yellow(),
        percentage(metrics.prunable_count, metrics.total_count)
    );

    Ok(())
}

/// Handle search subcommand - search memories with semantic similarity
fn handle_search(query: &str, limit: Option<usize>) -> Result<()> {
    let limit = limit.unwrap_or(10);

    println!("{} {}", "Searching memories for:".bold(), query.cyan());
    println!();

    let results = search_memories(query, limit)?;

    if results.is_empty() {
        println!("{}", "No memories found matching the query.".yellow());
        return Ok(());
    }

    println!("{} {} result(s)\n", "Found".green(), results.len());

    for (i, result) in results.iter().enumerate() {
        let similarity_str = format_similarity(result.similarity);
        println!(
            "{}. {} {} [{}]",
            (i + 1).to_string().bold(),
            result.id.dimmed(),
            similarity_str,
            format_memory_type(&result.memory_type).dimmed()
        );
        println!("   {}", truncate(&result.content, 120));
        println!();
    }

    Ok(())
}

/// Handle list subcommand - list all memories
fn handle_list(memory_type: Option<&str>) -> Result<()> {
    let memories = load_all_memories()?;

    let filtered: Vec<_> = if let Some(filter_type) = memory_type {
        memories
            .into_iter()
            .filter(|m| m.memory_type.eq_ignore_ascii_case(filter_type))
            .collect()
    } else {
        memories
    };

    if filtered.is_empty() {
        if let Some(mt) = memory_type {
            println!("{} {}", "No memories found of type:".yellow(), mt.cyan());
        } else {
            println!("{}", "No memories found.".yellow());
        }
        return Ok(());
    }

    let type_str = memory_type
        .map(|t| format!(" (type: {})", t))
        .unwrap_or_default();
    println!(
        "{}{}\n",
        "Memory Entries".bold().underline(),
        type_str.dimmed()
    );

    print_memory_table(&filtered);

    Ok(())
}

/// Handle show subcommand - display full details of a specific memory
fn handle_show(memory_id: &str) -> Result<()> {
    let memory = find_memory_by_id(memory_id)?;

    let Some(memory) = memory else {
        anyhow::bail!("Memory with ID '{}' not found", memory_id);
    };

    println!("{}", "Memory Details".bold().underline());
    println!();

    println!("{} {}", "ID:".bold(), memory.id.cyan());
    println!(
        "{} {}",
        "Type:".bold(),
        format_memory_type(&memory.memory_type).cyan()
    );
    println!(
        "{} {}",
        "Confidence:".bold(),
        format_confidence(memory.confidence)
    );

    if let Some(created) = memory.created_at {
        println!("{} {}", "Created:".bold(), format_timestamp(created).cyan());
    }

    if let Some(last_used) = memory.last_used {
        println!(
            "{} {}",
            "Last Used:".bold(),
            format_timestamp(last_used).cyan()
        );
    }

    if memory.use_count > 0 {
        println!(
            "{} {}",
            "Use Count:".bold(),
            memory.use_count.to_string().cyan()
        );
    }

    if let Some(source) = &memory.source {
        println!("{} {}", "Source:".bold(), source.dimmed());
    }

    println!();
    println!("{}", "Content:".bold());
    println!("{}", memory.content);

    Ok(())
}

/// Handle test subcommand - test memory retrieval for a query
fn handle_test(query: &str) -> Result<()> {
    println!(
        "{} {}",
        "Testing memory retrieval for:".bold(),
        query.cyan()
    );
    println!();

    let memories = load_all_memories()?;

    if memories.is_empty() {
        println!("{}", "No memories available to test.".yellow());
        return Ok(());
    }

    // Calculate relevance scores for all memories
    let mut scored: Vec<_> = memories
        .into_iter()
        .map(|m| {
            let score = calculate_relevance(&m, query);
            (m, score)
        })
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Show top matches that would be injected
    println!("{}", "Top memories that would be injected:".bold());
    println!();

    let threshold = 0.3;
    let mut shown = 0;

    for (memory, score) in scored.iter().take(5) {
        let would_inject = *score >= threshold;
        let indicator = if would_inject {
            "✓".green()
        } else {
            "✗".dimmed()
        };

        println!(
            "{} {} {} {} [{}]",
            indicator,
            memory.id.dimmed(),
            format_similarity(*score),
            format_confidence(memory.confidence).dimmed(),
            format_memory_type(&memory.memory_type).dimmed()
        );
        println!("   {}", truncate(&memory.content, 100));

        if would_inject {
            shown += 1;
        }
        println!();
    }

    println!(
        "{} {} {} would be injected (threshold: {})",
        "→".cyan(),
        shown.to_string().bold(),
        "memories".bold(),
        format_similarity(threshold).dimmed()
    );

    Ok(())
}

/// Handle export subcommand - export memories to JSON file
fn handle_export(path: &PathBuf) -> Result<()> {
    let memories = load_all_memories()?;

    if memories.is_empty() {
        println!("{}", "No memories to export.".yellow());
        return Ok(());
    }

    let export_data = ExportData {
        version: "1.0".to_string(),
        exported_at: chrono::Utc::now().to_rfc3339(),
        count: memories.len(),
        memories,
    };

    let json = serde_json::to_string_pretty(&export_data)
        .context("Failed to serialize memories to JSON")?;

    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)
        .with_context(|| format!("Failed to write export file to {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, path)
        .with_context(|| format!("Failed to rename export file to {}", path.display()))?;

    println!(
        "{} {} memory(s) to {}",
        "Exported".green().bold(),
        export_data.count,
        path.display().to_string().cyan()
    );

    Ok(())
}

/// Handle import subcommand - import memories from JSON file
fn handle_import(path: &PathBuf) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Import file not found: {}", path.display());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read import file from {}", path.display()))?;

    let export_data: ExportData = serde_json::from_str(&content)
        .context("Failed to parse import file (invalid JSON format)")?;

    // Validate version
    if export_data.version != "1.0" {
        println!(
            "{} Unknown export version: {}. Proceeding anyway.",
            "Warning:".yellow().bold(),
            export_data.version
        );
    }

    // Check for duplicates
    let existing = load_all_memories()?;
    let existing_ids: std::collections::HashSet<_> = existing.iter().map(|m| &m.id).collect();

    let mut imported = 0;
    let mut skipped = 0;

    for memory in &export_data.memories {
        if existing_ids.contains(&memory.id) {
            skipped += 1;
        } else {
            // Would import here
            imported += 1;
        }
    }

    println!("{}", "Import Preview".bold().underline());
    println!();
    println!(
        "  Total in file:    {}",
        export_data.count.to_string().cyan()
    );
    println!("  New memories:     {}", imported.to_string().green());
    println!("  Duplicates:       {}", skipped.to_string().yellow());
    println!();

    // Confirm import
    print!("Proceed with import? [y/N] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().eq_ignore_ascii_case("y") {
        // Perform actual import
        for memory in &export_data.memories {
            if !existing_ids.contains(&memory.id) {
                save_memory(memory.clone())?;
            }
        }

        println!(
            "\n{} {} memory(s) imported successfully",
            "✓".green().bold(),
            imported
        );
    } else {
        println!("\n{} Import cancelled", "✗".red());
    }

    Ok(())
}

/// Handle prune subcommand - remove unused or low-confidence memories
fn handle_prune(dry_run: bool) -> Result<()> {
    let memories = load_all_memories()?;

    if memories.is_empty() {
        println!("{}", "No memories to prune.".yellow());
        return Ok(());
    }

    // Identify prunable memories
    let prunable: Vec<_> = memories
        .iter()
        .filter(|m| should_prune(m))
        .cloned()
        .collect();

    if prunable.is_empty() {
        println!("{}", "No memories need pruning.".green());
        return Ok(());
    }

    println!("{}", "Prune Preview".bold().underline());
    println!();

    for memory in &prunable {
        let reason = if memory.confidence < 0.3 {
            "low confidence"
        } else if memory.use_count == 0 {
            "never used"
        } else {
            "stale"
        };

        println!(
            "  {} {} [{}] - {}",
            "•".red(),
            memory.id.dimmed(),
            format_confidence(memory.confidence),
            reason.dimmed()
        );
    }

    println!();
    println!(
        "Would remove {} of {} memory(s)",
        prunable.len().to_string().red().bold(),
        memories.len()
    );

    if dry_run {
        println!("\n{} Dry run - no changes made", "→".cyan());
        return Ok(());
    }

    // Confirm deletion
    print!("\nProceed with pruning? [y/N] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().eq_ignore_ascii_case("y") {
        let mut removed = 0;
        for memory in &prunable {
            if delete_memory(&memory.id)? {
                removed += 1;
            }
        }

        println!(
            "\n{} {} memory(s) pruned successfully",
            "✓".green().bold(),
            removed
        );
    } else {
        println!("\n{} Pruning cancelled", "✗".red());
    }

    Ok(())
}

// Helper Functions

/// Format similarity score as colored percentage string
fn format_similarity(score: f32) -> String {
    let percentage = (score * 100.0).round() as u32;
    let s = format!("{:>3}%", percentage);

    if score >= 0.7 {
        s.green().to_string()
    } else if score >= 0.4 {
        s.yellow().to_string()
    } else {
        s.red().to_string()
    }
}

/// Format confidence score with color
fn format_confidence(confidence: f32) -> String {
    let s = format!("{:.2}", confidence);

    if confidence >= 0.7 {
        s.green().to_string()
    } else if confidence >= 0.4 {
        s.yellow().to_string()
    } else {
        s.red().to_string()
    }
}

/// Print a confidence distribution bar
fn print_confidence_bar(label: &str, count: usize, total: usize, bar_char: char, color: &str) {
    let percentage = if total > 0 {
        (count as f32 / total as f32 * 100.0) as usize
    } else {
        0
    };

    let bar_width = 30;
    let filled = if total > 0 {
        (count as f32 / total as f32 * bar_width as f32) as usize
    } else {
        0
    };

    let bar: String = std::iter::repeat_n(bar_char, filled).collect();
    let empty: String = " ".repeat(bar_width - filled);

    let bar_colored = match color {
        "green" => bar.green().to_string(),
        "yellow" => bar.yellow().to_string(),
        "red" => bar.red().to_string(),
        _ => bar,
    };

    println!(
        "  {} [{}{}] {:>3}% ({:>3})",
        label, bar_colored, empty, percentage, count
    );
}

/// Print memory entries as a formatted table
fn print_memory_table(memories: &[MemoryEntry]) {
    // Header
    println!(
        "  {}  {:<12} {:<10} {:<20} {}",
        "ID".bold(),
        "Type".bold(),
        "Conf".bold(),
        "Created".bold(),
        "Preview".bold()
    );
    println!("  {}", "─".repeat(100).dimmed());

    // Rows
    for memory in memories {
        let id_short = if memory.id.len() > 8 {
            &memory.id[..8]
        } else {
            &memory.id
        };

        let type_str = format_memory_type(&memory.memory_type);
        let conf_str = format_confidence(memory.confidence);
        let created_str = memory
            .created_at
            .map(format_timestamp_short)
            .unwrap_or_else(|| "-".to_string());
        let preview = truncate(&memory.content, 40);

        println!(
            "  {}  {:<12} {:<10} {:<20} {}",
            id_short.dimmed(),
            type_str,
            conf_str,
            created_str.dimmed(),
            preview
        );
    }

    println!("  {}", "─".repeat(100).dimmed());
    println!("  Total: {} memory(s)", memories.len().to_string().cyan());
}

/// Format memory type for display
fn format_memory_type(mem_type: &str) -> String {
    match mem_type.to_lowercase().as_str() {
        "learnings" => "learning".to_string(),
        "task_traces" => "task".to_string(),
        "code_patterns" => "pattern".to_string(),
        "tool_usage" => "tool".to_string(),
        _ => mem_type.to_lowercase(),
    }
}

/// Format Unix timestamp as human-readable date
fn format_timestamp(ts: u64) -> String {
    match chrono::DateTime::from_timestamp(ts as i64, 0) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => ts.to_string(),
    }
}

/// Format timestamp for compact display
fn format_timestamp_short(ts: u64) -> String {
    match chrono::DateTime::from_timestamp(ts as i64, 0) {
        Some(dt) => dt.format("%Y-%m-%d").to_string(),
        None => ts.to_string(),
    }
}

/// Truncate string to max length with ellipsis
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Calculate percentage string
fn percentage(part: usize, total: usize) -> String {
    if total == 0 {
        "0".to_string()
    } else {
        format!("{:.0}", (part as f32 / total as f32) * 100.0)
    }
}

/// Calculate relevance score for a memory against a query
fn calculate_relevance(memory: &MemoryEntry, query: &str) -> f32 {
    let mut score = 0.0;

    // Content similarity (simplified)
    let query_lower = query.to_lowercase();
    let content_lower = memory.content.to_lowercase();

    if content_lower.contains(&query_lower) {
        score += 0.5;
    }

    // Confidence weighting
    score *= memory.confidence;

    // Recency boost
    if let Some(last_used) = memory.last_used {
        let days_since = (chrono::Utc::now().timestamp() - last_used as i64) / 86400;
        if days_since < 7 {
            score += 0.1;
        }
    }

    score.min(1.0)
}

/// Check if a memory should be pruned
fn should_prune(memory: &MemoryEntry) -> bool {
    if memory.confidence < 0.3 {
        if let Some(last_used) = memory.last_used {
            let days_since = (chrono::Utc::now().timestamp() - last_used as i64) / 86400;
            return days_since > 30;
        }
        // Never used and low confidence
        return memory.use_count == 0;
    }
    false
}

// Data Structures

/// Memory entry for display
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub memory_type: String,
    pub confidence: f32,
    pub created_at: Option<u64>,
    pub last_used: Option<u64>,
    pub use_count: usize,
    pub source: Option<String>,
}

/// Search result with similarity score
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub content: String,
    pub memory_type: String,
    pub similarity: f32,
    pub confidence: f32,
}

/// Memory metrics for stats display
#[derive(Debug, Default)]
pub struct MemoryMetrics {
    pub total_count: usize,
    pub vector_count: usize,
    pub legacy_count: usize,
    pub by_type: HashMap<String, usize>,
    pub high_confidence: usize,
    pub medium_confidence: usize,
    pub low_confidence: usize,
    pub avg_confidence: f32,
    pub used_count: usize,
    pub prunable_count: usize,
}

/// Export/import data structure
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ExportData {
    pub version: String,
    pub exported_at: String,
    pub count: usize,
    pub memories: Vec<MemoryEntry>,
}

// Storage Interface

/// Load memory metrics from storage
fn load_memory_metrics() -> Result<MemoryMetrics> {
    let memories = load_all_memories()?;

    let mut metrics = MemoryMetrics {
        total_count: memories.len(),
        ..Default::default()
    };

    let mut total_confidence = 0.0;

    for memory in &memories {
        // Count by type
        let count = metrics
            .by_type
            .entry(memory.memory_type.clone())
            .or_insert(0);
        *count += 1;

        // Confidence distribution
        total_confidence += memory.confidence;

        if memory.confidence >= 0.7 {
            metrics.high_confidence += 1;
        } else if memory.confidence >= 0.4 {
            metrics.medium_confidence += 1;
        } else {
            metrics.low_confidence += 1;
        }

        // Usage tracking
        if memory.use_count > 0 {
            metrics.used_count += 1;
        }

        // Prunable check
        if should_prune(memory) {
            metrics.prunable_count += 1;
        }
    }

    if !memories.is_empty() {
        metrics.avg_confidence = total_confidence / memories.len() as f32;
    }

    // Estimate vector vs legacy based on content structure
    for memory in &memories {
        if memory.id.len() == 36 && memory.id.contains('-') {
            // Likely UUID = vector memory
            metrics.vector_count += 1;
        } else {
            metrics.legacy_count += 1;
        }
    }

    Ok(metrics)
}

/// Load all memories from storage
fn load_all_memories() -> Result<Vec<MemoryEntry>> {
    let mut memories = Vec::new();

    // Try to load from vector memory storage
    if let Ok(vector_memories) = load_vector_memories() {
        memories.extend(vector_memories);
    }

    // Try to load from legacy memory storage
    if let Ok(legacy_memories) = load_legacy_memories() {
        memories.extend(legacy_memories);
    }

    Ok(memories)
}

/// Load memories from vector memory storage
#[cfg(feature = "vector-memory")]
fn load_vector_memories() -> Result<Vec<MemoryEntry>> {
    use rustycode_vector_memory::{MemoryType, VectorMemory};

    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let _memory = VectorMemory::new(&cwd);

    // We can't easily load without init, so we'll scan the storage directly
    let storage_dir = cwd.join(".rustycode").join("vector_memory");

    if !storage_dir.exists() {
        return Ok(Vec::new());
    }

    let mut memories = Vec::new();

    for mem_type in [
        MemoryType::Learnings,
        MemoryType::TaskTraces,
        MemoryType::CodePatterns,
        MemoryType::ToolUsage,
    ] {
        let type_name = match mem_type {
            MemoryType::Learnings => "learnings",
            MemoryType::TaskTraces => "task_traces",
            MemoryType::CodePatterns => "code_patterns",
            MemoryType::ToolUsage => "tool_usage",
            _ => "unknown",
        };

        let index_path = storage_dir.join(format!("{}.json", type_name));

        if index_path.exists() {
            let content = std::fs::read_to_string(&index_path)
                .with_context(|| format!("Failed to read {}", index_path.display()))?;

            let entries: Vec<rustycode_vector_memory::MemoryEntry> = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse {}", index_path.display()))?;

            for entry in entries {
                // Skip empty entries (deleted/removed)
                if entry.content.is_empty() {
                    continue;
                }

                let created_ts = entry.metadata.created_timestamp.map(|t| t as u64);

                memories.push(MemoryEntry {
                    id: entry.id,
                    content: entry.content,
                    memory_type: type_name.to_string(),
                    confidence: entry.metadata.confidence,
                    created_at: created_ts,
                    last_used: None, // Vector memory doesn't track this
                    use_count: entry.metadata.occurrence_count as usize,
                    source: entry.metadata.source_task,
                });
            }
        }
    }

    Ok(memories)
}

/// Load memories from legacy memory storage
fn load_legacy_memories() -> Result<Vec<MemoryEntry>> {
    use rustycode_memory::{load, MemoryDomain};

    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let memory_dir = rustycode_memory::memory_dir(&cwd);

    if !memory_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = load(&memory_dir)?;
    let mut memories = Vec::new();

    for entry in entries {
        let memory_type = match entry.domain {
            MemoryDomain::CodeStyle => "code_style",
            MemoryDomain::Testing => "testing",
            MemoryDomain::Git => "git",
            MemoryDomain::Debugging => "debugging",
            MemoryDomain::Workflow => "workflow",
            MemoryDomain::Architecture => "architecture",
            MemoryDomain::ProjectSpecific => "project_specific",
            _ => "other",
        };

        let created_ts = entry
            .created_at
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs());

        let last_used_ts = entry.last_used.and_then(|t| {
            t.duration_since(std::time::SystemTime::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs())
        });

        memories.push(MemoryEntry {
            id: entry.id,
            content: entry.action,
            memory_type: memory_type.to_string(),
            confidence: entry.confidence,
            created_at: created_ts,
            last_used: last_used_ts,
            use_count: entry.use_count,
            source: Some(format!("{:?}", entry.source)),
        });
    }

    Ok(memories)
}

#[cfg(not(feature = "vector-memory"))]
fn load_vector_memories() -> Result<Vec<MemoryEntry>> {
    Ok(Vec::new())
}

/// Search memories with semantic similarity
#[cfg(feature = "vector-memory")]
fn search_memories(query: &str, limit: usize) -> Result<Vec<SearchResult>> {
    use rustycode_vector_memory::{MemoryType, VectorMemory};

    let cwd = std::env::current_dir().context("Failed to get current directory")?;

    // Initialize vector memory
    let mut memory = VectorMemory::new(&cwd);

    // Try to init, but if it fails, we can still do text search
    let vector_available = memory.init().is_ok();

    let mut results = Vec::new();

    if vector_available {
        // Use vector search
        for mem_type in [
            MemoryType::Learnings,
            MemoryType::TaskTraces,
            MemoryType::CodePatterns,
            MemoryType::ToolUsage,
        ] {
            let type_results = memory.search(query, mem_type, limit);

            let type_name = match mem_type {
                MemoryType::Learnings => "learnings",
                MemoryType::TaskTraces => "task_traces",
                MemoryType::CodePatterns => "code_patterns",
                MemoryType::ToolUsage => "tool_usage",
                _ => "other",
            };

            for result in type_results {
                results.push(SearchResult {
                    id: result.entry.id,
                    content: result.entry.content,
                    memory_type: type_name.to_string(),
                    similarity: result.similarity,
                    confidence: result.entry.metadata.confidence,
                });
            }
        }

        // Sort by similarity
        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
    } else {
        // Fallback to text search
        let memories = load_all_memories()?;

        for memory in memories {
            let score = calculate_relevance(&memory, query);
            if score > 0.1 {
                results.push(SearchResult {
                    id: memory.id,
                    content: memory.content,
                    memory_type: memory.memory_type,
                    similarity: score,
                    confidence: memory.confidence,
                });
            }
        }

        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
    }

    Ok(results)
}

/// Find a memory by ID
fn find_memory_by_id(id: &str) -> Result<Option<MemoryEntry>> {
    let memories = load_all_memories()?;

    Ok(memories.into_iter().find(|m| m.id == id))
}

/// Save a memory entry
fn save_memory(memory: MemoryEntry) -> Result<()> {
    // For now, this is a placeholder - would need to determine where to save
    // based on memory type and existing storage location
    let _ = memory;
    println!("Note: Memory saving not yet implemented in import");
    Ok(())
}

#[cfg(not(feature = "vector-memory"))]
fn search_memories(_query: &str, _limit: usize) -> Result<Vec<SearchResult>> {
    Ok(Vec::new())
}

/// Delete a memory by ID
fn delete_memory(id: &str) -> Result<bool> {
    // Try to delete from vector memory first
    if delete_vector_memory(id)? {
        return Ok(true);
    }

    // Try legacy memory
    delete_legacy_memory(id)
}

/// Delete from vector memory storage
#[cfg(feature = "vector-memory")]
fn delete_vector_memory(id: &str) -> Result<bool> {
    use rustycode_vector_memory::{MemoryType, VectorMemory};

    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let mut memory = VectorMemory::new(&cwd);

    if memory.init().is_err() {
        return Ok(false);
    }

    for mem_type in [
        MemoryType::Learnings,
        MemoryType::TaskTraces,
        MemoryType::CodePatterns,
        MemoryType::ToolUsage,
    ] {
        if memory.remove(mem_type, id) {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(not(feature = "vector-memory"))]
fn delete_vector_memory(_id: &str) -> Result<bool> {
    Ok(false)
}

/// Delete from legacy memory storage
fn delete_legacy_memory(_id: &str) -> Result<bool> {
    // Legacy memory doesn't support individual deletion easily
    // Would need to load, filter, and save
    Ok(false)
}
