---
name: debug
description: Systematic debugging tool using instrumentation for stepping, variable inspection, and stack traces. Use when encountering bugs, test failures, or unexpected behavior.
license: MIT
metadata:
  version: "1.0"
  author: rustycode
---

# Debug Skill

The Debug Skill allows for systematic debugging of code through the agent's instrumentation of the codebase.

## Capabilities
- **Step**: Step through code execution (if instrumentation is available).
- **Inspect**: Read variable states at runtime.
- **Trace**: Generate stack traces or flow analysis.

## Workflow
1. Identify the bug or failing test.
2. Use `debug_step` or `debug_inspect` to localize the issue.
3. Once the bug is isolated, propose a fix.
4. Verify the fix by running the affected test.

## Tools
- `debug_step`: Execute next logical block or step.
- `debug_inspect`: Retrieve value of variables or state.
- `debug_trace`: Retrieve current execution trace/stack.
