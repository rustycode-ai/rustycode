---
name: document
description: Documentation generation tool for creating READMEs, API docs, and architecture summaries. Use when documenting new features or improving existing docs.
effort: medium
activation:
  mode: manual
allowed_tools:
  - Read
  - Write
---

# Document Skill

The Document Skill ensures that codebases are well-documented, accessible, and maintainable.

## Capabilities
- **API Documentation**: Generate clear documentation for public interfaces and modules.
- **Onboarding Guides**: Create READMEs and guides for new contributors.
- **Architecture Summaries**: Document high-level design decisions and component interactions.

## Workflow
1. Read the target code and any existing documentation.
2. Identify the target audience and documentation goals.
3. Use `Write` to create or update documentation files (e.g., `README.md`, `ARCHITECTURE.md`).
4. Ensure documentation is accurate, concise, and follows project conventions.

## Tools
- `Read`: Inspect source code and existing documentation.
- `Write`: Create or modify documentation files.
