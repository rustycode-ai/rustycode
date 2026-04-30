use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::env;
use std::io::{self, Write};

/// Question tool - Ask user for preferences, decisions, or clarification
///
/// This tool enables interactive questions during agent execution:
/// - In TUI mode: Handled inline by the streaming layer with a native dialog
/// - In CLI mode: Prompts user via stdin/stdout
/// - In auto mode: Uses defaults or skips
///
/// # TUI Integration
///
/// When running in the TUI, the "question" tool is intercepted during streaming.
/// The TUI shows a native dialog and sends the answer back through a channel.
/// This provides a seamless user experience without blocking stdin.
///
/// # Auto Mode Behavior
///
/// In auto mode (non-interactive), this tool will:
/// - Return the default value if provided
/// - Return the first option if options are given
/// - Skip entirely if no default or options are available
pub struct QuestionTool;

impl Tool for QuestionTool {
    fn name(&self) -> &'static str {
        "question"
    }

    fn description(&self) -> &'static str {
        r#"Ask the user a question and get their response.

Use this tool when you need to:
- Gather user preferences or requirements
- Clarify ambiguous instructions
- Get approval for significant decisions
- Choose between multiple implementation options
- Confirm understanding before proceeding

**Behavior by mode:**
- **TUI mode**: Shows a native dialog with optional multiple-choice selection
- **CLI mode**: Prompts via stdin/stdout
- **Auto mode**: Uses default value or first option automatically

**Examples:**
- Ask for database choice: {"question": "Which database?", "options": ["PostgreSQL", "MySQL", "SQLite"]}
- Confirm action: {"question": "Continue with deletion?", "default": "no", "options": ["yes", "no"]}
- Get preference: {"question": "Testing framework preference?", "default": "pytest"}"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["question"],
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user"
                },
                "options": {
                    "type": "array",
                    "description": "List of options for the user to choose from (optional)",
                    "items": { "type": "string" },
                    "minItems": 1
                },
                "default": {
                    "type": "string",
                    "description": "Default value to use if user doesn't respond or in auto mode"
                },
                "multiple": {
                    "type": "boolean",
                    "description": "Allow multiple selections (comma-separated)",
                    "default": false
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let question = required_string(&params, "question")?;
        let options = params
            .get("options")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>());
        let default = optional_string(&params, "default");
        let multiple = params
            .get("multiple")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Check if we're in auto mode (non-interactive)
        let is_auto_mode = env::var("RUSTYCODE_AUTO_MODE")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);

        let response = if is_auto_mode {
            // In auto mode, use default or first option
            if let Some(def) = default {
                def.to_string()
            } else if let Some(opts) = &options {
                opts.first()
                    .ok_or_else(|| anyhow!("No options provided and no default value"))?
                    .to_string()
            } else {
                return Err(anyhow!(
                    "Question tool requires either 'default' or 'options' in auto mode"
                ));
            }
        } else {
            // Interactive mode - prompt the user
            prompt_user(question, options.as_deref(), default, multiple)?
        };

        let output = format!("**Question:** {question}\n\n**Your response:** {response}");

        // Build metadata
        let metadata = json!({
            "question": question,
            "response": response,
            "has_options": options.is_some(),
            "has_default": default.is_some(),
            "multiple": multiple,
            "auto_mode": is_auto_mode
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

/// Prompt the user and get their response
fn prompt_user(
    question: &str,
    options: Option<&[&str]>,
    default: Option<&str>,
    multiple: bool,
) -> Result<String> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    // Display the question
    writeln!(handle, "\n--- Question ---")?;
    writeln!(handle, "{question}")?;

    // Display options if provided
    if let Some(opts) = options {
        writeln!(handle)?;
        for (i, opt) in opts.iter().enumerate() {
            let marker = if let Some(def) = default {
                if *opt == def {
                    " (default)"
                } else {
                    ""
                }
            } else {
                ""
            };
            writeln!(handle, "{}. {}{}", i + 1, opt, marker)?;
        }
    }

    // Display default value
    if let Some(def) = default {
        writeln!(handle, "\n[Default: {def}]")?;
    }

    // Display prompt
    if multiple {
        write!(handle, "\nYour response (comma-separated): ")?;
    } else if options.is_some() {
        write!(handle, "\nYour choice (number or text): ")?;
    } else {
        write!(handle, "\nYour response: ")?;
    }

    handle.flush()?;

    // Read user input
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let input = input.trim();

    // Handle empty input
    if input.is_empty() {
        if let Some(def) = default {
            return Ok(def.to_string());
        }
        return Err(anyhow!("No response provided and no default value"));
    }

    // Parse numbered selection
    if let Some(opts) = options {
        // Try to parse as number
        if let Ok(num) = input.parse::<usize>() {
            if num > 0 && num <= opts.len() {
                return Ok(opts[num - 1].to_string());
            }
        }

        // Check if input matches an option exactly
        if opts.iter().any(|opt| opt.eq_ignore_ascii_case(input)) {
            return Ok(input.to_string());
        }

        // If we have options but input doesn't match, return as-is
        // (user might have typed a custom response)
    }

    // Handle multiple selections
    if multiple {
        // Split by comma and clean up
        let selections: Vec<&str> = input
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        if selections.is_empty() {
            return Err(anyhow!("No valid selections provided"));
        }

        // Validate against options if provided
        if let Some(opts) = options {
            for selection in &selections {
                if !opts.iter().any(|opt| opt.eq_ignore_ascii_case(selection)) {
                    return Err(anyhow!("Invalid selection: {selection}"));
                }
            }
        }

        return Ok(selections.join(", "));
    }

    Ok(input.to_string())
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string parameter `{key}`"))
}

fn optional_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_question_tool_metadata() {
        let tool = QuestionTool;
        assert_eq!(tool.name(), "question");
        assert!(tool.description().contains("Ask the user"));
        assert_eq!(tool.permission(), ToolPermission::None);
    }

    #[test]
    fn test_question_parameters_schema() {
        let tool = QuestionTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "question");

        // Check question property
        assert_eq!(schema["properties"]["question"]["type"], "string");

        // Check options array
        assert_eq!(schema["properties"]["options"]["type"], "array");
        assert_eq!(schema["properties"]["options"]["items"]["type"], "string");

        // Check default
        assert_eq!(schema["properties"]["default"]["type"], "string");

        // Check multiple
        assert_eq!(schema["properties"]["multiple"]["type"], "boolean");
        assert_eq!(schema["properties"]["multiple"]["default"], false);
    }

    #[test]
    fn test_question_missing_question() {
        let tool = QuestionTool;
        let ctx = ToolContext::new("/tmp");

        // Set auto mode to avoid stdin issues in tests
        env::set_var("RUSTYCODE_AUTO_MODE", "true");

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("question"));

        env::remove_var("RUSTYCODE_AUTO_MODE");
    }

    #[test]
    fn test_question_auto_mode_with_default() {
        let tool = QuestionTool;
        let ctx = ToolContext::new("/tmp");

        env::set_var("RUSTYCODE_AUTO_MODE", "true");

        let result = tool.execute(
            json!({
                "question": "Choose a framework",
                "default": "React"
            }),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("React"));
        assert!(output.text.contains("**Your response:** React"));

        env::remove_var("RUSTYCODE_AUTO_MODE");
    }

    #[test]
    fn test_question_auto_mode_with_options() {
        let tool = QuestionTool;
        let ctx = ToolContext::new("/tmp");

        env::set_var("RUSTYCODE_AUTO_MODE", "true");

        let result = tool.execute(
            json!({
                "question": "Choose a framework",
                "options": ["React", "Vue", "Angular"]
            }),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        // Should return first option in auto mode
        assert!(output.text.contains("React"));

        env::remove_var("RUSTYCODE_AUTO_MODE");
    }

    #[test]
    fn test_question_auto_mode_no_default_or_options() {
        let tool = QuestionTool;
        let ctx = ToolContext::new("/tmp");

        env::set_var("RUSTYCODE_AUTO_MODE", "true");

        let result = tool.execute(
            json!({
                "question": "What's your name?"
            }),
            &ctx,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("requires either 'default' or 'options'"));

        env::remove_var("RUSTYCODE_AUTO_MODE");
    }

    #[test]
    fn test_question_metadata() {
        let tool = QuestionTool;
        let ctx = ToolContext::new("/tmp");

        env::set_var("RUSTYCODE_AUTO_MODE", "true");

        let result = tool.execute(
            json!({
                "question": "Choose a database",
                "options": ["PostgreSQL", "MySQL"],
                "default": "PostgreSQL",
                "multiple": false
            }),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();

        // Check metadata
        let metadata = output.structured.unwrap();
        assert_eq!(metadata["question"], "Choose a database");
        assert_eq!(metadata["response"], "PostgreSQL");
        assert_eq!(metadata["has_options"], true);
        assert_eq!(metadata["has_default"], true);
        assert_eq!(metadata["multiple"], false);
        assert_eq!(metadata["auto_mode"], true);

        env::remove_var("RUSTYCODE_AUTO_MODE");
    }
}
