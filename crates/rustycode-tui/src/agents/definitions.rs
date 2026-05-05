//! Built-in agent definitions with `when_to_use` descriptions.

/// A built-in agent definition.
pub struct AgentDefinition {
    /// Agent type identifier (e.g. "general-purpose", "explore").
    pub agent_type: &'static str,
    /// Short label for display.
    #[allow(dead_code)]
    pub label: &'static str,
    /// When-to-use description — included in the tool prompt so the LLM
    /// can decide which agent to invoke based on user message content.
    pub when_to_use: &'static str,
    /// System prompt for the sub-agent.
    pub system_prompt: &'static str,
}

/// All built-in agent definitions.
pub fn built_in_agents() -> &'static [AgentDefinition] {
    &[
        AgentDefinition {
            agent_type: "general-purpose",
            label: "General Purpose",
            when_to_use: "Use when you need a second opinion on whether this implementation, \
                want a deeper root-cause investigation, or should hand a substantial coding task \
                to a subagent through the shared runtime.",
            system_prompt: "You are a capable coding assistant with access to file editing, \
                search, and bash tools. Execute the given task autonomously. Read files to \
                understand context before making changes. Verify your work by running relevant \
                tests or checks after implementation.",
        },
        AgentDefinition {
            agent_type: "explore",
            label: "Explorer",
            when_to_use: "Use when you need to quickly find files by patterns, search code \
                for keywords, or answer questions about the codebase — especially for broad \
                exploration that would take more than 3 queries.",
            system_prompt: "You are a codebase explorer. Find files, trace execution paths, \
                map architecture layers, and document dependencies to inform development \
                decisions. Be thorough in your search. Report findings concisely.",
        },
        AgentDefinition {
            agent_type: "plan",
            label: "Planner",
            when_to_use: "Use when you need to plan the implementation strategy for a task. \
                Returns step-by-step plans, identifies critical files, and considers \
                architectural trade-offs.",
            system_prompt: "You are a software architect planning feature implementations. \
                Analyze the existing codebase, identify relevant files and patterns, and \
                produce a clear step-by-step implementation plan. Consider dependencies, \
                risks, and trade-offs.",
        },
        AgentDefinition {
            agent_type: "code-reviewer",
            label: "Code Reviewer",
            when_to_use: "Use immediately after writing or modifying code. Reviews code for \
                quality, security, and maintainability.",
            system_prompt: "You are an expert code reviewer. Review the specified code for: \
                correctness, security vulnerabilities, error handling, performance issues, \
                naming conventions, and adherence to project patterns. Rate severity as \
                CRITICAL, HIGH, MEDIUM, or LOW.",
        },
        AgentDefinition {
            agent_type: "security-reviewer",
            label: "Security Reviewer",
            when_to_use: "Use when code handles user input, authentication, API endpoints, \
                or sensitive data. Flags secrets, SSRF, injection, unsafe crypto, and \
                OWASP Top 10 vulnerabilities.",
            system_prompt: "You are a security specialist. Analyze the code for vulnerabilities \
                including: hardcoded secrets, injection attacks (SQL, XSS, command), \
                authentication bypasses, path traversal, CSRF, and unsafe cryptographic \
                operations. Follow OWASP Top 10 guidelines.",
        },
        AgentDefinition {
            agent_type: "tdd-guide",
            label: "TDD Guide",
            when_to_use: "Use proactively when writing new features, fixing bugs, or \
                refactoring code. Enforces write-tests-first methodology with 80%+ coverage.",
            system_prompt:
                "You are a test-driven development specialist. Follow the RED-GREEN-REFACTOR \
                cycle: write failing tests first, implement minimal code to pass, then refactor. \
                Target 80%+ test coverage. Use descriptive test names that explain behavior.",
        },
        AgentDefinition {
            agent_type: "build-error-resolver",
            label: "Build Error Resolver",
            when_to_use: "Use when the build fails, type errors occur, or compilation issues \
                arise. Fixes build/type errors only with minimal diffs, no architectural edits. \
                Focuses on getting the build green quickly.",
            system_prompt: "You are a build error resolution specialist. Fix compilation errors, \
                type mismatches, and dependency issues with minimal changes. Do not make \
                architectural changes — focus only on getting the build to pass. Read error \
                messages carefully and fix root causes, not symptoms.",
        },
    ]
}

/// Build the tool description string that includes `when_to_use` entries
/// for auto-activation. The LLM reads this and decides which agent to invoke.
pub fn build_agent_tool_description() -> String {
    let agents = built_in_agents();
    let mut agent_lines = Vec::with_capacity(agents.len());

    for agent in agents {
        agent_lines.push(format!("- {}: {}", agent.agent_type, agent.when_to_use));
    }

    format!(
        "Launch a new agent to handle complex, multi-step tasks. Each agent type has \
         specialized capabilities:\n\n\
         {}\n\n\
         Available agent types:\n{}\n\n\
         The agent runs autonomously and returns results. Use for tasks that benefit from \
         focused, uninterrupted execution.",
        "When the agent is done, it will return a single message back to you.",
        agent_lines.join("\n"),
    )
}

/// Find an agent definition by type.
pub fn find_agent(agent_type: &str) -> Option<&'static AgentDefinition> {
    built_in_agents()
        .iter()
        .find(|a| a.agent_type == agent_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_agents_not_empty() {
        let agents = built_in_agents();
        assert!(!agents.is_empty());
        assert!(agents.len() >= 7);
    }

    #[test]
    fn all_agents_have_required_fields() {
        for agent in built_in_agents() {
            assert!(!agent.agent_type.is_empty(), "agent_type is empty");
            assert!(
                !agent.label.is_empty(),
                "label is empty for {}",
                agent.agent_type
            );
            assert!(
                !agent.when_to_use.is_empty(),
                "when_to_use is empty for {}",
                agent.agent_type
            );
            assert!(
                !agent.system_prompt.is_empty(),
                "system_prompt is empty for {}",
                agent.agent_type
            );
        }
    }

    #[test]
    fn agent_types_are_unique() {
        let agents = built_in_agents();
        let mut types = std::collections::HashSet::new();
        for agent in agents {
            assert!(
                types.insert(agent.agent_type),
                "duplicate agent_type: {}",
                agent.agent_type
            );
        }
    }

    #[test]
    fn find_agent_known_type() {
        assert!(find_agent("general-purpose").is_some());
        assert!(find_agent("explore").is_some());
        assert!(find_agent("plan").is_some());
        assert!(find_agent("code-reviewer").is_some());
    }

    #[test]
    fn find_agent_unknown_returns_none() {
        assert!(find_agent("nonexistent").is_none());
    }

    #[test]
    fn build_description_contains_all_agents() {
        let desc = build_agent_tool_description();
        for agent in built_in_agents() {
            assert!(
                desc.contains(agent.agent_type),
                "missing agent_type: {}",
                agent.agent_type
            );
            assert!(
                desc.contains(agent.when_to_use),
                "missing when_to_use for: {}",
                agent.agent_type
            );
        }
    }
}
