use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde_json::json;
use std::env;
use std::io::IsTerminal;

const MAX_QUESTIONS: usize = 4;
const MAX_OPTIONS: usize = 4;
const MIN_OPTIONS: usize = 2;
const CHIP_WIDTH: usize = 12;

#[derive(serde::Deserialize, JsonSchema)]
pub struct QuestionOption {
    label: String,
    description: String,
    #[allow(dead_code)]
    preview: Option<String>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct Question {
    #[serde(rename = "question")]
    text: String,
    header: String,
    options: Vec<QuestionOption>,
    #[serde(default)]
    multi_select: bool,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct AskUserQuestionParams {
    questions: Vec<Question>,
    answers: Option<std::collections::HashMap<String, String>>,
    #[allow(dead_code)]
    annotations: Option<std::collections::HashMap<String, QuestionAnnotation>>,
}

#[derive(serde::Deserialize, serde::Serialize, JsonSchema)]
pub struct QuestionAnnotation {
    #[allow(dead_code)]
    preview: Option<String>,
    #[allow(dead_code)]
    notes: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct AskUserQuestionTool;

    name: "AskUserQuestion",
    description: r#"Use this tool when you need to ask the user questions during execution. This allows you to:
1. Gather user preferences or requirements
2. Clarify ambiguous instructions
3. Get decisions on implementation choices as you work
4. Offer choices to the user about what direction to take

Usage notes:
- Users will always be able to select "Other" to provide custom text input
- Use multiSelect: true to allow multiple answers to be selected for a question
- If you recommend a specific option, make that the first option in the list and add "(Recommended)" at the end of the label
- Each question must have 2-4 options with a short label and description
- The header field is a very short chip/tag (max 12 chars) for the question category

**Behavior by mode:**
- **TUI mode**: Shows a rich interactive dialog with option cards and previews
- **CLI mode**: Prompts via stdin/stdout with numbered choices
- **Auto mode**: Uses the first option of each question automatically"#,
    permission: ToolPermission::None,
    tags: [ToolTag::Explore],

    execute(params: AskUserQuestionParams, ctx) {
        let questions = params.questions;
        let provided_answers = params.answers;
        let annotations = params.annotations;

        validate_questions(&questions)?;

        let is_auto_mode = env::var("RUSTYCODE_AUTO_MODE")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);

        // When stdin is not a terminal (e.g., TUI has captured it, or piped
        // input), attempting to read from it would deadlock. The TUI sets
        // RUSTYCODE_TUI=1 to signal this. Fall back to auto-answer with a
        // note so the LLM knows it should re-ask later if the answer matters.
        let stdin_is_tty = std::io::stdin().is_terminal();
        let in_tui = std::env::var("RUSTYCODE_TUI").is_ok();
        let non_interactive = !stdin_is_tty || in_tui;

        let answers = if let Some(ref ans) = provided_answers {
            ans.clone()
        } else if is_auto_mode || non_interactive {
            auto_answer(&questions)?
        } else {
            prompt_questions(&questions)?
        };

        let auto_selected = provided_answers.is_none() && (is_auto_mode || non_interactive);

        let mut parts = Vec::new();
        for q in &questions {
            let answer = answers.get(&q.text).cloned().unwrap_or_default();
            parts.push(format!("\"{}\"=\"{}\"", q.text, answer));
        }

        let answers_text = parts.join(", ");

        let output = if auto_selected {
            format!(
                "Auto-selected answers (non-interactive mode): {}. \
                 These are defaults, not user choices. \
                 If the specific choice matters, proceed with the recommended option \
                 and note this in your response.",
                answers_text
            )
        } else {
            format!(
                "User has answered your questions: {}. You can now continue with the user's answers in mind.",
                answers_text
            )
        };

        let metadata = json!({
            "questions": questions.iter().map(|q| json!({
                "question": q.text,
                "header": q.header,
                "options": q.options.iter().map(|o| json!({
                    "label": o.label,
                    "description": o.description,
                })).collect::<Vec<_>>(),
                "multiSelect": q.multi_select,
            })).collect::<Vec<_>>(),
            "answers": answers,
            "annotations": annotations,
        });

        Ok(ToolOutput::text(output).with_metadata(ctx, || metadata))
    }
}

fn validate_questions(questions: &[Question]) -> Result<()> {
    if questions.is_empty() || questions.len() > MAX_QUESTIONS {
        return Err(anyhow!(
            "Must provide 1-{} questions, got {}",
            MAX_QUESTIONS,
            questions.len()
        ));
    }

    let question_texts: Vec<&str> = questions.iter().map(|q| q.text.as_str()).collect();
    if question_texts.len()
        != question_texts
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
    {
        return Err(anyhow!("Question texts must be unique"));
    }

    for q in questions {
        if q.options.len() < MIN_OPTIONS || q.options.len() > MAX_OPTIONS {
            return Err(anyhow!(
                "Question \"{}\" must have {}-{} options, got {}",
                q.text,
                MIN_OPTIONS,
                MAX_OPTIONS,
                q.options.len()
            ));
        }

        let labels: Vec<&str> = q.options.iter().map(|o| o.label.as_str()).collect();
        if labels.len()
            != labels
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
        {
            return Err(anyhow!(
                "Option labels must be unique within question \"{}\"",
                q.text
            ));
        }

        if q.header.len() > CHIP_WIDTH {
            return Err(anyhow!(
                "Header \"{}\" exceeds {} characters",
                q.header,
                CHIP_WIDTH
            ));
        }
    }

    Ok(())
}

fn auto_answer(questions: &[Question]) -> Result<std::collections::HashMap<String, String>> {
    let mut answers = std::collections::HashMap::new();
    for q in questions {
        let first = q
            .options
            .first()
            .ok_or_else(|| anyhow!("Question \"{}\" has no options", q.text))?;
        answers.insert(q.text.clone(), first.label.clone());
    }
    Ok(answers)
}

fn prompt_questions(questions: &[Question]) -> Result<std::collections::HashMap<String, String>> {
    use std::io::{self, Write};

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    let mut answers = std::collections::HashMap::new();

    for q in questions {
        writeln!(handle, "\n--- {} ---", q.header)?;
        writeln!(handle, "{}", q.text)?;
        writeln!(handle)?;

        for (i, opt) in q.options.iter().enumerate() {
            writeln!(handle, "  {}. {} — {}", i + 1, opt.label, opt.description)?;
        }

        if q.multi_select {
            write!(handle, "\nYour choices (comma-separated numbers or text): ")?;
        } else {
            write!(
                handle,
                "\nYour choice (number or text, or 'other' for custom): "
            )?;
        }
        handle.flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            let first = q
                .options
                .first()
                .ok_or_else(|| anyhow!("No options available"))?;
            answers.insert(q.text.clone(), first.label.clone());
            continue;
        }

        if q.multi_select {
            let selections = parse_multi_selection(input, &q.options);
            answers.insert(q.text.clone(), selections);
        } else {
            let answer = parse_single_selection(input, &q.options);
            answers.insert(q.text.clone(), answer);
        }
    }

    Ok(answers)
}

#[must_use]
fn parse_single_selection(input: &str, options: &[QuestionOption]) -> String {
    if let Ok(num) = input.parse::<usize>() {
        if num > 0 && num <= options.len() {
            return options[num - 1].label.clone();
        }
    }

    for opt in options {
        if opt.label.eq_ignore_ascii_case(input) {
            return opt.label.clone();
        }
    }

    input.to_string()
}

#[must_use]
fn parse_multi_selection(input: &str, options: &[QuestionOption]) -> String {
    let mut selected = Vec::new();

    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if let Ok(num) = part.parse::<usize>() {
            if num > 0 && num <= options.len() {
                selected.push(options[num - 1].label.clone());
                continue;
            }
        }

        for opt in options {
            if opt.label.eq_ignore_ascii_case(part) {
                selected.push(opt.label.clone());
                break;
            }
        }
    }

    if selected.is_empty() {
        input.to_string()
    } else {
        selected.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;
    use crate::ToolContext;

    static AUTO_MODE_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    fn with_auto_mode<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _lock = AUTO_MODE_LOCK.lock();
        let previous = std::env::var("RUSTYCODE_AUTO_MODE").ok();
        std::env::set_var("RUSTYCODE_AUTO_MODE", "true");
        let result = f();
        match previous {
            Some(v) => std::env::set_var("RUSTYCODE_AUTO_MODE", v),
            None => std::env::remove_var("RUSTYCODE_AUTO_MODE"),
        }
        result
    }

    #[test]
    fn tool_metadata() {
        let tool = AskUserQuestionTool;
        assert_eq!(tool.name(), "AskUserQuestion");
        assert!(tool.description().contains("ask the user questions"));
        assert_eq!(tool.permission(), ToolPermission::None);
    }

    #[test]
    fn parameters_schema() {
        let tool = AskUserQuestionTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn validate_good_questions() {
        let questions = vec![Question {
            text: "Which framework?".into(),
            header: "Framework".into(),
            options: vec![
                QuestionOption {
                    label: "React (Recommended)".into(),
                    description: "Component-based UI library".into(),
                    preview: None,
                },
                QuestionOption {
                    label: "Vue".into(),
                    description: "Progressive framework".into(),
                    preview: None,
                },
            ],
            multi_select: false,
        }];
        assert!(validate_questions(&questions).is_ok());
    }

    #[test]
    fn validate_too_many_questions() {
        let questions: Vec<Question> = (0..5)
            .map(|i| Question {
                text: format!("Q{i}?"),
                header: format!("H{i}"),
                options: vec![
                    QuestionOption {
                        label: "A".into(),
                        description: "a".into(),
                        preview: None,
                    },
                    QuestionOption {
                        label: "B".into(),
                        description: "b".into(),
                        preview: None,
                    },
                ],
                multi_select: false,
            })
            .collect();
        assert!(validate_questions(&questions).is_err());
    }

    #[test]
    fn validate_too_few_options() {
        let questions = vec![Question {
            text: "Q?".into(),
            header: "H".into(),
            options: vec![QuestionOption {
                label: "Only".into(),
                description: "one option".into(),
                preview: None,
            }],
            multi_select: false,
        }];
        let err = validate_questions(&questions).unwrap_err();
        assert!(err.to_string().contains("2-4 options"));
    }

    #[test]
    fn validate_duplicate_question_texts() {
        let questions = vec![
            Question {
                text: "Same?".into(),
                header: "H1".into(),
                options: vec![
                    QuestionOption {
                        label: "A".into(),
                        description: "a".into(),
                        preview: None,
                    },
                    QuestionOption {
                        label: "B".into(),
                        description: "b".into(),
                        preview: None,
                    },
                ],
                multi_select: false,
            },
            Question {
                text: "Same?".into(),
                header: "H2".into(),
                options: vec![
                    QuestionOption {
                        label: "C".into(),
                        description: "c".into(),
                        preview: None,
                    },
                    QuestionOption {
                        label: "D".into(),
                        description: "d".into(),
                        preview: None,
                    },
                ],
                multi_select: false,
            },
        ];
        let err = validate_questions(&questions).unwrap_err();
        assert!(err.to_string().contains("unique"));
    }

    #[test]
    fn validate_duplicate_option_labels() {
        let questions = vec![Question {
            text: "Q?".into(),
            header: "H".into(),
            options: vec![
                QuestionOption {
                    label: "Same".into(),
                    description: "a".into(),
                    preview: None,
                },
                QuestionOption {
                    label: "Same".into(),
                    description: "b".into(),
                    preview: None,
                },
            ],
            multi_select: false,
        }];
        let err = validate_questions(&questions).unwrap_err();
        assert!(err.to_string().contains("unique"));
    }

    #[test]
    fn validate_header_too_long() {
        let questions = vec![Question {
            text: "Q?".into(),
            header: "This header is way too long for a chip".into(),
            options: vec![
                QuestionOption {
                    label: "A".into(),
                    description: "a".into(),
                    preview: None,
                },
                QuestionOption {
                    label: "B".into(),
                    description: "b".into(),
                    preview: None,
                },
            ],
            multi_select: false,
        }];
        let err = validate_questions(&questions).unwrap_err();
        assert!(err.to_string().contains("12 characters"));
    }

    #[test]
    fn auto_mode_picks_first_option() {
        let questions = vec![Question {
            text: "DB?".into(),
            header: "Database".into(),
            options: vec![
                QuestionOption {
                    label: "PostgreSQL".into(),
                    description: "Advanced RDBMS".into(),
                    preview: None,
                },
                QuestionOption {
                    label: "MySQL".into(),
                    description: "Popular RDBMS".into(),
                    preview: None,
                },
            ],
            multi_select: false,
        }];
        let answers = auto_answer(&questions).unwrap();
        assert_eq!(answers.get("DB?").unwrap(), "PostgreSQL");
    }

    #[test]
    fn execute_auto_mode() {
        with_auto_mode(|| {
            let tool = AskUserQuestionTool;
            let ctx = ToolContext::new("/tmp");

            let result = tool.execute(
                json!({
                    "questions": [{
                        "question": "Which ORM?",
                        "header": "ORM",
                        "options": [
                            {"label": "Diesel (Recommended)", "description": "Type-safe ORM"},
                            {"label": "SQLx", "description": "Async SQL"}
                        ]
                    }]
                }),
                &ctx,
            );

            assert!(result.is_ok());
            let output = result.unwrap();
            assert!(output.text.contains("Diesel (Recommended)"));
            assert!(output.text.contains("Auto-selected answers"));
        })
    }

    #[test]
    fn execute_with_pre_provided_answers() {
        let tool = AskUserQuestionTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({
                "questions": [{
                    "question": "Auth method?",
                    "header": "Auth",
                    "options": [
                        {"label": "JWT", "description": "Token-based auth"},
                        {"label": "Session", "description": "Cookie-based auth"}
                    ]
                }],
                "answers": {"Auth method?": "JWT"}
            }),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("JWT"));
    }

    #[test]
    fn execute_rejects_empty_questions() {
        with_auto_mode(|| {
            let tool = AskUserQuestionTool;
            let ctx = ToolContext::new("/tmp");

            let result = tool.execute(
                json!({
                    "questions": []
                }),
                &ctx,
            );

            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(err.to_string().contains("1-4 questions"));
        })
    }

    #[test]
    fn parse_single_selection_number() {
        let opts = vec![
            QuestionOption {
                label: "Alpha".into(),
                description: "a".into(),
                preview: None,
            },
            QuestionOption {
                label: "Beta".into(),
                description: "b".into(),
                preview: None,
            },
        ];
        assert_eq!(parse_single_selection("1", &opts), "Alpha");
        assert_eq!(parse_single_selection("2", &opts), "Beta");
    }

    #[test]
    fn parse_single_selection_text() {
        let opts = vec![
            QuestionOption {
                label: "Alpha".into(),
                description: "a".into(),
                preview: None,
            },
            QuestionOption {
                label: "Beta".into(),
                description: "b".into(),
                preview: None,
            },
        ];
        assert_eq!(parse_single_selection("beta", &opts), "Beta");
    }

    #[test]
    fn parse_single_selection_custom() {
        let opts = vec![
            QuestionOption {
                label: "Alpha".into(),
                description: "a".into(),
                preview: None,
            },
            QuestionOption {
                label: "Beta".into(),
                description: "b".into(),
                preview: None,
            },
        ];
        assert_eq!(
            parse_single_selection("my custom answer", &opts),
            "my custom answer"
        );
    }

    #[test]
    fn parse_multi_selection_mixed() {
        let opts = vec![
            QuestionOption {
                label: "Alpha".into(),
                description: "a".into(),
                preview: None,
            },
            QuestionOption {
                label: "Beta".into(),
                description: "b".into(),
                preview: None,
            },
            QuestionOption {
                label: "Gamma".into(),
                description: "g".into(),
                preview: None,
            },
        ];
        let result = parse_multi_selection("1, Beta, 3", &opts);
        assert_eq!(result, "Alpha, Beta, Gamma");
    }
}
