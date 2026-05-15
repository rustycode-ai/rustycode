//! System prompt construction for the TUI agent session.
//!
//! Separated from response.rs so prompt logic is testable in isolation
//! and response.rs stays focused on streaming orchestration.

use std::path::Path;

/// Build the full system prompt for a TUI agent session.
///
/// Combines identity + workflow rules, workspace context, platform info,
/// agent mode suffix, orchestration guidance, project-local files, and
/// memory instructions into a single system message string.
pub async fn build_system_prompt(
    cwd: &Path,
    workspace_context: Option<&str>,
    agent_mode: Option<&crate::services::agent_mode::AgentMode>,
    orchestration_guidance: Option<&str>,
    phase_context: Option<&str>,
) -> String {
    let mut parts = vec![
        // ── Identity + Workflow ──
        "You are RustyCode, an AI coding assistant.\n\
        \n\
        Output complete working code. No placeholders, no TODOs, no explanations of what you would do.\n\
        \n\
        ## Workflow\n\
        \n\
        For bug fixes and small changes:\n\
        1. Read the error or failing test output (1 turn)\n\
        2. Locate the relevant code with grep/read (1-2 turns, batch independent reads)\n\
        3. Edit the file with the minimal fix (1 turn)\n\
        4. Verify by running the relevant test (1 turn)\n\
        Target: 4-6 turns. If you understand the bug after reading the error, skip step 2 and fix directly.\n\
        \n\
        For complex tasks (multi-file, refactors, new features):\n\
        1. Scope: read the key files to understand the change surface (2-3 turns)\n\
        2. Plan: write a numbered list of specific edits needed — state what files change and how (1 turn)\n\
        3. Execute: make edits one by one, verifying after each (N turns)\n\
        4. Verify: run build/test/lint on the full change (1 turn)\n\
        For tasks with 3+ steps, use TodoWrite to track progress. Mark each step done as you finish.\n\
        After each step: which step am I on? How many remain? Does the next step still make sense?\n\
        \n\
        ## Self-check\n\
        \n\
        Before each turn, ask:\n\
        - Am I closer to done than last turn? If not, stop exploring and act now.\n\
        - Am I going backwards? (re-reading files, re-running searches already done) If yes, make your best edit instead.\n\
        - Do I understand enough to edit right now? If yes, edit — don't read more for confirmation.\n\
        - If the same approach fails twice → switch strategy entirely\n\
        - If tests fail after editing → re-read the error output, don't guess at a fix\n\
        - Before saying 'done' → verify: did I run the tests? did they pass?\n\
        \n\
        ## Decision shortcuts\n\
        \n\
        - Confident in the fix? Edit now. Don't read more files for reassurance.\n\
        - Past halfway through your turn budget with no edit? Stop reading, make your best fix.\n\
        - Made an edit and tests pass? You're done. Don't look for more things to change.\n\
        - Error message clearly shows the problem? Fix it directly — skip exploration.\n\
        \n\
        ## Rules\n\
        \n\
        - Read files before modifying them\n\
        - Make targeted changes, not broad refactors\n\
        - Run tests to verify your changes\n\
        - Use parallel tool calls when operations are independent\n\
        - After making changes, always verify (build/test/lint) before declaring success\n\
        - If repeating the same failed approach, switch strategy rather than retrying\n\
        \n\
        ## Anti-patterns\n\
        \n\
        - Writing reproduction scripts when error output is already available\n\
        - Reading files unrelated to the task out of curiosity\n\
        - Re-reading a file you already have in context\n\
        - Exploring for more than 3 turns without making an edit\n\
        - Writing test scripts to verify when you can just run the existing tests\n\
        - Continuing to edit after tests pass — ship what works\n\
        \n\
        ## Before saying 'done'\n\
        \n\
        - Run the specific failing test — does it pass now?\n\
        - Run the full test suite — no regressions?\n\
        - Check for import errors or syntax issues\n\
        - Does the fix address the root cause, not just the symptom?\n\
        \n\
        ## When stuck\n\
        \n\
        - Same approach failing 5+ turns → read different files, check git blame, look at tests for API contracts, or simplify the fix"
            .to_string(),
        // ── Workspace ──
        workspace_context
            .map(|ctx| format!("## Project\n{}", ctx))
            .unwrap_or_else(|| "No workspace context available.".to_string()),
        // ── Platform ──
        format!(
            "Platform: {} | Date: {}",
            std::env::consts::OS,
            chrono::Utc::now().format("%Y-%m-%d")
        ),
        // ── Planning mode policy ──
        "Planning mode policy:\n\
        - If a requested action is blocked by planning mode, say you are stalled, name the blocker briefly, and ask the user to switch to implementation mode with /plan.\n\
        - If a required instruction file is missing or empty, say so explicitly and stop.\n\
        - If planning appears complete, say you are ready to switch to implementation mode and wait for the user's confirmation.\n\
        - Do not silently stop after a blocker; explain the next step."
            .to_string(),
        // ── Orchestration tier guidance ──
        "Orchestration tier guidance:\n\
        - For simple tasks (reading files, listing, searching): proceed directly with available tools.\n\
        - For complex tasks (refactoring, multi-file changes, debugging): break the task into steps, verify each step, and escalate if stuck.\n\
        - If you detect you are repeating the same failed approach, switch strategy rather than retrying.\n\
        - After making changes, always verify (build/test/lint) before declaring success."
            .to_string(),
    ];

    // ── Agent mode suffix ──
    if let Some(mode) = agent_mode {
        parts.push(mode.system_prompt_suffix().to_string());
    }

    // ── Orchestration guidance ──
    if let Some(guidance) = orchestration_guidance {
        parts.push(guidance.to_string());
    }

    // ── Phase context ──
    if let Some(ctx) = phase_context {
        parts.push(format!("Previous orchestration context:\n{}", ctx));
    }

    // ── Custom prompts from env ──
    if let Ok(custom_prompt) = std::env::var("RUSTYCODE_SYSTEM_PROMPT") {
        if !custom_prompt.is_empty() {
            parts.push(custom_prompt);
        }
    } else if let Ok(prompt_file) = std::env::var("RUSTYCODE_SYSTEM_PROMPT_FILE") {
        if !prompt_file.is_empty() {
            if let Ok(content) = tokio::fs::read_to_string(&prompt_file).await {
                if !content.trim().is_empty() {
                    parts.push(content);
                }
            }
        }
    }

    // ── Project-local files ──
    if let Some(cwd_str) = cwd.to_str() {
        let project_prompt = Path::new(cwd_str).join(".rustycode_system_prompt");
        if tokio::fs::metadata(&project_prompt).await.is_ok() {
            if let Ok(content) = tokio::fs::read_to_string(&project_prompt).await {
                if !content.trim().is_empty() {
                    parts.push(content);
                }
            }
        }

        let agents_md = Path::new(cwd_str).join("AGENTS.md");
        if tokio::fs::metadata(&agents_md).await.is_ok() {
            if let Ok(content) = tokio::fs::read_to_string(&agents_md).await {
                if !content.trim().is_empty() {
                    parts.push(format!("## Project Instructions (AGENTS.md)\n{}", content));
                }
            }
        }

        // Inject memory instructions (project-scoped)
        let mem_dir = rustycode_memory::memory_dir(Path::new(cwd_str));
        if let Some(mem_instructions) =
            rustycode_memory::read_path::build_memory_instructions(&mem_dir)
        {
            parts.push(mem_instructions);
        }
    }

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn system_prompt_contains_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = build_system_prompt(tmp.path(), None, None, None, None).await;
        assert!(prompt.contains("RustyCode"));
        assert!(prompt.contains("## Workflow"));
        assert!(prompt.contains("## Self-check"));
        assert!(prompt.contains("## Rules"));
        assert!(prompt.contains("## Anti-patterns"));
        assert!(prompt.contains("## Decision shortcuts"));
        assert!(prompt.contains("Platform:"));
        assert!(prompt.contains("TodoWrite"));
        assert!(prompt.contains("going backwards"));
        assert!(prompt.contains("Confident in the fix"));
        assert!(prompt.contains("## Before saying 'done'"));
        assert!(prompt.contains("## When stuck"));
        assert!(prompt.contains("which step am I on"));
        assert!(prompt.contains("root cause"));
    }

    #[tokio::test]
    async fn system_prompt_includes_workspace_context() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt =
            build_system_prompt(tmp.path(), Some("A Rust TUI project"), None, None, None).await;
        assert!(prompt.contains("## Project"));
        assert!(prompt.contains("A Rust TUI project"));
    }

    #[tokio::test]
    async fn system_prompt_reads_agents_md() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::write(tmp.path().join("AGENTS.md"), "Use Rust 2021 edition.")
            .await
            .unwrap();
        let prompt = build_system_prompt(tmp.path(), None, None, None, None).await;
        assert!(prompt.contains("Project Instructions (AGENTS.md)"));
        assert!(prompt.contains("Use Rust 2021 edition."));
    }
}
