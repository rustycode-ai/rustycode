---
name: review
description: Comprehensive code review tool for identifying bugs, performance bottlenecks, and style issues. Use when reviewing pull requests or auditing code quality.
effort: medium
activation:
  mode: manual
allowed_tools:
  - Read
  - Grep
---

# Review Skill

The Review Skill provides a systematic framework for auditing code quality and identifying potential improvements.

## Capabilities
- **Static Analysis**: Audit code for common anti-patterns and performance bottlenecks.
- **Safety Check**: Identify potential security vulnerabilities or unsafe patterns.
- **Style Review**: Ensure consistency with project conventions and idiomatic usage.

## Workflow
1. Read the target files to understand the implementation logic.
2. Use `grep` to find related usages and cross-references.
3. Analyze code for:
   - Resource leaks (memory, file handles).
   - Unnecessary complexity or duplication.
   - Suboptimal algorithms or data structures.
   - Adherence to project-specific rules (e.g., error handling patterns).
4. Provide a structured report with findings and concrete improvement suggestions.

## Tools
- `Read`: Inspect source code and documentation.
- `Grep`: Search for patterns, usages, and anti-patterns.
