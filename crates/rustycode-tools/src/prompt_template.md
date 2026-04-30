# Prompt Template System

A Handlebars-based template system for generating system and user prompts with dynamic context injection.

## Overview

The prompt template system provides:
- **Dynamic context injection**: Automatically inject project, git, and environment information
- **Reusable templates**: Built-in templates for common scenarios
- **Custom templates**: Register your own templates
- **Type-safe context**: Structured context with builder pattern
- **Token estimation**: Rough token estimates for rendered prompts

## Usage

### Basic Usage

```rust
use rustycode_tools::{PromptContext, PromptTemplateEngine};

// Create template engine
let engine = PromptTemplateEngine::new();

// Build context from environment
let ctx = PromptContext::from_environment("/path/to/project")
    .with_var("mode", "code")
    .with_open_files(vec!["src/main.rs".to_string()]);

// Render a template
let rendered = engine.render("system_default", &ctx)?;
println!("{}", rendered.content);
println!("Estimated tokens: {}", rendered.token_estimate);
```

### Built-in Templates

#### `system_default`
General-purpose system prompt with project context, git info, and tool usage guidelines.

#### `system_code_review`
Specialized for code review tasks with review guidelines and process.

#### `system_refactor`
Optimized for refactoring tasks with principles and workflow.

#### `user_task`
User task template with task description and context.

### Custom Templates

```rust
let mut engine = PromptTemplateEngine::new();

// Register a custom template
engine.register_template("my_template", "Hello {{name}}!")?;

// Render it
let mut ctx = PromptContext::from_environment(".");
ctx = ctx.with_var("name", "World");
let rendered = engine.render("my_template", &ctx)?;
assert_eq!(rendered.content, "Hello World!");
```

### Template String Rendering

```rust
let engine = PromptTemplateEngine::new();
let ctx = PromptContext::from_environment(".")
    .with_var("feature", "async");

let template = "Implement {{feature}} functionality";
let rendered = engine.render_string(template, &ctx)?;
```

## Context Fields

The `PromptContext` structure provides:

| Field | Type | Description |
|-------|------|-------------|
| `cwd` | `String` | Current working directory |
| `project_name` | `String` | Project directory name |
| `open_files` | `Vec<String>` | List of open/modified files |
| `git_branch` | `Option<String>` | Git branch name |
| `git_status` | `Option<String>` | Git status summary |
| `platform` | `String` | OS platform |
| `current_date` | `String` | Current date (YYYY-MM-DD) |
| `variables` | `HashMap<String, String>` | Custom variables |

## Handlebars Features

The template engine supports standard Handlebars syntax:

### Conditionals

```handlebars
{{#if git_branch}}
Branch: {{git_branch}}
{{else}}
No git repository
{{/if}}
```

### Loops

```handlebars
Files:
{{#each open_files}}
  - {{this}}
{{/each}}
```

### Custom Variables

```handlebars
Mode: {{mode}}
Focus: {{focus}}
```

## Integration with Existing Code

To integrate with the existing prompt building in `rustycode-core`:

```rust
use rustycode_tools::{PromptContext, PromptTemplateEngine};

// In run_agent() or similar
let template_engine = PromptTemplateEngine::new();
let mut ctx = PromptContext::from_environment(cwd.to_str().unwrap());
ctx = ctx
    .with_open_files(open_files)
    .with_git_branch(git_branch);

let rendered = template_engine.render("system_default", &ctx)?;
let system_prompt = rendered.content;

// Use system_prompt in ChatMessage::system()
```

## Testing

Run tests with:

```bash
cargo test -p rustycode-tools prompt_template
cargo test -p rustycode-tools --test prompt_template_tests
```

Run example:

```bash
cargo run -p rustycode-tools --example prompt_template_example
```

## Design Decisions

1. **Handlebars choice**: Industry-standard, well-documented, Rust-native
2. **No escaping**: Use `no_escape` to allow markdown and code blocks
3. **Static templates**: Compiled-in for zero runtime dependency
4. **Builder pattern**: Fluent API for context construction
5. **Rough token estimation**: Simple heuristic (len / 4) for budgeting

## Future Enhancements

- Template inheritance and composition
- External template loading from files
- Template validation and linting
- More accurate token estimation
- Template versioning and migration
- Context helpers for common scenarios
