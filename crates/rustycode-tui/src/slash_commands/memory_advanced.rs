//! Advanced memory slash commands
//!
//! Provides commands for managing auto-memories:
//! /memory preferences - Show all preferences
//! /memory decisions - Show all decisions
//! /memory errors - Show all errors
//! /memory recent - Show recent memories
//! /memory important - Show important memories
//! /memory cleanup - Clean up old memories

use crate::memory_auto::{MemoryType, ThreadSafeAutoMemory};
use anyhow::Result;
use std::sync::Arc;

/// Handle /memory commands
pub fn handle_memory_command(
    _state: &mut crate::tasks::WorkspaceTasks,
    args: &[String],
    auto_memory: &Option<Arc<ThreadSafeAutoMemory>>,
) -> Result<String> {
    if args.is_empty() {
        return Ok(format!(
            "💾 Auto-Memory Commands:\n\
             /memory preferences - Show all preferences\n\
             /memory decisions - Show all decisions\n\
             /memory errors - Show all errors\n\
             /memory recent [days] - Show recent memories (default: 7 days)\n\
             /memory important [threshold] - Show important memories (default: 0.7)\n\
             /memory cleanup - Clean up old memories\n\
             /memory suggest <context> - Get suggestions for context\n\
             /memory stats - Show memory statistics\n\
             \n\
             Current: 💾 {} memories stored",
            auto_memory.as_ref().map(|m| m.count()).unwrap_or(0)
        ));
    }

    let subcommand = args[0].to_lowercase();
    let sub_args: Vec<String> = args[1..].to_vec();

    match subcommand.as_str() {
        "preferences" => show_preferences(auto_memory),
        "decisions" => show_decisions(auto_memory),
        "errors" => show_errors(auto_memory),
        "recent" => show_recent(auto_memory, &sub_args),
        "important" => show_important(auto_memory, &sub_args),
        "cleanup" => cleanup_memories(auto_memory),
        "suggest" => suggest_memories(auto_memory, &sub_args),
        "stats" => show_stats(auto_memory),
        _ => Ok(format!(
            "❌ Unknown memory command: {}\n\
             Use /memory for available commands",
            subcommand
        )),
    }
}

/// Show all preferences
fn show_preferences(auto_memory: &Option<Arc<ThreadSafeAutoMemory>>) -> Result<String> {
    let Some(manager) = auto_memory else {
        return Ok("❌ Auto-memory not available".to_string());
    };

    let preferences = manager.preferences();

    if preferences.is_empty() {
        return Ok("📋 No preferences saved yet".to_string());
    }

    let mut output = format!("⚙️ Preferences ({} total):\n\n", preferences.len());

    for pref in &preferences {
        output.push_str(&format!(
            "  • {}: {}\n    📊 Importance: {:.0}% | 👁 Accessed: {} times\n\n",
            pref.key,
            pref.value,
            pref.importance * 100.0,
            pref.access_count
        ));
    }

    Ok(output)
}

/// Show all decisions
fn show_decisions(auto_memory: &Option<Arc<ThreadSafeAutoMemory>>) -> Result<String> {
    let Some(manager) = auto_memory else {
        return Ok("❌ Auto-memory not available".to_string());
    };

    let decisions = manager.decisions();

    if decisions.is_empty() {
        return Ok("🎯 No decisions saved yet".to_string());
    }

    let mut output = format!("🎯 Decisions ({} total):\n\n", decisions.len());

    for decision in &decisions {
        output.push_str(&format!(
            "  • {}\n    📝 {}\n    📊 Importance: {:.0}% | 📅 {}\n\n",
            decision.key,
            decision.value,
            decision.importance * 100.0,
            decision.created_at.format("%Y-%m-%d")
        ));
    }

    Ok(output)
}

/// Show all errors
fn show_errors(auto_memory: &Option<Arc<ThreadSafeAutoMemory>>) -> Result<String> {
    let Some(manager) = auto_memory else {
        return Ok("❌ Auto-memory not available".to_string());
    };

    let errors = manager.errors();

    if errors.is_empty() {
        return Ok("✅ No errors recorded yet".to_string());
    }

    let mut output = format!("🐛 Errors & Solutions ({} total):\n\n", errors.len());

    for error in &errors {
        output.push_str(&format!(
            "  • {}\n    💡 {}\n    📊 Importance: {:.0}% | 📅 {}\n\n",
            error.key,
            error.value,
            error.importance * 100.0,
            error.created_at.format("%Y-%m-%d")
        ));
    }

    Ok(output)
}

/// Show recent memories
fn show_recent(auto_memory: &Option<Arc<ThreadSafeAutoMemory>>, args: &[String]) -> Result<String> {
    let Some(manager) = auto_memory else {
        return Ok("❌ Auto-memory not available".to_string());
    };

    let days = args
        .first()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(7)
        .max(1) as i64;

    let recent = manager.recent(days);

    if recent.is_empty() {
        return Ok(format!("📅 No memories in the last {} days", days));
    }

    let mut output = format!(
        "📅 Recent memories (last {} days, {} total):\n\n",
        days,
        recent.len()
    );

    for memory in &recent {
        output.push_str(&format!(
            "  • [{}] {}\n    {}\n    👁 Accessed: {} times\n\n",
            format_memory_type(&memory.memory_type),
            memory.key,
            memory.value,
            memory.access_count
        ));
    }

    Ok(output)
}

/// Show important memories
fn show_important(
    auto_memory: &Option<Arc<ThreadSafeAutoMemory>>,
    args: &[String],
) -> Result<String> {
    let Some(manager) = auto_memory else {
        return Ok("❌ Auto-memory not available".to_string());
    };

    let threshold = args
        .first()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.7);

    let important = manager.important(threshold);

    if important.is_empty() {
        return Ok(format!(
            "⭐ No memories with importance >= {:.0}%",
            threshold * 100.0
        ));
    }

    let mut output = format!(
        "⭐ Important memories (>={:.0}%, {} total):\n\n",
        threshold * 100.0,
        important.len()
    );

    for memory in &important {
        output.push_str(&format!(
            "  • [{}] {}\n    {}\n    📊 Importance: {:.0}% | 👁 {}\n\n",
            format_memory_type(&memory.memory_type),
            memory.key,
            memory.value,
            memory.importance * 100.0,
            memory.access_count
        ));
    }

    Ok(output)
}

/// Cleanup old memories
fn cleanup_memories(auto_memory: &Option<Arc<ThreadSafeAutoMemory>>) -> Result<String> {
    let Some(manager) = auto_memory else {
        return Ok("❌ Auto-memory not available".to_string());
    };

    let removed = manager.cleanup()?;

    if removed == 0 {
        Ok("✅ Memory cleanup complete - nothing to remove".to_string())
    } else {
        Ok(format!(
            "🧹 Memory cleanup complete - removed {} old memories",
            removed
        ))
    }
}

/// Get suggestions for context
fn suggest_memories(
    auto_memory: &Option<Arc<ThreadSafeAutoMemory>>,
    args: &[String],
) -> Result<String> {
    let Some(manager) = auto_memory else {
        return Ok("❌ Auto-memory not available".to_string());
    };

    if args.is_empty() {
        return Ok(
            "❌ Please provide a context for suggestions\n  Example: /memory suggest theme"
                .to_string(),
        );
    }

    let context = args.join(" ");
    let suggestions = manager.get_suggestions(&context);

    if suggestions.is_empty() {
        return Ok(format!("💡 No suggestions found for: {}", context));
    }

    let mut output = format!("💡 Suggestions for '{}':\n\n", context);

    for suggestion in &suggestions {
        output.push_str(&format!("  {}\n", suggestion));
    }

    Ok(output)
}

/// Show memory statistics
fn show_stats(auto_memory: &Option<Arc<ThreadSafeAutoMemory>>) -> Result<String> {
    let Some(manager) = auto_memory else {
        return Ok("❌ Auto-memory not available".to_string());
    };

    let count = manager.count();
    let preferences = manager.preferences();
    let decisions = manager.decisions();
    let errors = manager.errors();

    let total_access: usize = preferences
        .iter()
        .chain(decisions.iter())
        .chain(errors.iter())
        .map(|m| m.access_count)
        .sum();

    let avg_importance: f64 = preferences
        .iter()
        .chain(decisions.iter())
        .chain(errors.iter())
        .map(|m| m.importance)
        .sum::<f64>()
        / count.max(1) as f64;

    let mut output = format!(
        "📊 Memory Statistics:\n\n\
         • Total memories: {}\n\
         • Preferences: {}\n\
         • Decisions: {}\n\
         • Errors: {}\n\
         • Total accesses: {}\n\
         • Average importance: {:.0}%\n",
        count,
        preferences.len(),
        decisions.len(),
        errors.len(),
        total_access,
        avg_importance * 100.0
    );

    // Find most accessed memory
    let most_accessed = preferences
        .iter()
        .chain(decisions.iter())
        .chain(errors.iter())
        .max_by_key(|m| m.access_count);

    if let Some(memory) = most_accessed {
        output.push_str(&format!(
            " • Most accessed: {} ({} times)\n",
            memory.key, memory.access_count
        ));
    }

    Ok(output)
}

/// Format memory type for display
fn format_memory_type(memory_type: &MemoryType) -> &str {
    match memory_type {
        MemoryType::Preference => "⚙️",
        MemoryType::Decision => "🎯",
        MemoryType::Error => "🐛",
        MemoryType::Context => "📝",
        MemoryType::Pattern => "🔄",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_auto::ThreadSafeAutoMemory;

    use tempfile::TempDir;

    #[test]
    fn test_format_memory_type() {
        assert_eq!(format_memory_type(&MemoryType::Preference), "⚙️");
        assert_eq!(format_memory_type(&MemoryType::Decision), "🎯");
        assert_eq!(format_memory_type(&MemoryType::Error), "🐛");
        assert_eq!(format_memory_type(&MemoryType::Context), "📝");
        assert_eq!(format_memory_type(&MemoryType::Pattern), "🔄");
    }

    #[test]
    fn test_show_preferences_empty() {
        let temp_dir = TempDir::new().unwrap();
        let auto_memory = Some(Arc::new(
            ThreadSafeAutoMemory::new(temp_dir.path()).unwrap(),
        ));

        let result = show_preferences(&auto_memory).unwrap();
        assert!(result.contains("No preferences saved yet"));
    }

    #[test]
    fn test_show_decisions_empty() {
        let temp_dir = TempDir::new().unwrap();
        let auto_memory = Some(Arc::new(
            ThreadSafeAutoMemory::new(temp_dir.path()).unwrap(),
        ));

        let result = show_decisions(&auto_memory).unwrap();
        assert!(result.contains("No decisions saved yet"));
    }

    #[test]
    fn test_show_errors_empty() {
        let temp_dir = TempDir::new().unwrap();
        let auto_memory = Some(Arc::new(
            ThreadSafeAutoMemory::new(temp_dir.path()).unwrap(),
        ));

        let result = show_errors(&auto_memory).unwrap();
        assert!(result.contains("No errors recorded yet"));
    }

    #[test]
    fn test_cleanup_memories() {
        let temp_dir = TempDir::new().unwrap();
        let auto_memory = Some(Arc::new(
            ThreadSafeAutoMemory::new(temp_dir.path()).unwrap(),
        ));

        let result = cleanup_memories(&auto_memory).unwrap();
        assert!(result.contains("cleanup complete"));
    }

    #[test]
    fn test_handle_memory_command_help() {
        let temp_dir = TempDir::new().unwrap();
        let mut state = crate::tasks::WorkspaceTasks {
            tasks: Vec::new(),
            todos: Vec::new(),
            active_agents: Vec::new(),
        };
        let auto_memory = Some(Arc::new(
            ThreadSafeAutoMemory::new(temp_dir.path()).unwrap(),
        ));

        let result = handle_memory_command(&mut state, &[], &auto_memory).unwrap();
        assert!(result.contains("Auto-Memory Commands"));
        assert!(result.contains("preferences"));
        assert!(result.contains("decisions"));
        assert!(result.contains("errors"));
    }

    #[test]
    fn test_handle_memory_command_unknown() {
        let temp_dir = TempDir::new().unwrap();
        let mut state = crate::tasks::WorkspaceTasks {
            tasks: Vec::new(),
            todos: Vec::new(),
            active_agents: Vec::new(),
        };
        let auto_memory = Some(Arc::new(
            ThreadSafeAutoMemory::new(temp_dir.path()).unwrap(),
        ));

        let result =
            handle_memory_command(&mut state, &["unknown".to_string()], &auto_memory).unwrap();
        assert!(result.contains("Unknown memory command"));
    }

    #[test]
    fn test_show_stats() {
        let temp_dir = TempDir::new().unwrap();
        let auto_memory = Some(Arc::new(
            ThreadSafeAutoMemory::new(temp_dir.path()).unwrap(),
        ));

        let result = show_stats(&auto_memory).unwrap();
        assert!(result.contains("Memory Statistics"));
        assert!(result.contains("Total memories"));
    }
}
