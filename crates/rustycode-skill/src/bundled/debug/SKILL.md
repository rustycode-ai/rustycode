---
name: debug
description: Systematic debugging tool using instrumentation for stepping, variable inspection, and stack traces. Use when encountering bugs, test failures, or unexpected behavior.
license: MIT
metadata:
  version: "1.0"
  author: rustycode
---

# Debug Skill

The Debug Skill allows for systematic debugging of code through exploration, instrumentation, and testing.

## Capabilities
- **Explore**: Search for error messages, log patterns, and relevant code.
- **Inspect**: Read source code and configuration to understand state.
- **Execute**: Run tests and debug commands to reproduce and isolate issues.

## Workflow
1. Identify the bug or failing test.
2. Use `grep` to search for error messages or relevant code locations.
3. Use `read_file` to inspect the code and understand logic/state.
4. Use `bash` to run tests or execute reproduction scripts.
5. Once the bug is isolated, propose a fix and verify it.

## Tools
- `bash`: Run tests, debuggers (gdb, lldb, pdb), or reproduction scripts.
- `read_file`: Inspect source code and logs.
- `grep`: Search for patterns, errors, and variable usages.
