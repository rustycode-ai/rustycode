# rustycode-prompt

Prompt templating system using Handlebars with built-in LLM system/user prompts.

## Purpose

Provides a flexible templating system for constructing LLM prompts with variable interpolation, built-in system prompts (coding assistant, code review, debugging), and dynamic context injection. Uses Handlebars syntax for familiar template rendering with fallback for missing variables.

## Key Types

- `TemplateManager` — Core template registry and rendering engine
- `PromptBuilder` — Layered prompt construction (system + user + context)
- `PromptLayer` — Individual prompt component
- `EnvironmentContext` — Git status, directory context
- `InstructionScanner` — Extracts .md instructions from projects
- `ModelProvider` — Identifies model capabilities for prompt optimization

## Public API

```rust
use rustycode_prompt::{TemplateManager, context};

let manager = TemplateManager::new()?;

// Render a built-in system prompt
let context = context! {
    "name" => "CodeBot",
    "language" => "Rust"
};
let prompt = manager.coding_assistant_prompt(&context)?;

// Render a template by name
let user_prompt = manager.render("user/explain_code", &context)?;

// Inline template rendering
let mut manager = TemplateManager::new()?;
let output = manager.render_inline("Hello {{name}}!", &context)?;
```

## Built-in Templates

### System Prompts

- `system/coding_assistant` — General coding help
- `system/code_review` — Code review and feedback
- `system/debug` — Debugging and diagnostics
- `system/headless_coding_agent` — Autonomous agent mode

### User Prompts

- `user/explain_code` — Code explanation requests
- `user/generate` — Code generation with requirements
- `user/refactor` — Code improvement requests

## Template Variables

```
// Coding assistant
"name" → Assistant name (default: "Claude")
"context" → Additional context (Git status, file lists, etc.)

// Code review
"language" → Target programming language
"code" → Code to review

// User requests
"language", "code", "task", "requirements", "constraints", "examples", "goals"
```

## Features

- **Handlebars templating** — Familiar {{variable}} syntax
- **Custom templates** — Load from `.hbs`/`.tera` files
- **Inline rendering** — Render templates inline without registration
- **Context helpers** — Macros for building template contexts
- **Smart defaults** — Missing variables render as empty string
- **Unicode support** — Full support for international characters

## Dependencies

- `handlebars` — Template rendering
- `serde_json` — JSON context
- `walkdir` — File discovery for custom templates
- `thiserror` — Error types
- `tokio` — Async runtime (for async discovery)

## Architecture Notes

- Sits between LLM providers and orchestration
- Used to construct consistent, high-quality prompts
- Extensible for domain-specific prompt libraries
- Caches parsed templates in registry
- Inline templates capped at 256 to prevent memory bloat

## Testing

- Template rendering tests
- Built-in template validation
- Context macro tests
- Inline template caching tests
- Unicode and special character handling

## See Also

- `rustycode-llm` — LLM provider implementations
- `rustycode-orchestration` — Autonomous mode that uses prompts
