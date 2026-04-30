#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cloned_instead_of_copied,
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::format_push_string,
    clippy::manual_string_new,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::needless_raw_string_hashes,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Tests for Claude's synthetic test data generation concepts.
//!
//! This test module covers:
//! - Variable extraction from prompt templates
//! - Synthetic test case generation with variable analysis
//! - Planning-driven test case generation
//! - Iterative refinement of test data
//! - Example block construction for multishot learning

use std::collections::HashMap;

/// Extract variables from a template (finds all {{VAR}} patterns)
fn extract_variables(template: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if let Some(&'{') = chars.peek() {
                chars.next(); // consume second '{'
                let mut var_name = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == '}' {
                        chars.next();
                        if let Some(&'}') = chars.peek() {
                            chars.next();
                            break;
                        }
                    }
                    var_name.push(ch);
                    chars.next();
                }
                if !var_name.is_empty() {
                    vars.push(var_name);
                }
            }
        }
    }
    vars
}

/// Variable specification with metadata for generating test data
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct VariableSpec {
    name: String,
    description: String,
    source: String,         // Who provides this data?
    format: String,         // How is it formatted?
    tone: String,           // What tone/style?
    typical_length: String, // Expected size
}

/// Synthetic test case with generated variable values
#[derive(Debug, Clone)]
struct SyntheticTestCase {
    variables: HashMap<String, String>,
    planning: String,
}

/// Generator for synthetic test data
#[allow(dead_code)]
struct SyntheticTestDataGenerator {
    template: String,
    variables: Vec<VariableSpec>,
}

impl SyntheticTestDataGenerator {
    fn new(template: &str) -> Self {
        let var_names = extract_variables(template);
        let variables = var_names
            .into_iter()
            .map(|name| VariableSpec {
                name: name.clone(),
                description: format!("Variable {}", name),
                source: "Unknown".to_string(),
                format: "Text".to_string(),
                tone: "Neutral".to_string(),
                typical_length: "Medium".to_string(),
            })
            .collect();

        Self {
            template: template.to_string(),
            variables,
        }
    }

    /// Update variable specification
    fn set_variable_spec(&mut self, name: &str, spec: VariableSpec) {
        if let Some(var) = self.variables.iter_mut().find(|v| v.name == name) {
            *var = spec;
        }
    }

    /// Generate variable analysis planning text
    fn generate_planning(&self) -> String {
        let mut planning = "1. Prompt Template Summary:\n".to_string();
        planning.push_str("This template contains variables that need to be populated ");
        planning.push_str("with realistic test data for evaluation.\n\n");

        planning.push_str("2. Variable Analysis:\n\n");
        for var in &self.variables {
            planning.push_str(&format!("{}:\n", var.name.to_uppercase()));
            planning.push_str(&format!("- Source: {}\n", var.source));
            planning.push_str(&format!("- Format: {}\n", var.format));
            planning.push_str(&format!("- Tone: {}\n", var.tone));
            planning.push_str(&format!("- Length: {}\n", var.typical_length));
            planning.push('\n');
        }
        planning
    }

    /// Create a test case with provided values
    fn create_test_case(&self, values: HashMap<String, String>) -> SyntheticTestCase {
        SyntheticTestCase {
            variables: values,
            planning: self.generate_planning(),
        }
    }
}

/// Construct example block for multishot learning
fn construct_example_block(
    input_template: &str,
    input_values: &HashMap<String, String>,
    output: &str,
) -> String {
    let mut example = String::new();
    example.push_str("<example>\n");
    example.push_str("<input>\n");

    // Fill in the template with values
    let filled = input_template;
    let mut filled = filled.to_string();
    for (key, value) in input_values {
        let placeholder = format!("{{{{{}}}}}", key);
        filled = filled.replace(&placeholder, value);
    }

    example.push_str(&filled);
    example.push_str("\n</input>\n");
    example.push_str("<output>\n");
    example.push_str(output);
    example.push_str("\n</output>\n");
    example.push_str("</example>\n");
    example
}

#[test]
fn test_extract_variables_for_synthetic_data() {
    let template = r#"Based on the following documents:

{{DOCUMENTS}}

Please answer this customer question:
{{QUESTION}}

Provide a helpful, policy-compliant response."#;

    let vars = extract_variables(template);
    assert_eq!(vars.len(), 2);
    assert!(vars.contains(&"DOCUMENTS".to_string()));
    assert!(vars.contains(&"QUESTION".to_string()));
}

#[test]
fn test_synthetic_test_data_generator_creation() {
    let template = "Analyze {{THING1}} and {{THING2}} for security issues.";
    let generator = SyntheticTestDataGenerator::new(template);

    assert_eq!(generator.variables.len(), 2);
    assert_eq!(generator.variables[0].name, "THING1");
    assert_eq!(generator.variables[1].name, "THING2");
}

#[test]
fn test_variable_specification_update() {
    let template = "Process {{DOCUMENTS}} with {{QUESTION}}";
    let mut generator = SyntheticTestDataGenerator::new(template);

    let doc_spec = VariableSpec {
        name: "DOCUMENTS".to_string(),
        description: "Company policy documents".to_string(),
        source: "Policy team".to_string(),
        format: "Structured FAQ entries".to_string(),
        tone: "Professional, formal".to_string(),
        typical_length: "300-500 words".to_string(),
    };

    generator.set_variable_spec("DOCUMENTS", doc_spec);

    assert_eq!(generator.variables[0].source, "Policy team");
    assert_eq!(generator.variables[0].tone, "Professional, formal");
}

#[test]
fn test_planning_generation() {
    let template = "Analyze {{DOCUMENTS}} and answer {{QUESTION}}";
    let mut generator = SyntheticTestDataGenerator::new(template);

    let doc_spec = VariableSpec {
        name: "DOCUMENTS".to_string(),
        description: "Company documents".to_string(),
        source: "Legal team".to_string(),
        format: "Policy statements".to_string(),
        tone: "Formal".to_string(),
        typical_length: "Several paragraphs".to_string(),
    };
    generator.set_variable_spec("DOCUMENTS", doc_spec);

    let question_spec = VariableSpec {
        name: "QUESTION".to_string(),
        description: "Customer question".to_string(),
        source: "End users".to_string(),
        format: "Natural language".to_string(),
        tone: "Conversational, informal".to_string(),
        typical_length: "1-2 sentences".to_string(),
    };
    generator.set_variable_spec("QUESTION", question_spec);

    let planning = generator.generate_planning();

    assert!(planning.contains("Prompt Template Summary"));
    assert!(planning.contains("Variable Analysis"));
    assert!(planning.contains("DOCUMENTS"));
    assert!(planning.contains("QUESTION"));
    assert!(planning.contains("Legal team"));
    assert!(planning.contains("End users"));
    assert!(planning.contains("Formal"));
    assert!(planning.contains("Conversational, informal"));
}

#[test]
fn test_create_synthetic_test_case() {
    let template = "Answer {{QUESTION}} based on {{DOCUMENTS}}";
    let generator = SyntheticTestDataGenerator::new(template);

    let mut values = HashMap::new();
    values.insert(
        "QUESTION".to_string(),
        "Can I return my item after 30 days?".to_string(),
    );
    values.insert(
        "DOCUMENTS".to_string(),
        "Return Policy: Items must be returned within 30 days.".to_string(),
    );

    let test_case = generator.create_test_case(values);

    assert_eq!(test_case.variables.len(), 2);
    assert!(test_case.variables["QUESTION"].contains("return"));
    assert!(test_case.planning.contains("Variable Analysis"));
}

#[test]
fn test_construct_example_block_for_multishot() {
    let template = "Classify sentiment: {{TEXT}}";
    let mut values = HashMap::new();
    values.insert("TEXT".to_string(), "I love this product!".to_string());
    let output = "Positive";

    let example = construct_example_block(template, &values, output);

    assert!(example.contains("<example>"));
    assert!(example.contains("<input>"));
    assert!(example.contains("<output>"));
    assert!(example.contains("I love this product!"));
    assert!(example.contains("Positive"));
}

#[test]
fn test_multiple_example_construction() {
    let template = "Translate: {{TEXT}} to {{LANGUAGE}}";

    let examples = vec![
        (vec![("TEXT", "Hello"), ("LANGUAGE", "Spanish")], "Hola"),
        (
            vec![("TEXT", "Goodbye"), ("LANGUAGE", "French")],
            "Au revoir",
        ),
        (vec![("TEXT", "Thank you"), ("LANGUAGE", "German")], "Danke"),
    ];

    let mut all_examples = String::new();
    for (pairs, output) in examples {
        let mut values = HashMap::new();
        for (key, value) in pairs {
            values.insert(key.to_string(), value.to_string());
        }
        all_examples.push_str(&construct_example_block(template, &values, output));
    }

    assert!(all_examples.contains("<example>"));
    assert_eq!(all_examples.split("<example>").count(), 4); // 3 examples + 1 empty at start
}

#[test]
fn test_customer_service_scenario_generation() {
    // Simulates the ACME corporation customer service scenario from cookbook
    let template = r#"You are a customer service agent for ACME Corp.

Use these policy documents to answer customer questions:
{{DOCUMENTS}}

Customer question:
{{QUESTION}}

Provide a helpful response."#;

    let mut generator = SyntheticTestDataGenerator::new(template);

    // Set realistic variable specs for customer service
    let doc_spec = VariableSpec {
        name: "DOCUMENTS".to_string(),
        description: "Company policies and FAQs".to_string(),
        source: "ACME policy/legal team".to_string(),
        format: "Structured entries with headers".to_string(),
        tone: "Professional, formal".to_string(),
        typical_length: "300-500 words".to_string(),
    };
    generator.set_variable_spec("DOCUMENTS", doc_spec);

    let question_spec = VariableSpec {
        name: "QUESTION".to_string(),
        description: "Customer inquiry".to_string(),
        source: "End users/customers".to_string(),
        format: "Natural language question".to_string(),
        tone: "Informal, conversational".to_string(),
        typical_length: "20-50 words".to_string(),
    };
    generator.set_variable_spec("QUESTION", question_spec);

    let planning = generator.generate_planning();

    assert!(planning.contains("ACME policy/legal team"));
    assert!(planning.contains("End users/customers"));
    assert!(planning.contains("Professional, formal"));
    assert!(planning.contains("Informal, conversational"));
}

#[test]
fn test_iterative_refinement_concept() {
    // Simulates iterative refinement: start with basic, then refine
    let template = "Analyze {{CODE}} written in {{LANGUAGE}}";

    // Initial basic specs
    let mut generator = SyntheticTestDataGenerator::new(template);
    let planning_v1 = generator.generate_planning();

    // Refine with more specific requirements
    let code_spec = VariableSpec {
        name: "CODE".to_string(),
        description: "Source code snippet".to_string(),
        source: "Developer".to_string(),
        format: "Code block with syntax".to_string(),
        tone: "Technical".to_string(),
        typical_length: "10-50 lines".to_string(),
    };
    generator.set_variable_spec("CODE", code_spec);

    let lang_spec = VariableSpec {
        name: "LANGUAGE".to_string(),
        description: "Programming language".to_string(),
        source: "Developer".to_string(),
        format: "Language name".to_string(),
        tone: "Technical identifier".to_string(),
        typical_length: "1 word".to_string(),
    };
    generator.set_variable_spec("LANGUAGE", lang_spec);

    let planning_v2 = generator.generate_planning();

    // V2 should be more detailed
    assert!(planning_v2.len() > planning_v1.len());
    assert!(planning_v2.contains("10-50 lines"));
    assert!(planning_v2.contains("Technical"));
}

#[test]
fn test_synthetic_test_case_with_conversation() {
    // Generate test cases for conversational AI
    let template = r#"System: You are a helpful assistant.
User: {{USER_INPUT}}
Assistant: {{ASSISTANT_RESPONSE}}"#;

    let mut generator = SyntheticTestDataGenerator::new(template);

    let user_spec = VariableSpec {
        name: "USER_INPUT".to_string(),
        description: "User message".to_string(),
        source: "Human user".to_string(),
        format: "Natural language".to_string(),
        tone: "Varies (question, command, casual)".to_string(),
        typical_length: "5-30 words".to_string(),
    };
    generator.set_variable_spec("USER_INPUT", user_spec);

    let assistant_spec = VariableSpec {
        name: "ASSISTANT_RESPONSE".to_string(),
        description: "AI response".to_string(),
        source: "AI assistant".to_string(),
        format: "Natural language".to_string(),
        tone: "Helpful, polite".to_string(),
        typical_length: "10-100 words".to_string(),
    };
    generator.set_variable_spec("ASSISTANT_RESPONSE", assistant_spec);

    let mut values = HashMap::new();
    values.insert(
        "USER_INPUT".to_string(),
        "What's the weather like today?".to_string(),
    );
    values.insert(
        "ASSISTANT_RESPONSE".to_string(),
        "I don't have access to real-time weather data.".to_string(),
    );

    let test_case = generator.create_test_case(values);

    assert!(test_case.variables["USER_INPUT"].contains("weather"));
    assert!(test_case.planning.contains("Helpful, polite"));
    assert!(test_case.planning.contains("Varies"));
}

#[test]
fn test_golden_example_construction() {
    // Creating golden examples for multishot learning
    let template = "Sentiment analysis: {{TEXT}}";

    let golden_examples = vec![
        ("This product is amazing!", "Positive"),
        ("Terrible experience, would not recommend.", "Negative"),
        ("It's okay, nothing special.", "Neutral"),
        ("Absolutely love it! Best purchase ever.", "Positive"),
        ("Waste of money. Very disappointed.", "Negative"),
    ];

    let mut few_shot_block = String::new();
    few_shot_block.push_str("<few_shot_examples>\n");

    for (text, sentiment) in golden_examples {
        let mut values = HashMap::new();
        values.insert("TEXT".to_string(), text.to_string());
        few_shot_block.push_str(&construct_example_block(template, &values, sentiment));
    }

    few_shot_block.push_str("</few_shot_examples>\n");

    // Verify structure
    assert!(few_shot_block.contains("<few_shot_examples>"));
    assert!(few_shot_block.contains("This product is amazing!"));
    assert!(few_shot_block.contains("Positive"));
    assert!(few_shot_block.contains("Negative"));
    assert!(few_shot_block.contains("Neutral"));

    // Count examples
    assert_eq!(few_shot_block.split("<example>").count() - 1, 5);
}

#[test]
fn test_variable_extraction_with_nested_braces() {
    // Edge case: templates with code blocks containing braces
    let template = r#"Analyze this code:
```rust
fn main() {
    let x = {{VALUE}};
}
```
Language: {{LANGUAGE}}"#;

    let vars = extract_variables(template);
    assert_eq!(vars.len(), 2);
    assert!(vars.contains(&"VALUE".to_string()));
    assert!(vars.contains(&"LANGUAGE".to_string()));
}

#[test]
fn test_empty_template_handling() {
    let generator = SyntheticTestDataGenerator::new("");
    assert_eq!(generator.variables.len(), 0);

    let planning = generator.generate_planning();
    assert!(planning.contains("Prompt Template Summary"));
}

#[test]
fn test_complex_multivariable_scenario() {
    // RAG-style template with many variables
    let template = r#"You are a {{ROLE}} at {{COMPANY}}.

Context: {{CONTEXT}}

Task: {{TASK}}

Constraints:
{{CONSTRAINTS}}

Output format: {{OUTPUT_FORMAT}}"#;

    let vars = extract_variables(template);
    assert_eq!(vars.len(), 6);

    let mut generator = SyntheticTestDataGenerator::new(template);

    // Set realistic specs for each variable
    let specs = vec![
        (
            "ROLE",
            "Job title",
            "System config",
            "Single word",
            "Neutral",
            "1-3 words",
        ),
        (
            "COMPANY",
            "Organization name",
            "System config",
            "Proper noun",
            "Neutral",
            "1-5 words",
        ),
        (
            "CONTEXT",
            "Background information",
            "Knowledge base",
            "Paragraphs",
            "Informative",
            "100-500 words",
        ),
        (
            "TASK",
            "What to do",
            "User input",
            "Imperative",
            "Direct",
            "1-3 sentences",
        ),
        (
            "CONSTRAINTS",
            "Limitations",
            "System config",
            "Bulleted list",
            "Formal",
            "3-10 items",
        ),
        (
            "OUTPUT_FORMAT",
            "Response structure",
            "System config",
            "Format spec",
            "Neutral",
            "1-2 lines",
        ),
    ];

    for (name, desc, source, format, tone, length) in specs {
        generator.set_variable_spec(
            name,
            VariableSpec {
                name: name.to_string(),
                description: desc.to_string(),
                source: source.to_string(),
                format: format.to_string(),
                tone: tone.to_string(),
                typical_length: length.to_string(),
            },
        );
    }

    let planning = generator.generate_planning();

    // Verify all variables are documented
    assert!(planning.contains("ROLE"));
    assert!(planning.contains("COMPANY"));
    assert!(planning.contains("CONTEXT"));
    assert!(planning.contains("TASK"));
    assert!(planning.contains("CONSTRAINTS"));
    assert!(planning.contains("OUTPUT_FORMAT"));

    // Verify realistic details
    assert!(planning.contains("Knowledge base"));
    assert!(planning.contains("User input"));
}
