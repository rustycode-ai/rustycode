//! Automatic task and todo extraction from LLM responses
//!
//! This module provides intelligent extraction of actionable items from:
//! - LLM assistant responses (bullet points, "I will" statements, plans)
//! - Tool outputs (TODO/FIXME comments, file analysis results)
//! - Conversation patterns (agreed upon actions, next steps)

use crate::app::tasks::{create_task, create_todo, WorkspaceTasks};
use rustycode_protocol::tool_names as tn;

/// Extract actionable items from text and add them to workspace tasks
pub fn extract_action_items(text: &str, tasks: &mut WorkspaceTasks) {
    // Extract todos (quick checklist items)
    let todos = extract_todos(text);
    for todo_text in &todos {
        // Avoid duplicates
        if !tasks.todos.iter().any(|t| t.text == *todo_text) {
            let todo = create_todo(todo_text.clone());
            tasks.todos.push(todo);
            tracing::info!("Auto-created todo: {}", todo_text);
        }
    }

    // Extract tasks (larger work items)
    let task_descriptions = extract_tasks(text);
    for description in &task_descriptions {
        // Avoid duplicates
        if !tasks.tasks.iter().any(|t| t.description == *description) {
            let task = create_task(description.clone());
            tasks.tasks.push(task);
            tracing::info!("Auto-created task: {}", description);
        }
    }
}

/// Extract todo items from text
///
/// Looks for:
/// - Bullet points at end of response
/// - Numbered lists
/// - "Next steps" sections
/// - Short actionable phrases (< 80 chars)
fn extract_todos(text: &str) -> Vec<String> {
    let mut todos = Vec::new();

    // Split into lines
    let lines: Vec<&str> = text.lines().collect();

    // Track if we're in a todo/list section
    let mut in_list = false;
    let mut list_end_buffer = 2; // Lines to wait before declaring list ended

    for line in &lines {
        let trimmed = line.trim();

        // Check for the new structured TODO format: "TODO: action | Done: criteria | Verify: how"
        if trimmed.to_uppercase().starts_with("TODO:") {
            let content = trimmed[5..].trim();
            if !content.is_empty() && content.len() > 3 {
                todos.push(content.to_string());
                in_list = true;
                list_end_buffer = 2;
            }
            continue;
        }

        // Check for section headers that indicate lists
        let trimmed_lower = trimmed.to_lowercase();
        if trimmed_lower.starts_with("next steps")
            || trimmed_lower.starts_with("todo")
            || trimmed_lower.starts_with("action items")
            || trimmed_lower.starts_with("checklist")
            || trimmed_lower.starts_with("to do:")
        {
            in_list = true;
            list_end_buffer = 2;
            continue;
        }

        // Check for Phase headers in structured plans
        if trimmed_lower.starts_with("phase ")
            && (trimmed_lower.contains(":") || trimmed_lower.contains("—"))
        {
            in_list = true;
            list_end_buffer = 2;
            continue;
        }

        // Check for list markers
        let is_bullet = trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("• ")
            || trimmed.starts_with("○ ")
            || trimmed.starts_with("◦ ");

        let is_numbered = trimmed
            .bytes()
            .enumerate()
            .take_while(|(_, b)| b.is_ascii_digit())
            .last()
            .map(|(idx, _)| {
                let after_digits = idx + 1;
                trimmed
                    .as_bytes()
                    .get(after_digits)
                    .map(|&b| b == b'.' || b == b')' || b == b' ')
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        if is_bullet || is_numbered {
            // Extract the content
            let content = if is_bullet {
                trimmed[2..].trim()
            } else if is_numbered {
                // Find the separator (". ", ") ", or just space)
                if let Some(pos) = trimmed.find(['.', ')', ' ']) {
                    &trimmed[pos + 1..]
                } else {
                    trimmed
                }
                .trim()
            } else {
                trimmed
            };

            // Only add if it's short and actionable
            if content.len() < 100 && content.len() > 3 && is_actionable_phrase(content) {
                todos.push(content.to_string());
                in_list = true;
                list_end_buffer = 2;
            }
        } else if trimmed.is_empty() {
            // Empty line - might be end of list
            if in_list {
                if list_end_buffer > 0 {
                    list_end_buffer -= 1;
                } else {
                    in_list = false;
                }
            }
        } else if !trimmed.is_empty() && !in_list {
            // Non-list content, not in list section
            continue;
        }
    }

    todos
}

/// Verbs that indicate non-actionable statements (thoughts, opinions, desires)
const NON_ACTION_VERBS: &[&str] = &[
    "think",
    "consider",
    "look at",
    "check out",
    "see",
    "believe",
    "feel",
    "hope",
    "want",
    "need",
    "wish",
    "wonder",
    "guess",
    "imagine",
    "suppose",
    "assume",
    "expect",
    "prefer",
    "like",
    "love",
    "hate",
    "mind",
    "care",
    "agree",
    "disagree",
    "discuss",
    "mention",
    "describe",
    "explain",
    "summarize",
    "note",
    "notice",
];

/// Extract task descriptions from text
///
/// Looks for:
/// - "I will" statements
/// - "I need to" statements
/// - "Let's" statements
/// - Plan sections with structure
/// - Multi-sentence action items (> 80 chars)
///
/// Filters out:
/// - Non-actionable statements (I will think about, I'll consider, etc.)
fn extract_tasks(text: &str) -> Vec<String> {
    let mut tasks = Vec::new();

    // Split into sentences
    let sentences: Vec<&str> = text
        .split('.')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    for sentence in &sentences {
        let lower = sentence.to_lowercase();

        // Pattern 1: "I will/need to/should" statements
        if lower.starts_with("i will ")
            || lower.starts_with("i'll ")
            || lower.starts_with("i need to ")
            || lower.starts_with("i should ")
        {
            // Extract the action part
            let action = if lower.starts_with("i will ") {
                &sentence[6..]
            } else if lower.starts_with("i'll ") {
                &sentence[5..]
            } else if lower.starts_with("i need to ") {
                &sentence[10..]
            } else if lower.starts_with("i should ") {
                &sentence[9..]
            } else {
                sentence
            };

            // Check if the first word is a non-action verb
            let first_word = action
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_lowercase();

            // Also check for multi-word phrases at the start of the action
            let action_lower = action.to_lowercase();
            let starts_with_non_action_phrase = NON_ACTION_VERBS
                .iter()
                .any(|phrase| action_lower.starts_with(phrase));

            if NON_ACTION_VERBS.contains(&first_word.as_str()) || starts_with_non_action_phrase {
                // Skip non-actionable statements like "I will think about it" or "I need to look at"
                continue;
            }

            if action.len() > 10 && action.len() < 200 {
                tasks.push(capitalize_first(action.trim()));
            }
        }

        // Pattern 2: "Let's" statements
        if lower.starts_with("let's ") || lower.starts_with("let us ") {
            let action = if lower.starts_with("let's ") {
                &sentence[6..]
            } else {
                &sentence[8..]
            };

            // Check if the first word is a non-action verb
            let first_word = action
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_lowercase();

            let action_lower = action.to_lowercase();
            let starts_with_non_action_phrase = NON_ACTION_VERBS
                .iter()
                .any(|phrase| action_lower.starts_with(phrase));

            if NON_ACTION_VERBS.contains(&first_word.as_str()) || starts_with_non_action_phrase {
                continue;
            }

            if action.len() > 10 && action.len() < 200 {
                tasks.push(capitalize_first(action.trim()));
            }
        }
    }

    // Remove duplicates while preserving order
    let mut seen = std::collections::HashSet::new();
    tasks.retain(|t| seen.insert(t.clone()));

    tasks
}

/// Extract todos from tool execution results
///
/// Parses:
/// - Code comments (TODO, FIXME, HACK)
/// - File listings
/// - Search results
pub fn extract_todos_from_tool_result(tool_name: &str, output: &str) -> Vec<String> {
    let mut todos = Vec::new();

    match tool_name {
        tn::GREP | "search_files" | "ripgrep" => {
            // Extract TODO/FIXME from grep results
            // Grep output format: "path/to/file.rs:42: // TODO: description"
            // We need to find the TODO:/FIXME: marker, not the first colon (which is in the file path)
            for line in output.lines() {
                let line_upper = line.to_uppercase();
                let marker_pos = if let Some(pos) = line_upper.find("TODO:") {
                    Some(pos + 5)
                } else {
                    line_upper.find("FIXME:").map(|pos| pos + 6)
                };
                if let Some(content_start) = marker_pos {
                    if content_start < line.len() {
                        let comment = line[content_start..].trim();
                        if !comment.is_empty() && comment.len() < 100 {
                            todos.push(format!("{}: {}", tool_name, comment));
                        }
                    }
                }
            }
        }
        tn::READ | "read" => {
            // Extract TODO/FIXME from file contents
            for line in output.lines() {
                let trimmed = line.trim();
                let upper = trimmed.to_uppercase();

                if upper.starts_with("// TODO:") || upper.starts_with("// FIXME:") {
                    let comment = trimmed.split_once(':').map(|x| x.1).unwrap_or("").trim();
                    if !comment.is_empty() {
                        todos.push(format!("TODO: {}", comment));
                    }
                } else if upper.starts_with("# TODO:") || upper.starts_with("# FIXME:") {
                    let comment = trimmed.split_once(':').map(|x| x.1).unwrap_or("").trim();
                    if !comment.is_empty() {
                        todos.push(format!("TODO: {}", comment));
                    }
                } else if upper.starts_with("/* TODO:") || upper.starts_with("/* FIXME:") {
                    let comment = trimmed.split_once(':').map(|x| x.1).unwrap_or("").trim();
                    if !comment.is_empty() {
                        todos.push(format!("TODO: {}", comment));
                    }
                }
            }
        }
        _ => {
            // Generic extraction - look for action items
            for line in output.lines() {
                let trimmed = line.trim();
                if is_actionable_phrase(trimmed) && trimmed.len() < 80 {
                    todos.push(trimmed.to_string());
                }
            }
        }
    }

    todos
}

/// Check if a phrase looks actionable
///
/// Actionable phrases typically:
/// - Start with a verb
/// - Are imperative or future tense
/// - Describe concrete actions
fn is_actionable_phrase(text: &str) -> bool {
    if text.is_empty() || text.len() < 5 {
        return false;
    }

    // Skip questions
    if text.ends_with('?') {
        return false;
    }

    let lower = text.to_lowercase();
    let first_word = lower.split_whitespace().next().unwrap_or("");

    // Skip if it starts with a non-action verb
    if NON_ACTION_VERBS.contains(&first_word) {
        return false;
    }

    // Skip if it starts with a non-action phrase
    if NON_ACTION_VERBS
        .iter()
        .any(|phrase| lower.starts_with(phrase))
    {
        return false;
    }

    // Common action verbs
    let action_verbs = [
        "implement",
        "add",
        "fix",
        "remove",
        "update",
        "create",
        "delete",
        "write",
        "test",
        "refactor",
        "optimize",
        "check",
        "verify",
        "ensure",
        "install",
        "configure",
        "setup",
        "deploy",
        "build",
        "run",
        "execute",
        "review",
        "document",
        "improve",
        "enhance",
        "migrate",
        "convert",
        "extract",
        "parse",
        "validate",
        "handle",
        "process",
        "generate",
        "make",
        "change",
        "replace",
        "rename",
        "move",
        "copy",
    ];

    // Check if starts with action verb
    if action_verbs.contains(&first_word) {
        return true;
    }

    // Check for "to" + verb constructions
    if let Some(rest) = lower.strip_prefix("to ") {
        if let Some(verb) = rest.split_whitespace().next() {
            return action_verbs.contains(&verb);
        }
    }

    false
}

/// Capitalize the first letter of a string
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_todos_from_bullets() {
        let text = r#"
Here's what we need to do:
- Fix the bug in main.rs
- Add tests for auth
- Update the docs

Let me know if you need help."#;

        let todos = extract_todos(text);
        assert_eq!(todos.len(), 3);
        assert!(todos.iter().any(|t| t.contains("Fix the bug")));
        assert!(todos.iter().any(|t| t.contains("Add tests")));
    }

    #[test]
    fn test_extract_tasks_from_plans() {
        let text = r#"
I will implement the login feature using JWT tokens. This will involve creating a new auth module.

Let's add error handling to the API endpoints.

I need to refactor the database layer."#;

        let tasks = extract_tasks(text);
        assert!(tasks.len() >= 2);
        assert!(tasks
            .iter()
            .any(|t| t.contains("implement") || t.contains("login")));
        assert!(tasks
            .iter()
            .any(|t| t.contains("error handling") || t.contains("refactor")));
    }

    #[test]
    fn test_extract_todos_from_grep() {
        let output = r#"
src/main.rs:42: // TODO: Add error handling
src/auth.rs:15: // FIXME: This is a security risk
src/utils.rs:78: fn helper() {"#;

        let todos = extract_todos_from_tool_result("Grep", output);
        assert!(todos.len() >= 2);
        assert!(todos
            .iter()
            .any(|t| t.contains("error handling") || t.contains("security risk")));
    }

    #[test]
    fn test_is_actionable_phrase() {
        assert!(is_actionable_phrase("Implement the feature"));
        assert!(is_actionable_phrase("Fix the bug"));
        assert!(is_actionable_phrase("Add error handling"));
        assert!(is_actionable_phrase("to add error handling"));
        assert!(!is_actionable_phrase("This is a description"));
        assert!(!is_actionable_phrase("The code does X"));
    }

    #[test]
    fn test_no_duplicate_todos() {
        let mut tasks = WorkspaceTasks {
            tasks: vec![],
            todos: vec![create_todo("Fix the bug".to_string())],
            active_agents: vec![],
        };

        let text = "- Fix the bug\n- Add tests";
        extract_action_items(text, &mut tasks);

        // "Fix the bug" already exists, so only "Add tests" should be added
        assert!(!tasks.todos.is_empty());
        assert!(
            tasks.todos.iter().any(|t| t.text == "Add tests"),
            "Should have added 'Add tests'"
        );
        // Verify no duplicate
        assert_eq!(
            tasks
                .todos
                .iter()
                .filter(|t| t.text == "Fix the bug")
                .count(),
            1,
            "Should not have duplicate 'Fix the bug'"
        );
    }

    #[test]
    fn test_no_false_positives_non_action_verbs() {
        let text = r#"
I will think about this more.
I'll consider the options.
I need to look at the documentation first.
I should see what's possible.
I'll implement the feature after reviewing.
Let's add error handling.
Let's consider another approach.
- consider adding a logo
- discuss the next version?
- Fix the bug"#;

        let tasks = extract_tasks(text);
        let todos = extract_todos(text);

        // Should NOT extract "think about", "consider", "look at", "see" from tasks
        assert!(!tasks.iter().any(|t| t.to_lowercase().contains("think")));
        assert!(!tasks.iter().any(|t| t.to_lowercase().contains("consider")));
        assert!(!tasks.iter().any(|t| t.to_lowercase().contains("look at")));

        // Should NOT extract "consider", "discuss" from todos
        assert!(!todos.iter().any(|t| t.to_lowercase().contains("consider")));
        assert!(!todos.iter().any(|t| t.to_lowercase().contains("discuss")));

        // Should extract the actionable statements
        assert!(tasks
            .iter()
            .any(|t| t.contains("Implement") || t.contains("Add error handling")));
        assert!(todos.iter().any(|t| t.contains("Fix the bug")));
    }
}
