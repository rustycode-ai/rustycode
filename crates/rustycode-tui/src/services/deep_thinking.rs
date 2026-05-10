//! Deep-thinking integration bridge for the TUI.

use rustycode_orchestration::thinking::{
    ActivationSignals, DefaultActivationPolicy, SignalRisk, SignalTier, ThinkingActivationPolicy,
};
use rustycode_tools::providers::decompose::DecomposeProblemTool;
use rustycode_tools::{Tool, ToolContext};

/// Result of the deep-thinking analysis.
#[derive(Debug)]
pub struct DeepThinkingResult {
    /// Whether deep thinking was activated.
    pub activated: bool,
    /// Human-readable reason for the decision.
    pub reason: String,
    /// The (possibly transformed) message content to send to the LLM.
    pub content: String,
}

/// Analyzes a user message and transforms it if deep thinking is warranted.
///
/// This is the main entry point called from the service integration layer
/// before the message is sent to the LLM. It uses heuristic analysis to
/// populate `ActivationSignals`, checks them against the
/// `DefaultActivationPolicy`, and if activated, auto-invokes the first
/// reasoning step (`reasoning_decompose`) and injects the result.
pub fn analyze_and_transform(content: &str) -> DeepThinkingResult {
    let policy = DefaultActivationPolicy { threshold: 3 };
    let signals = analyze_complexity(content);

    let activated = policy.should_activate(&signals);
    let reason = policy.reason(&signals);

    let transformed = if activated {
        let decomposition = auto_invoke_decompose(content);
        format_planning_message_with_decomposition(content, decomposition.as_deref())
    } else {
        content.to_string()
    };

    tracing::info!(
        activated,
        reason = %reason,
        content_len = content.len(),
        "Deep-thinking analysis complete"
    );

    DeepThinkingResult {
        activated,
        reason,
        content: transformed,
    }
}

/// Heuristic complexity analysis of a user message.
///
/// Examines the message for indicators of complexity:
/// - Length (longer messages tend to be more complex)
/// - Multi-step keywords ("build", "implement", "create", "refactor")
/// - Architectural keywords ("design", "architecture", "system", "framework")
/// - Ambiguity signals ("figure out", "investigate", "explore")
/// - Scale signals ("multiple", "several", "various", "all")
fn analyze_complexity(content: &str) -> ActivationSignals {
    let lower = content.to_lowercase();
    let word_count = content.split_whitespace().count();

    // Determine tier based on message length and scope
    let tier = if word_count > 100 || contains_multi_file_indicators(&lower) {
        SignalTier::Heavy
    } else if word_count > 30 || contains_implementation_keywords(&lower) {
        SignalTier::Standard
    } else {
        SignalTier::Light
    };

    // Determine risk based on operation type
    let risk = if contains_critical_keywords(&lower) {
        SignalRisk::High
    } else if contains_modification_keywords(&lower) {
        SignalRisk::Medium
    } else {
        SignalRisk::Low
    };

    let is_strategic = contains_strategic_keywords(&lower);
    let is_ambiguous = contains_ambiguity_signals(&lower);

    let mut signals = ActivationSignals::new().with_tier(tier).with_risk(risk);

    if is_strategic {
        signals = signals.strategic();
    }
    if is_ambiguous {
        signals = signals.ambiguous();
    }

    signals
}

/// Generates the planning prompt prefix injected before the user's message.
///
/// When the reasoning engine activates, it auto-invokes `reasoning_decompose`
/// so the LLM sees an actual decomposition result instead of just being told
/// to call a tool. The prompt then instructs continuation from step 2.
fn format_planning_message_with_decomposition(
    original: &str,
    decomposition: Option<&str>,
) -> String {
    if let Some(decomp) = decomposition {
        format!(
            r#"<system-reminder>
Complex task detected. Decomposition:

{decomp}

RULES — follow this exact sequence:
1. Read at most 3 files/sections to understand the problem
2. IMMEDIATELY Write or Edit with your implementation
3. Bash to verify/test the result
4. If verification fails, fix with Edit (do NOT re-read everything)
5. If verification passes, STOP — output the result, do NOT rewrite working code

You MUST produce code by your 4th tool call. No exceptions.
Do NOT read the entire file — read only what you need, then write.
NEVER rewrite code that already produces correct output — ship it and move on.
</system-reminder>

{original}"#,
            decomp = decomp,
            original = original
        )
    } else {
        original.to_string()
    }
}

/// Auto-invoke the first reasoning step to guarantee the engine starts.
/// `reasoning_decompose` is a pure prompt-template tool (no I/O, no LLM call)
/// so this is instant and zero-cost.
fn auto_invoke_decompose(goal: &str) -> Option<String> {
    let tool = DecomposeProblemTool;
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let ctx = ToolContext::new(cwd);
    let params = serde_json::json!({"goal": goal, "context": ""});

    match tool.execute(params, &ctx) {
        Ok(output) => {
            tracing::info!(
                "Auto-invoke decompose succeeded ({} chars)",
                output.text.len()
            );
            Some(output.text)
        }
        Err(e) => {
            tracing::warn!("Auto-invoke decompose failed: {e}");
            None
        }
    }
}

// ── Keyword Detection Helpers ──────────────────────────────────────────────

fn contains_multi_file_indicators(text: &str) -> bool {
    const INDICATORS: &[&str] = &[
        "entire project",
        "full application",
        "complete system",
        "from scratch",
        "end-to-end",
        "mips interpreter",
        "compiler",
        "interpreter",
        "virtual machine",
        "operating system",
        "database engine",
        "web server",
        "game engine",
        "framework",
        "all files",
        "multiple files",
        "entire codebase",
    ];
    INDICATORS.iter().any(|i| text.contains(i))
}

fn contains_implementation_keywords(text: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "implement",
        "build",
        "create",
        "develop",
        "refactor",
        "rewrite",
        "migrate",
        "add feature",
        "integrate",
        "set up",
        "configure",
        "design",
        "from scratch",
        "interpreter",
        "compiler",
        "emulator",
    ];
    KEYWORDS.iter().any(|k| text.contains(k))
}

fn contains_critical_keywords(text: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "database",
        "migration",
        "authentication",
        "security",
        "encryption",
        "payment",
        "production",
        "deploy",
        "release",
    ];
    KEYWORDS.iter().any(|k| text.contains(k))
}

fn contains_modification_keywords(text: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "change", "modify", "update", "fix", "replace", "remove", "delete", "rename", "move",
    ];
    KEYWORDS.iter().any(|k| text.contains(k))
}

fn contains_strategic_keywords(text: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "architecture",
        "design",
        "system",
        "strategy",
        "plan",
        "roadmap",
        "structure",
        "organize",
        "pattern",
        "approach",
        "best practice",
        "scalab",
        "from scratch",
        "interpreter",
        "compiler",
        "emulator",
        "microservices",
    ];
    KEYWORDS.iter().any(|k| text.contains(k))
}

fn contains_ambiguity_signals(text: &str) -> bool {
    const SIGNALS: &[&str] = &[
        "not sure",
        "figure out",
        "investigate",
        "explore",
        "maybe",
        "might need",
        "somehow",
        "what do you think",
        "how should",
        "what's the best",
        "how to approach",
        "i need help deciding",
    ];
    SIGNALS.iter().any(|s| text.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_message_not_activated() {
        let result = analyze_and_transform("hello");
        assert!(!result.activated);
        assert_eq!(result.content, "hello");
    }

    #[test]
    fn test_complex_task_activated() {
        let result = analyze_and_transform(
            "Build a MIPS I interpreter from scratch that supports all instructions",
        );
        assert!(result.activated);
        assert!(result.content.contains("<system-reminder>"));
        assert!(result.content.contains("MIPS I interpreter"));
        assert!(result.content.contains("Write"));
        assert!(result.content.contains("MUST produce code"));
    }

    #[test]
    fn test_strategic_architecture_activated() {
        let result =
            analyze_and_transform("Design the architecture for our new microservices system");
        assert!(result.activated);
    }

    #[test]
    fn test_simple_fix_not_activated() {
        let result = analyze_and_transform("fix the typo in README.md");
        assert!(!result.activated);
    }

    #[test]
    fn test_ambiguity_boosts_activation() {
        let result = analyze_and_transform(
            "I need to figure out how to implement the caching layer, not sure about the approach",
        );
        // "figure out" + "not sure" → ambiguous, plus "implement" → standard tier
        // Should activate because strategic-ish + ambiguous
        assert!(result.activated);
    }

    #[test]
    fn test_reason_contains_factors() {
        let result = analyze_and_transform("Build a complete database engine from scratch");
        assert!(result.reason.contains("Activated"));
        assert!(result.reason.contains("heavy"));
    }

    #[test]
    fn test_multi_file_indicators() {
        assert!(contains_multi_file_indicators("build a mips interpreter"));
        assert!(contains_multi_file_indicators(
            "create a compiler from scratch"
        ));
        assert!(!contains_multi_file_indicators("fix a typo"));
    }

    #[test]
    fn test_planning_message_preserves_original() {
        let original =
            "Build a MIPS I interpreter that supports R-type, I-type, and J-type instructions";
        let result = analyze_and_transform(original);
        assert!(result.content.contains(original));
    }
}
