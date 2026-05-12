---
name: test
description: Comprehensive test generation and verification tool. Use when adding new features, fixing bugs, or improving test coverage.
effort: medium
activation:
  mode: manual
allowed_tools:
  - Read
  - Write
  - Bash
---

# Test Skill

The Test Skill focuses on creating robust, reliable, and comprehensive test suites for software components.

## Capabilities
- **Unit Testing**: Generate focused tests for individual functions and modules.
- **Integration Testing**: Verify the interaction between different components and subsystems.
- **Regression Testing**: Ensure that bug fixes remain permanent and features continue to work as intended.

## Workflow
1. Read the target code to understand its public API and edge cases.
2. Identify testing requirements and strategy (e.g., table-driven tests, property-based testing).
3. Use `Write` to create or update test files.
4. Use `Bash` to run tests and verify coverage/results.
5. Iterate until all scenarios are covered and tests pass.

## Tools
- `Read`: Inspect source code and existing tests.
- `Write`: Create or modify test implementation.
- `Bash`: Run test runners (cargo test, pytest, npm test).
