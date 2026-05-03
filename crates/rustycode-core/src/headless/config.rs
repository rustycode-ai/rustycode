/// Maximum number of lines of assistant text to accumulate before checking
/// for repetition. After this threshold, we check and potentially truncate.
pub(crate) const REPETITION_CHECK_THRESHOLD: usize = 200;

/// Maximum number of tool-use turns before we break to prevent infinite loops.
/// Most successful tasks complete in 8-15 turns. 25 provides ample room
/// while preventing runaway sessions that waste time and tokens.
pub(crate) const MAX_TOOL_TURNS: usize = 25;

/// Maximum number of tool calls per single turn before we force-break.
pub(crate) const MAX_TOOLS_PER_TURN: usize = 20;

/// Maximum consecutive similar tool calls before we force-break.
/// "Similar" means same tool name + first 80 chars of arguments match
/// (after normalizing `&&` vs `;` separators).
pub(crate) const MAX_SIMILAR_CONSECUTIVE: usize = 3;

/// Timeout for each individual stream chunk (prevents hangs).
pub(crate) const CHUNK_TIMEOUT: std::time::Duration = std::time::Duration::from_mins(2);

/// Maximum tool result size sent back to the model (in bytes).
/// Tool outputs longer than this are truncated to the last N bytes (which
/// usually contain the actual error/result). This prevents build logs and
/// verbose test output from consuming the entire context window.
pub(crate) const MAX_TOOL_RESULT_BYTES: usize = 8_000;

/// Maximum number of messages before trimming kicks in.
/// When exceeded, older tool-result messages are summarized to reclaim context.
/// Most models have 128k+ token context; ~50 messages with truncated outputs
/// typically stays well within limits.
pub(crate) const MAX_MESSAGES_BEFORE_TRIM: usize = 40;

/// When trimming, keep this many recent messages intact (including the task
/// description and the most recent turns).
pub(crate) const MIN_RECENT_MESSAGES_TO_KEEP: usize = 10;

/// Maximum retries for transient LLM stream errors (rate limit, server error).
pub(crate) const MAX_STREAM_RETRIES: usize = 3;

/// Delay between stream retries (milliseconds), doubles each attempt.
pub(crate) const INITIAL_RETRY_DELAY_MS: u64 = 1_000;

/// Default system prompt for headless coding agent mode.
///
/// Deliberately concise - trusts the model to reason about verification,
/// planning, and error recovery without exhaustive rules. The best agents are
/// guided, not micromanaged.
pub(crate) const HEADLESS_SYSTEM_PROMPT: &str = "\
You are an expert coding agent. Your job: read the task, implement the solution, verify it works.

## Core Loop
Every turn you MUST make at least one tool call. No exceptions.
- If you don't know what to do: read files, grep, list_dir.
- If you know what to do: write_file, edit_file, or bash.
- If something failed: read the error output, then fix it.
- If verification passes: end your turn. Do not re-verify.

## Before Implementing
1. Extract the exact success criteria from the task (what must be true when you're done?)
2. Read relevant files to understand the codebase
3. Plan your approach, then execute

## Verification
After making changes, ALWAYS verify before stopping:
- Run tests if they exist. If tests fail, fix the code (not the tests).
- For builds: verify the binary/artifact was actually created, not just that compilation succeeded.
- For packages: verify importable from outside the source directory.
- For output files: check they exist and have the expected content.

## Stopping
Only end your turn when ALL success criteria are verified. \"Task completed\" means \
you wrote code AND verified it works - not that you read files or explored the codebase.

## Tools (tiered access)
Default: read_file, write_file, edit_file, bash, grep, glob — always available.
Extended: web_fetch, lsp_*, git_*, notebook_edit — activated when needed.
Use lsp_diagnostics before editing to check current error state.
Use semantic_search for concept-level queries; grep for exact patterns.

### Tool tips
- `edit_file`: old_string must match EXACTLY. If it fails, re-read the file first.
- `bash` with `cat > file << 'EOF'`: preferred for files over ~50 lines.
- `bash` timeout_secs: set to 300 for builds/installs (default 120s, max 600s).
- `grep`/`glob`: use before reading large files to find relevant sections.
- `read_file` supports `offset` and `limit` for reading specific line ranges.";

/// System prompt for retry iterations where the previous attempt only read files.
/// Demands immediate action - no more exploration.
pub(crate) const RETRY_SYSTEM_PROMPT: &str = "\
You are an expert coding agent. A previous attempt FAILED - it only read/explored files \
without making changes. This is a RETRY.

Your FIRST tool call MUST be write_file or edit_file. Do not read files, explore, or plan - \
you already have context from the previous attempt. Write the solution now, then verify it works.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_headless_prompt_includes_tier_guidance() {
        assert!(
            HEADLESS_SYSTEM_PROMPT.contains("Extended"),
            "system prompt must mention Extended tool tier"
        );
        assert!(
            HEADLESS_SYSTEM_PROMPT.contains("tiered access"),
            "system prompt must describe tiered access model"
        );
        assert!(
            HEADLESS_SYSTEM_PROMPT.contains("lsp_diagnostics"),
            "system prompt must mention lsp_diagnostics"
        );
        assert!(
            HEADLESS_SYSTEM_PROMPT.contains("semantic_search"),
            "system prompt must mention semantic_search"
        );
    }
}
