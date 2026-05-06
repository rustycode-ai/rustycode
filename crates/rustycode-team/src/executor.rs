//! TeamExecutor -- bridges LLM calls to team agent actions.
//!
//! This is the answer to "how shall LLM direct agents to do anything?"
//!
//! Each team role gets:
//! 1. A role-specific system prompt constraining its behavior
//! 2. A filtered briefing (RoleBriefing) with the right context slice
//! 3. Only the tools appropriate for its role
//! 4. LLM response parsed into a structured turn (BuilderTurn/SkepticTurn/JudgeTurn)
//!
//! The executor runs the Builder->Skeptic->Judge cycle per plan step:
//!
//! ```text
//! PlanManager.current_step()
//!     |
//!     +-- Builder: system_prompt + briefing + write/bash tools -> BuilderTurn
//!     |
//!     +-- Skeptic: system_prompt + briefing + read/grep tools -> SkepticTurn
//!     |
//!     +-- Judge: system_prompt + briefing + bash tools -> JudgeTurn
//!     |
//!     +-- Feed results -> Coordinator (trust) + PlanManager (step progress)
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rustycode_llm::provider::ChatMessage;
use rustycode_protocol::team::*;
use tracing::warn;

use super::coordinator::{BuilderAction, SkepticReview, SkepticVerdict as CoordVerdict, TurnInput};

// Role-specific system prompts

/// System prompts per a sub-module so `format!` strings with `#` don't confuse the parser.
pub(crate) mod prompts {
    use rustycode_protocol::team::StructuralDeclaration;
    pub fn builder_system_prompt(step_context: &str) -> String {
        let mut prompt = String::new();
        prompt.push_str("You are the **Builder** agent in a multi-agent coding team.\n\n");
        prompt.push_str("## Your Job\n");
        prompt.push_str("Implement the current plan step by writing code changes.\n\n");
        prompt.push_str("## Tools\n");
        prompt.push_str(
            "You have tools available: read_file, write_file, bash, grep, glob. USE THEM.\n",
        );
        prompt.push_str("Do NOT just describe what you would do — actually read files, write files, and run commands.\n\n");
        prompt.push_str("## Workflow\n");
        prompt.push_str("1. read_file existing code to understand the current state\n");
        prompt.push_str("2. write_file to create or modify files\n");
        prompt.push_str("3. bash to run build/test commands and verify your changes\n");
        prompt.push_str("4. Iterate: if tests fail, read errors, fix, and re-run\n");
        prompt.push_str(
            "5. When done, respond with the Final Output Format below (no more tool calls)\n\n",
        );
        prompt.push_str("## Current Step Context\n");
        prompt.push_str(step_context);
        prompt.push_str("\n\n");
        prompt.push_str("## Final Output Format\n");
        prompt.push_str(
            "When you have finished all tool calls and your code is working, respond with ONLY\n",
        );
        prompt.push_str("a JSON object matching this schema (no more tool calls after this):\n");
        prompt.push_str("```json\n");
        prompt.push_str("{\n");
        prompt.push_str("  \"approach\": \"one-line description\",\n");
        prompt.push_str(
            "  \"changes\": [{\"path\": \"src/file.rs\", \"summary\": \"what changed\"}],\n",
        );
        prompt.push_str("  \"claims\": [\"what you accomplished\"],\n");
        prompt.push_str("  \"confidence\": 0.85,\n");
        prompt.push_str("  \"done\": false\n");
        prompt.push_str("}\n");
        prompt.push_str("```\n\n");
        prompt.push_str("## Rules\n");
        prompt.push_str("- Make minimal changes for the current step only\n");
        prompt.push_str("- Include error handling for anything that can fail\n");
        prompt.push_str("- Claims must be verifiable — run tests to verify before claiming done\n");
        prompt.push_str("- Set done: true ONLY when ALL plan steps are complete\n");
        prompt
            .push_str("- ACTUALLY USE the tools. Write files. Run commands. Do not just plan.\n\n");
        prompt.push_str("## Tool Call Format (CRITICAL)\n");
        prompt.push_str("Every tool call MUST have valid, non-null string parameters:\n");
        prompt
            .push_str("- bash: {\"command\": \"your shell command here\"} — NEVER null or empty\n");
        prompt.push_str("- write_file: {\"path\": \"file.js\", \"content\": \"...\"} — path and content required\n");
        prompt.push_str("- read_file: {\"path\": \"file.js\"} — path required\n");
        prompt.push_str("- grep: {\"pattern\": \"regex\", \"path\": \"dir\"} — pattern required\n");
        prompt
            .push_str("- Do NOT use python/perl/ruby -e one-liners. Use bash commands directly.\n");
        prompt.push_str("- Do NOT use commands with control characters or shell escapes.\n");
        prompt
    }

    pub fn skeptic_system_prompt(step_context: &str) -> String {
        let mut prompt = String::new();
        prompt.push_str("You are the **Skeptic** agent in a multi-agent coding team.\n\n");
        prompt.push_str("## Your Job\n");
        prompt.push_str("Review the Builder's changes. Verify claims against actual code.\n");
        prompt.push_str(
            "Check for bugs, missing error handling, security issues, hallucinations.\n\n",
        );
        prompt.push_str("## Tools\n");
        prompt.push_str(
            "You have tools: read_file, grep, glob. Use them to actually read the files\n",
        );
        prompt.push_str("the Builder modified. Do NOT approve claims you haven't verified.\n\n");
        prompt.push_str("## Workflow\n");
        prompt.push_str("1. Read each file the Builder claims to have modified\n");
        prompt.push_str("2. Verify each claim against the actual code\n");
        prompt.push_str("3. Check for bugs, security issues, missing error handling\n");
        prompt.push_str("4. Respond with your verdict\n\n");
        prompt.push_str("## Current Step Context\n");
        prompt.push_str(step_context);
        prompt.push_str("\n\n");
        prompt.push_str("## Final Output Format\n");
        prompt.push_str("Respond with a JSON object (no more tool calls):\n");
        prompt.push_str("{\"verdict\": \"approve|needs_work|veto\", ");
        prompt.push_str("\"verified\": [...], \"refuted\": [...], \"insights\": [...]}\n\n");
        prompt.push_str("## Rules\n");
        prompt.push_str("- approve: All claims verified, code is correct\n");
        prompt.push_str("- needs_work: Some claims wrong or code has issues\n");
        prompt.push_str("- veto: Hallucination or critical security bug\n");
        prompt.push_str("- ALWAYS read the actual files before approving\n");
        prompt
    }

    pub fn skeptic_system_prompt_with_declaration(
        step_context: &str,
        declaration: Option<&StructuralDeclaration>,
    ) -> String {
        let mut prompt = skeptic_system_prompt(step_context);

        if let Some(decl) = declaration {
            prompt.push_str("\n\n## Structural Compliance Check\n");
            prompt.push_str(
                "The Architect locked this structural declaration before implementation.\n",
            );
            prompt.push_str("Verify Builder only touched declared modules:\n\n");
            prompt.push_str("**Declared modules:**\n");
            for m in &decl.modules {
                prompt.push_str(&format!("- `{}` ({:?}): {}\n", m.path, m.action, m.purpose));
            }
            if !decl.interfaces.is_empty() {
                prompt.push_str("\n**Declared interfaces:**\n");
                for i in &decl.interfaces {
                    prompt.push_str(&format!("- `{}` defined in `{}`\n", i.name, i.defined_in));
                }
            }
            prompt.push_str("\n**Veto if:** Builder modified files not in the declared list,");
            prompt.push_str(" added undeclared dependencies, or changed interface signatures.\n");
        }

        prompt
    }

    pub fn judge_system_prompt(step_context: &str) -> String {
        let mut prompt = String::new();
        prompt.push_str("You are the **Judge** agent in a multi-agent coding team.\n\n");
        prompt.push_str("## Your Job\n");
        prompt.push_str("Verify changes by running compilation and tests. Report only facts.\n\n");
        prompt.push_str("## Tools\n");
        prompt
            .push_str("You have tools: bash, read_file. Use bash to run build/test commands.\n\n");
        prompt.push_str("## Workflow\n");
        prompt.push_str("1. Run `cargo check` (or equivalent build command) via bash\n");
        prompt.push_str("2. Run `cargo test` (or equivalent test command) via bash\n");
        prompt.push_str("3. Report results as facts\n\n");
        prompt.push_str("## Current Step Context\n");
        prompt.push_str(step_context);
        prompt.push_str("\n\n");
        prompt.push_str("## Final Output Format\n");
        prompt.push_str("Respond with JSON (no more tool calls):\n");
        prompt.push_str("{\"compiles\": bool, \"tests\": TestSummary, ");
        prompt.push_str("\"dirty_files\": [...], \"compile_errors\": [...]}\n\n");
        prompt.push_str("## Rules\n");
        prompt.push_str("- Run actual commands, do not guess\n");
        prompt.push_str("- Report facts, not opinions\n");
        prompt
    }

    pub fn coordinator_system_prompt() -> String {
        "You are the **Coordinator** agent overseeing the Builder-Skeptic-Judge loop.\n\nYou do NOT write code. You manage the team.".to_string()
    }

    pub fn scalpel_system_prompt(step_context: &str) -> String {
        let mut prompt = String::new();
        prompt.push_str("You are the **Scalpel** agent in a multi-agent coding team.\n\n");
        prompt.push_str("## Your Job\n");
        prompt.push_str(
            "Make targeted, surgical fixes for specific failures reported by the Judge.\n",
        );
        prompt.push_str(
            "You do NOT redesign or refactor — you make minimal fixes exactly where broken.\n\n",
        );
        prompt.push_str("## Tools\n");
        prompt.push_str("You have tools: read_file, write_file, bash. USE THEM.\n");
        prompt.push_str("Read the broken file, fix it, verify it compiles.\n\n");
        prompt.push_str("## Workflow\n");
        prompt.push_str("1. read_file the file with the error\n");
        prompt.push_str("2. write_file to apply the fix\n");
        prompt.push_str("3. bash to verify the fix compiles\n");
        prompt.push_str("4. If still broken, iterate (max 3 attempts)\n");
        prompt.push_str("5. Respond with Final Output Format when done\n\n");
        prompt.push_str("## Current Step Context\n");
        prompt.push_str(step_context);
        prompt.push_str("\n\n");
        prompt.push_str("## Final Output Format\n");
        prompt.push_str("Respond with a JSON object (no more tool calls):\n");
        prompt.push_str("```json\n");
        prompt.push_str("{\n");
        prompt.push_str("  \"approach\": \"one-line description of the fix\",\n");
        prompt.push_str(
            "  \"changes\": [{\"path\": \"src/file.rs\", \"summary\": \"what was fixed\"}],\n",
        );
        prompt.push_str("  \"claims\": [\"what you fixed\"],\n");
        prompt.push_str("  \"confidence\": 0.9,\n");
        prompt.push_str("  \"done\": true\n");
        prompt.push_str("}\n");
        prompt.push_str("```\n\n");
        prompt.push_str("## Rules\n");
        prompt.push_str("- Fix ONLY the reported failure — no scope creep\n");
        prompt.push_str("- Make one precise edit per failure, maximum 5 lines changed per file\n");
        prompt.push_str("- Never introduce new dependencies\n");
        prompt.push_str("- Always set done: true (you complete your fix in one turn)\n");
        prompt.push_str("- Verify your fix compiles before responding\n");
        prompt
    }

    pub fn architect_system_prompt(step_context: &str) -> String {
        let mut prompt = String::new();
        prompt.push_str("You are the **Architect** agent in a multi-agent coding team.\n\n");
        prompt.push_str("## Your Job\n");
        prompt
            .push_str("Analyze the codebase and produce a StructuralDeclaration that constrains\n");
        prompt.push_str(
            "all subsequent Builder work. You have READ-ONLY access — you never write code.\n\n",
        );
        prompt.push_str("## Tools\n");
        prompt.push_str("You have tools: read_file, grep, glob, lsp_references, lsp_hover.\n");
        prompt.push_str("Use them to explore the codebase before making declarations.\n\n");
        prompt.push_str("## Workflow\n");
        prompt.push_str("1. glob to discover project structure\n");
        prompt.push_str("2. read_file key files to understand patterns\n");
        prompt.push_str("3. grep for interfaces and dependencies\n");
        prompt.push_str("4. Respond with structural declaration\n\n");
        prompt.push_str("## Current Step Context\n");
        prompt.push_str(step_context);
        prompt.push_str("\n\n");
        prompt.push_str("## Final Output Format\n");
        prompt.push_str("Respond with a JSON object (no more tool calls):\n");
        prompt.push_str("```json\n");
        prompt.push_str("{\n");
        prompt.push_str("  \"declaration\": {\n");
        prompt.push_str("    \"modules\": [{\"path\": \"src/module.rs\", \"action\": \"create\",");
        prompt.push_str(" \"exports\": [], \"imports\": [], \"purpose\": \"\"}],\n");
        prompt
            .push_str("    \"interfaces\": [{\"name\": \"Trait\", \"defined_in\": \"src/lib.rs\",");
        prompt.push_str(" \"methods\": [], \"implementors\": []}],\n");
        prompt.push_str("    \"dependencies\": {\"add\": [], \"remove\": [], \"keep\": []}\n");
        prompt.push_str("  },\n");
        prompt.push_str("  \"rationale\": \"why this structure\",\n");
        prompt.push_str("  \"confidence\": 0.9\n");
        prompt.push_str("}\n");
        prompt.push_str("```\n\n");
        prompt.push_str("## Rules\n");
        prompt.push_str("- Every module must have a clear purpose (one-liner)\n");
        prompt.push_str("- Every dependency change must have a reason\n");
        prompt.push_str("- No implementation details — structure only\n");
        prompt.push_str("- The declaration is a binding contract for the Builder\n");
        prompt
    }
}

// Tool sets per role

/// Returns tool names appropriate for each role.
pub fn tools_for_role(role: TeamRole) -> Vec<&'static str> {
    match role {
        TeamRole::Builder => vec!["read_file", "write_file", "bash", "grep", "glob"],
        TeamRole::Scalpel => vec!["read_file", "write_file", "bash"],
        TeamRole::Skeptic => vec!["read_file", "grep", "glob"],
        TeamRole::Architect => vec!["read_file", "grep", "glob", "lsp_references", "lsp_hover"],
        TeamRole::Judge => vec!["bash", "read_file"],
        TeamRole::Coordinator => vec!["read_file", "bash"],
        #[allow(unreachable_patterns)]
        _ => vec![],
    }
}

// Turn parsing — extract structured turns from LLM responses

/// The result of parsing an LLM response into a structured turn.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ParsedTurn {
    Builder(BuilderTurn),
    Skeptic(SkepticTurn),
    Judge(JudgeTurn),
    Scalpel(BuilderTurn), // Scalpel uses same turn structure as Builder, but constrained scope
}

/// Parse a raw LLM text response into a structured turn.
pub fn parse_turn(raw: &str, role: TeamRole) -> Result<ParsedTurn> {
    let json_str = extract_json(raw)?;

    match role {
        TeamRole::Builder => {
            let turn: BuilderTurn =
                serde_json::from_str(&json_str).context("failed to parse BuilderTurn")?;
            Ok(ParsedTurn::Builder(turn))
        }
        TeamRole::Skeptic => {
            let turn: SkepticTurn =
                serde_json::from_str(&json_str).context("failed to parse SkepticTurn")?;
            Ok(ParsedTurn::Skeptic(turn))
        }
        TeamRole::Judge => {
            let turn: JudgeTurn =
                serde_json::from_str(&json_str).context("failed to parse JudgeTurn")?;
            Ok(ParsedTurn::Judge(turn))
        }
        TeamRole::Coordinator | TeamRole::Architect => Err(anyhow::anyhow!(
            "{} does not produce structured turns",
            role
        )),
        TeamRole::Scalpel => {
            // Scalpel produces a targeted fix turn (same structure as Builder but smaller scope)
            let turn: BuilderTurn =
                serde_json::from_str(&json_str).context("failed to parse ScalpelTurn")?;
            Ok(ParsedTurn::Scalpel(turn))
        }
        #[allow(unreachable_patterns)]
        _ => Err(anyhow::anyhow!(
            "unsupported role for turn parsing: {}",
            role
        )),
    }
}

/// Parse the Architect's JSON output into a structured ArchitectTurn.
/// Strips markdown fences if present (LLMs often wrap JSON in ```json blocks).
///
/// If the LLM returns valid JSON that doesn't conform to the ArchitectTurn schema,
/// we attempt a lenient parse: extract whatever fields we can and fill defaults
/// for the rest, rather than failing outright.
pub fn parse_architect_turn(raw: &str) -> Result<ArchitectTurn> {
    let cleaned = match extract_json(raw) {
        Ok(json) => json,
        Err(e) => {
            // If we can't extract any JSON at all, synthesize a minimal declaration
            // from the raw text so the pipeline doesn't stall.
            warn!(
                "Could not extract JSON from Architect response, synthesizing default: {}",
                e
            );
            return Ok(ArchitectTurn {
                declaration: StructuralDeclaration::default(),
                rationale: raw.chars().take(500).collect(),
                confidence: 0.3,
            });
        }
    };

    // Try strict parse first
    if let Ok(turn) = serde_json::from_str::<ArchitectTurn>(&cleaned) {
        return Ok(turn);
    }

    // Lenient fallback: try to parse as generic JSON and extract what we can
    warn!("Strict ArchitectTurn parse failed, attempting lenient parse");
    let value: serde_json::Value = serde_json::from_str(&cleaned)
        .context("Lenient parse also failed — response is not valid JSON")?;

    Ok(ArchitectTurn {
        declaration: parse_declaration_lenient(&value),
        rationale: value
            .get("rationale")
            .and_then(|v| v.as_str())
            .unwrap_or("Auto-generated from non-standard Architect response")
            .to_string(),
        confidence: value
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5),
    })
}

/// Leniently extract a StructuralDeclaration from a JSON value.
/// Tries the `declaration` field first, then falls back to top-level fields.
fn parse_declaration_lenient(value: &serde_json::Value) -> StructuralDeclaration {
    let decl_value = value.get("declaration").unwrap_or(value);

    let modules = decl_value
        .get("modules")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(parse_module_declaration).collect())
        .unwrap_or_default();

    let interfaces = decl_value
        .get("interfaces")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|i| serde_json::from_value(i.clone()).ok())
                .collect()
        })
        .unwrap_or_default();

    let dependencies = decl_value
        .get("dependencies")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    StructuralDeclaration {
        modules,
        interfaces,
        dependencies,
    }
}

/// Parse a single ModuleDeclaration leniently — fill defaults for missing fields.
fn parse_module_declaration(value: &serde_json::Value) -> Option<ModuleDeclaration> {
    let path = value.get("path")?.as_str()?.to_string();
    if path.is_empty() {
        return None;
    }

    let action = value
        .get("action")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(ModuleAction::Create);

    Some(ModuleDeclaration {
        path,
        action,
        exports: value
            .get("exports")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        imports: value
            .get("imports")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        purpose: value
            .get("purpose")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// Parse the Scalpel's JSON output into a structured ScalpelTurn.
pub fn parse_scalpel_turn(raw: &str) -> Result<ScalpelTurn> {
    let cleaned = extract_json(raw)?;
    serde_json::from_str(&cleaned).context("Failed to parse ScalpelTurn from LLM output")
}

/// Extract JSON from a raw LLM response.
///
/// Handles:
/// - Plain JSON
/// - Markdown code fences
/// - Mixed text + JSON
pub fn extract_json(raw: &str) -> Result<String> {
    let trimmed = raw.trim();

    // Already valid JSON
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
    {
        return Ok(trimmed.to_string());
    }

    // Markdown code fences
    if let Some(json) = extract_from_code_fence(trimmed) {
        if serde_json::from_str::<serde_json::Value>(&json).is_ok() {
            return Ok(json);
        }
    }

    // Find first { ... last }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        let candidate = &trimmed[start..=end];
        if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
            return Ok(candidate.to_string());
        }
    }

    Err(anyhow::anyhow!("could not extract JSON from LLM response"))
}

fn extract_from_code_fence(text: &str) -> Option<String> {
    // Try ```json ... ``` first
    if let Some(start_marker) = text.find("```json") {
        let json_start = start_marker + 7;
        if let Some(end_marker) = text[json_start..].find("```") {
            return Some(text[json_start..json_start + end_marker].trim().to_string());
        }
    }

    // Try ``` ... ```
    if let Some(start_marker) = text.find("```") {
        let content_start = text[start_marker..].find('\n')?;
        let json_start = start_marker + content_start + 1;
        if let Some(end_marker) = text[json_start..].find("```") {
            return Some(text[json_start..json_start + end_marker].trim().to_string());
        }
    }

    None
}

impl ParsedTurn {
    /// Convert to a coordinator-compatible TurnInput.
    pub fn to_turn_input(&self) -> TurnInput {
        match self {
            ParsedTurn::Builder(turn) => TurnInput {
                builder_action: Some(BuilderAction {
                    approach: turn.approach.clone(),
                    files_changed: turn.changes.iter().map(|c| c.path.clone()).collect(),
                    claims_done: turn.done,
                }),
                skeptic_review: None,
                judge_results: None,
            },
            ParsedTurn::Skeptic(turn) => {
                let verdict = match turn.verdict {
                    SkepticVerdict::Approve => CoordVerdict::Approve,
                    SkepticVerdict::NeedsWork => CoordVerdict::RevisionNeeded,
                    SkepticVerdict::Veto => CoordVerdict::Stop,
                    #[allow(unreachable_patterns)]
                    _ => CoordVerdict::RevisionNeeded,
                };
                let issues: Vec<(String, String)> = turn
                    .refuted
                    .iter()
                    .map(|r| (r.evidence.clone(), r.claim.clone()))
                    .collect();
                let hallucination = matches!(turn.verdict, SkepticVerdict::Veto);
                TurnInput {
                    builder_action: None,
                    skeptic_review: Some(SkepticReview {
                        verdict,
                        issues,
                        hallucination_detected: hallucination,
                    }),
                    judge_results: None,
                }
            }
            ParsedTurn::Judge(turn) => TurnInput {
                builder_action: None,
                skeptic_review: None,
                judge_results: Some(VerificationState {
                    compiles: turn.compiles,
                    tests: turn.tests.clone(),
                    dirty_files: turn.dirty_files.clone(),
                }),
            },
            ParsedTurn::Scalpel(turn) => TurnInput {
                builder_action: Some(BuilderAction {
                    approach: turn.approach.clone(),
                    files_changed: turn.changes.iter().map(|c| c.path.clone()).collect(),
                    claims_done: turn.done,
                }),
                skeptic_review: None,
                judge_results: None,
            },
        }
    }
}

// Local capabilities — deterministic operations without LLM calls

pub mod local_capabilities {
    use std::path::Path;

    /// Check if a project compiles.
    /// Returns (success, error_output).
    pub fn check_compilation(project_root: &Path) -> (bool, String) {
        match std::process::Command::new("cargo")
            .args(["check", "--message-format=short"])
            .current_dir(project_root)
            .output()
        {
            Ok(output) => {
                let success = output.status.success();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                (success, stderr)
            }
            Err(e) => (false, format!("failed to run cargo check: {}", e)),
        }
    }

    /// Run tests and return summary.
    /// Returns (passed, failed, total, failed_names).
    pub fn run_tests(project_root: &Path) -> (usize, usize, usize, Vec<String>) {
        match std::process::Command::new("cargo")
            .args(["test", "--", "-Z", "unstable-options", "--format=json"])
            .current_dir(project_root)
            .output()
        {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                parse_test_output(&stdout)
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                (
                    0,
                    0,
                    0,
                    vec![format!("cargo test failed: {}", stderr.trim())],
                )
            }
            Err(e) => (0, 0, 0, vec![format!("failed to run cargo test: {}", e)]),
        }
    }

    /// Quick syntax check for a Rust file.
    pub fn check_syntax(file_path: &Path) -> Result<(), String> {
        match std::process::Command::new("rustc")
            .args(["--edition", "2021", "--emit=metadata", "-o", "/dev/null"])
            .arg(file_path)
            .output()
        {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Err(stderr)
            }
            Err(e) => Err(format!("failed to run rustc: {}", e)),
        }
    }

    /// Check if a file exists.
    pub fn file_info(path: &Path) -> Option<std::fs::Metadata> {
        std::fs::metadata(path).ok()
    }

    /// Grep for a pattern in a file. Returns matching lines (1-indexed).
    pub fn grep_file(path: &Path, pattern: &str) -> Vec<(usize, String)> {
        let Ok(content) = std::fs::read_to_string(path) else {
            return vec![];
        };
        content
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(pattern))
            .map(|(i, line)| (i + 1, line.to_string()))
            .collect()
    }

    fn parse_test_output(stdout: &str) -> (usize, usize, usize, Vec<String>) {
        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut failed_names = Vec::new();

        for line in stdout.lines() {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                if value.get("type").and_then(|t| t.as_str()) == Some("test") {
                    let event = value.get("event").and_then(|e| e.as_str()).unwrap_or("");
                    match event {
                        "ok" => passed += 1,
                        "failed" => {
                            failed += 1;
                            if let Some(name) = value.get("name").and_then(|n| n.as_str()) {
                                failed_names.push(name.to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let total = passed + failed;
        (passed, failed, total, failed_names)
    }
}

// TeamExecutor — the full execution engine

/// Configuration for the executor.
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub max_total_turns: u32,
    pub max_retries_per_step: u32,
    pub max_adaptations: u32,
    pub use_local_checks: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_total_turns: 50,
            max_retries_per_step: 3,
            max_adaptations: 5,
            use_local_checks: true,
        }
    }
}

/// The outcome of a full team execution.
#[derive(Debug, Clone)]
pub struct ExecutionOutcome {
    pub plan: Option<rustycode_protocol::Plan>,
    pub files_modified: Vec<String>,
    pub turns: u32,
    pub final_trust: f64,
    pub success: bool,
    pub message: String,
}

/// Result of local pre-checks.
#[derive(Debug, Clone, Default)]
pub struct PreCheckResult {
    pub compilation_ok: bool,
    pub compilation_errors: Vec<String>,
}

/// Result of local post-checks.
#[derive(Debug, Clone, Default)]
pub struct PostCheckResult {
    pub files_exist: Vec<String>,
    pub files_missing: Vec<String>,
}

/// The full team execution engine.
pub struct TeamExecutor {
    project_root: PathBuf,
}

impl TeamExecutor {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }

    /// Build system prompt for a specific role + step context.
    pub fn system_prompt_for_role(role: TeamRole, step_context: &str) -> String {
        Self::system_prompt_for_role_with_declaration(role, step_context, None)
    }

    /// Build system prompt for a specific role, with optional structural declaration.
    pub fn system_prompt_for_role_with_declaration(
        role: TeamRole,
        step_context: &str,
        declaration: Option<&StructuralDeclaration>,
    ) -> String {
        match role {
            TeamRole::Builder => prompts::builder_system_prompt(step_context),
            TeamRole::Scalpel => prompts::scalpel_system_prompt(step_context),
            TeamRole::Skeptic => {
                prompts::skeptic_system_prompt_with_declaration(step_context, declaration)
            }
            TeamRole::Architect => prompts::architect_system_prompt(step_context),
            TeamRole::Judge => prompts::judge_system_prompt(step_context),
            TeamRole::Coordinator => prompts::coordinator_system_prompt(),
            #[allow(unreachable_patterns)]
            _ => String::new(),
        }
    }

    /// Build the messages array for an LLM call to a specific role.
    pub fn build_messages_for_role(
        briefing: &RoleBriefing,
        step_context: &str,
        previous_turns: &[String],
    ) -> Vec<ChatMessage> {
        Self::build_messages_for_role_with_declaration(briefing, step_context, previous_turns, None)
    }

    /// Build the messages array for an LLM call to a specific role, with optional structural declaration.
    pub fn build_messages_for_role_with_declaration(
        briefing: &RoleBriefing,
        step_context: &str,
        previous_turns: &[String],
        structural_declaration: Option<&StructuralDeclaration>,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        let system_prompt = Self::system_prompt_for_role_with_declaration(
            briefing.role,
            step_context,
            structural_declaration,
        );
        messages.push(ChatMessage::system(system_prompt));

        let briefing_text = Self::format_briefing(briefing);
        messages.push(ChatMessage::user(briefing_text));

        for turn_summary in previous_turns {
            messages.push(ChatMessage::system(turn_summary.clone()));
        }

        messages
    }

    /// Format a RoleBriefing into text for the LLM.
    fn format_briefing(briefing: &RoleBriefing) -> String {
        let mut parts = Vec::new();

        parts.push(format!("## Task\n{}", briefing.task));

        if !briefing.code.is_empty() {
            parts.push("## Code Context".to_string());
            for snippet in &briefing.code {
                parts.push(format!(
                    "### {}\n```\n{}\n```",
                    snippet.path, snippet.content,
                ));
            }
        }

        if !briefing.attempts.is_empty() {
            parts.push("## Previous Attempts".to_string());
            for attempt in &briefing.attempts {
                let outcome_str = match attempt.outcome {
                    AttemptOutcome::Success => "OK",
                    AttemptOutcome::TestFailure => "FAIL",
                    AttemptOutcome::CompilationError => "COMPILE_ERR",
                    AttemptOutcome::Vetoed(_) => "VETOED",
                    AttemptOutcome::WrongApproach => "WRONG",
                    AttemptOutcome::Timeout => "TIMEOUT",
                    #[allow(unreachable_patterns)]
                    _ => "UNKNOWN",
                };
                parts.push(format!(
                    "- [{}] {} - {}",
                    outcome_str, attempt.approach, attempt.root_cause,
                ));
            }
        }

        if !briefing.constraints.is_empty() {
            let constraints: String = briefing
                .constraints
                .iter()
                .map(|c| format!("- {}", c))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("## Constraints\n{}", constraints));
        }

        if !briefing.insights.is_empty() {
            let insights: String = briefing
                .insights
                .iter()
                .map(|i| format!("- {}", i))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("## Insights\n{}", insights));
        }

        if let Some(ref verification) = briefing.verification {
            parts.push(format!(
                "## Current Verification\n- Compiles: {}\n- Tests: {}/{} passed",
                verification.compiles, verification.tests.passed, verification.tests.total,
            ));
            if !verification.tests.failed_names.is_empty() {
                parts.push(format!(
                    "- Failing: {}",
                    verification.tests.failed_names.join(", ")
                ));
            }
        }

        if let Some(ref trust) = briefing.trust_context {
            parts.push(format!(
                "## Trust State\n- Builder trust: {:.2}\n- Turn: {}\n- Degrading: {}",
                trust.builder_trust, trust.turn, trust.degrading,
            ));
        }

        parts.join("\n\n")
    }

    /// Run local pre-checks before an LLM call.
    pub fn run_pre_checks(&self, role: TeamRole) -> PreCheckResult {
        if role == TeamRole::Judge {
            let (compiles, errors) = local_capabilities::check_compilation(&self.project_root);
            return PreCheckResult {
                compilation_ok: compiles,
                compilation_errors: if compiles {
                    vec![]
                } else {
                    errors.lines().take(10).map(String::from).collect()
                },
            };
        }
        PreCheckResult::default()
    }

    /// Run local post-checks after an LLM call.
    pub fn run_post_checks(&self, claimed_files: &[String]) -> PostCheckResult {
        let mut existing = Vec::new();
        let mut missing = Vec::new();

        for file in claimed_files {
            let path = self.project_root.join(file);
            if path.exists() {
                existing.push(file.clone());
            } else {
                missing.push(file.clone());
            }
        }

        PostCheckResult {
            files_exist: existing,
            files_missing: missing,
        }
    }

    /// Get the project root.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_plain() {
        let json = r#"{"approach": "fix bug", "changes": [], "claims": [], "confidence": 0.9, "done": false}"#;
        let result = extract_json(json).unwrap();
        assert!(result.contains("fix bug"));
    }

    #[test]
    fn extract_json_from_fence() {
        let raw = "Here is my response:\n```json\n{\"approach\": \"fix\", \"changes\": [], \"claims\": [], \"confidence\": 0.5, \"done\": false}\n```\nDone!";
        let result = extract_json(raw).unwrap();
        assert!(result.contains("fix"));
    }

    #[test]
    fn extract_json_from_text() {
        let raw = "I made some changes:\n{\"approach\": \"edit auth\", \"changes\": [], \"claims\": [\"fixed it\"], \"confidence\": 0.8, \"done\": true}";
        let result = extract_json(raw).unwrap();
        assert!(result.contains("edit auth"));
    }

    #[test]
    fn parse_builder_turn() {
        let raw = r#"{"approach": "added null check", "changes": [{"path": "src/auth.rs", "summary": "added null check", "diff_hunk": "+ if user.is_none() { return Err(...); }", "lines_added": 1, "lines_removed": 0}], "claims": ["null check added"], "confidence": 0.9, "done": false}"#;
        let parsed = parse_turn(raw, TeamRole::Builder).unwrap();
        match parsed {
            ParsedTurn::Builder(turn) => {
                assert_eq!(turn.approach, "added null check");
                assert_eq!(turn.changes.len(), 1);
                assert!(!turn.done);
            }
            _ => panic!("expected Builder turn"),
        }
    }

    #[test]
    fn parse_skeptic_turn() {
        let raw = r#"{"verdict": "approve", "verified": ["null check is present"], "refuted": [], "insights": []}"#;
        let parsed = parse_turn(raw, TeamRole::Skeptic).unwrap();
        match parsed {
            ParsedTurn::Skeptic(turn) => {
                assert!(matches!(turn.verdict, SkepticVerdict::Approve));
                assert_eq!(turn.verified.len(), 1);
            }
            _ => panic!("expected Skeptic turn"),
        }
    }

    #[test]
    fn parse_judge_turn() {
        let raw = r#"{"compiles": true, "tests": {"total": 10, "passed": 10, "failed": 0, "failed_names": []}, "dirty_files": ["src/auth.rs"], "compile_errors": []}"#;
        let parsed = parse_turn(raw, TeamRole::Judge).unwrap();
        match parsed {
            ParsedTurn::Judge(turn) => {
                assert!(turn.compiles);
                assert_eq!(turn.tests.passed, 10);
                assert_eq!(turn.dirty_files.len(), 1);
            }
            _ => panic!("expected Judge turn"),
        }
    }

    #[test]
    fn builder_turn_to_turn_input() {
        let raw = r#"{"approach": "fix", "changes": [{"path": "src/main.rs", "summary": "fix", "diff_hunk": "", "lines_added": 1, "lines_removed": 0}], "claims": ["fixed"], "confidence": 0.9, "done": true}"#;
        let parsed = parse_turn(raw, TeamRole::Builder).unwrap();
        let input = parsed.to_turn_input();
        assert!(input.builder_action.is_some());
        let action = input.builder_action.unwrap();
        assert!(action.claims_done);
        assert_eq!(action.files_changed, vec!["src/main.rs"]);
    }

    #[test]
    fn skeptic_turn_to_turn_input() {
        let raw = r#"{"verdict": "needs_work", "verified": [], "refuted": [{"claim": "tests pass", "evidence": "2 tests fail"}], "insights": []}"#;
        let parsed = parse_turn(raw, TeamRole::Skeptic).unwrap();
        let input = parsed.to_turn_input();
        assert!(input.skeptic_review.is_some());
        let review = input.skeptic_review.unwrap();
        assert!(matches!(review.verdict, CoordVerdict::RevisionNeeded));
        assert_eq!(review.issues.len(), 1);
    }

    #[test]
    fn tools_for_role_are_appropriate() {
        let builder_tools = tools_for_role(TeamRole::Builder);
        assert!(builder_tools.contains(&"write_file"));
        assert!(builder_tools.contains(&"bash"));

        assert!(builder_tools.contains(&"write_file") || builder_tools.contains(&"read_file"));

        let skeptic_tools = tools_for_role(TeamRole::Skeptic);
        assert!(skeptic_tools.contains(&"read_file"));
        assert!(!skeptic_tools.contains(&"write_file"));

        let judge_tools = tools_for_role(TeamRole::Judge);
        assert!(judge_tools.contains(&"bash"));
        assert!(!judge_tools.contains(&"write_file"));
    }

    #[test]
    fn system_prompts_are_role_specific() {
        let builder = TeamExecutor::system_prompt_for_role(TeamRole::Builder, "step 1");
        assert!(builder.contains("Builder"));
        assert!(builder.contains("JSON"));

        let skeptic = TeamExecutor::system_prompt_for_role(TeamRole::Skeptic, "step 1");
        assert!(skeptic.contains("Skeptic"));

        assert!(skeptic.contains("approve"));

        let judge = TeamExecutor::system_prompt_for_role(TeamRole::Judge, "step 1");
        assert!(judge.contains("Judge"));
        assert!(judge.contains("cargo check"));

        let coord = TeamExecutor::system_prompt_for_role(TeamRole::Coordinator, "");
        assert!(coord.contains("Coordinator"));
    }

    #[test]
    fn post_checks_detect_missing_files() {
        let executor = TeamExecutor::new("/tmp/nonexistent");
        let result = executor.run_post_checks(&["src/fake.rs".to_string()]);
        assert!(result.files_missing.contains(&"src/fake.rs".to_string()));
        assert!(result.files_exist.is_empty());
    }

    #[test]
    fn parse_architect_turn_valid_json() {
        let json = r#"{
            "declaration": {
                "modules": [{"path": "src/x.rs", "action": "Create", "exports": ["X"], "imports": [], "purpose": "test"}],
                "interfaces": [],
                "dependencies": {"add": [], "remove": [], "keep": []}
            },
            "rationale": "simple",
            "confidence": 0.8
        }"#;
        let turn = parse_architect_turn(json).unwrap();
        assert_eq!(turn.declaration.modules.len(), 1);
        assert_eq!(turn.declaration.modules[0].path, "src/x.rs");
        assert!((turn.confidence - 0.8).abs() < 0.01);
    }

    #[test]
    fn parse_scalpel_turn_valid_json() {
        let json = r#"{"fixes": [{"file": "src/lib.rs", "issue": "missing ;", "action": "added ;"}], "done": true}"#;
        let turn = parse_scalpel_turn(json).unwrap();
        assert!(turn.done);
        assert_eq!(turn.fixes.len(), 1);
        assert_eq!(turn.fixes[0].file, "src/lib.rs");
    }

    #[test]
    fn skeptic_prompt_with_declaration_includes_modules() {
        use rustycode_protocol::team::{
            DependencyChanges, ModuleAction, ModuleDeclaration, StructuralDeclaration,
        };

        let decl = StructuralDeclaration {
            modules: vec![ModuleDeclaration {
                path: "src/architect.rs".to_string(),
                action: ModuleAction::Create,
                exports: vec!["ArchitectAgent".to_string()],
                imports: vec![],
                purpose: "Structural analysis".to_string(),
            }],
            interfaces: vec![],
            dependencies: DependencyChanges {
                add: vec![],
                remove: vec![],
                keep: vec![],
            },
        };

        let prompt = prompts::skeptic_system_prompt_with_declaration("step", Some(&decl));
        assert!(prompt.contains("Structural Compliance Check"));
        assert!(prompt.contains("src/architect.rs"));
        assert!(prompt.contains("Structural analysis"));
        assert!(prompt.contains("Veto if:"));
    }

    #[test]
    fn skeptic_prompt_without_declaration_is_standard() {
        let prompt = prompts::skeptic_system_prompt_with_declaration("step", None);
        assert!(prompt.contains("Skeptic"));
        assert!(!prompt.contains("Structural Compliance Check"));
    }

    #[test]
    fn skeptic_prompt_includes_interfaces_when_present() {
        use rustycode_protocol::team::{
            DependencyChanges, InterfaceDeclaration, StructuralDeclaration,
        };

        let decl = StructuralDeclaration {
            modules: vec![],
            interfaces: vec![InterfaceDeclaration {
                name: "Agent".to_string(),
                defined_in: "src/agent.rs".to_string(),
                methods: vec!["async fn execute(&self)".to_string()],
                implementors: vec![],
            }],
            dependencies: DependencyChanges {
                add: vec![],
                remove: vec![],
                keep: vec![],
            },
        };

        let prompt = prompts::skeptic_system_prompt_with_declaration("step", Some(&decl));
        assert!(prompt.contains("Declared interfaces"));
        assert!(prompt.contains("Agent"));
        assert!(prompt.contains("src/agent.rs"));
    }

    #[test]
    fn system_prompt_for_role_with_declaration_routes_correctly() {
        use rustycode_protocol::team::{DependencyChanges, StructuralDeclaration};

        let decl = StructuralDeclaration {
            modules: vec![],
            interfaces: vec![],
            dependencies: DependencyChanges {
                add: vec![],
                remove: vec![],
                keep: vec![],
            },
        };

        // Skeptic should get the enhanced prompt
        let skeptic_prompt = TeamExecutor::system_prompt_for_role_with_declaration(
            TeamRole::Skeptic,
            "step",
            Some(&decl),
        );
        assert!(skeptic_prompt.contains("Structural Compliance Check"));

        // Builder should get the standard prompt (unchanged)
        let builder_prompt = TeamExecutor::system_prompt_for_role_with_declaration(
            TeamRole::Builder,
            "step",
            Some(&decl),
        );
        assert!(!builder_prompt.contains("Structural Compliance Check"));
        assert!(builder_prompt.contains("Builder"));
    }
}
