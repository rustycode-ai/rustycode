//! Hook slash commands
//!
//! Provides commands for managing hooks (pre/post execution automation).

/// Handle hook slash commands
pub fn handle_hook_command(input: &str) -> Result<Option<String>, String> {
    let parts: Vec<&str> = input.split_whitespace().collect();

    if parts.is_empty() || parts.len() < 2 {
        return Ok(Some(hook_help()));
    }

    let subcommand = parts[1];

    match subcommand {
        "help" | "" => Ok(Some(hook_help())),
        "list" => Ok(Some(hook_help())),
        "status" => Ok(Some(
            "Hook Status:\n\
            \n\
            Hooks can be configured to run at various points during tool execution:\n\
            \n\
            • PreToolUse  - Before a tool executes (can modify/deny)\n\
            • PostToolUse - After a tool executes (logging, notifications)\n\
            • PrePrompt   - Before sending to LLM (sanitization)\n\
            • PostResponse - After LLM response (modification)\n\
            • OnError     - On any error (recovery actions)\n\
            \n\
            Configure hooks in ~/.rustycode/hooks/"
                .to_string(),
        )),
        _ => Ok(Some(format!(
            "Unknown hook command: {}\n\n{}",
            subcommand,
            hook_help()
        ))),
    }
}

/// Get hook help text
fn hook_help() -> String {
    "Hooks - Pre/Post Execution Automation\n\
    \n\
    Hooks allow you to run custom code at various points during tool execution.\n\
    \n\
    Commands:\n\
    • /hook list    - Show registered hooks\n\
    • /hook status  - Show hook configuration status\n\
    • /hook add     - Add a new hook (coming soon)\n\
    • /hook remove  - Remove a hook (coming soon)\n\
    • /hook enable  - Enable a hook (coming soon)\n\
    • /hook disable - Disable a hook (coming soon)\n\
    \n\
    Hook Types:\n\
    • PreToolUse    - Before tool execution\n\
    • PostToolUse   - After tool execution\n\
    • PrePrompt     - Before LLM prompt\n\
    • PostResponse  - After LLM response\n\
    • OnError       - On error occurrence\n\
    \n\
    Configuration: Create scripts in ~/.rustycode/hooks/"
        .to_string()
}
