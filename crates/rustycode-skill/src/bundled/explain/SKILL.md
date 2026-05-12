---
name: explain
description: Code explanation tool for providing clear, high-level understanding of complex logic and patterns. Use when onboarding to a new codebase or deciphering intricate algorithms.
effort: low
activation:
  mode: manual
allowed_tools:
  - Read
---

# Explain Skill

The Explain Skill translates technical implementation details into clear, human-readable explanations.

## Capabilities
- **Logic Mapping**: Break down complex functions into step-by-step logic flows.
- **Pattern Identification**: Identify and explain architectural patterns (e.g., factories, observers).
- **Onboarding Support**: Provide high-level summaries of crate structures and module responsibilities.

## Workflow
1. Read the target code and its documentation.
2. Analyze the data structures and control flow.
3. Synthesize the findings into a clear narrative that describes *what* the code does and *why* it was implemented that way.
4. Highlight notable patterns or trade-offs made in the implementation.

## Tools
- `Read`: Inspect source code and documentation.
