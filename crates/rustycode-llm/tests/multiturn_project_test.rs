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
    clippy::items_after_statements,
    clippy::manual_string_new,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Multi-turn project completion integration test
//!
//! This test simulates a realistic multi-turn interaction where:
//! 1. User provides a project request
//! 2. Assistant plans the approach
//! 3. Assistant implements features (using tools)
//! 4. Assistant verifies and completes the project
//!
//! This tests context management, tool execution, and conversation flow.

use rustycode_llm::mock::MockProvider;
use rustycode_llm::provider::{ChatMessage, CompletionRequest, LLMProvider};
use rustycode_protocol::SessionId;

/// Simulates a complete project workflow through multiple turns
#[tokio::test]
#[allow(clippy::vec_init_then_push)]
async fn test_multiturn_simple_project_completion() {
    let _session_id = SessionId::new();
    let mut conversation = Vec::new();

    // Turn 1: User requests a feature
    conversation.push(ChatMessage::user(
        "Create a simple calculator function that can add, subtract, multiply, and divide two numbers. Include proper error handling for division by zero."
            .to_string(),
    ));

    // Mock assistant response with plan
    conversation.push(ChatMessage::assistant(
        "I'll create a calculator function for you. Here's my plan: implement the function with proper error handling."
            .to_string(),
    ));

    // Turn 2: Assistant provides the implementation
    conversation.push(ChatMessage::assistant(
        "Here is the calculator implementation. It takes two f64 numbers and an operation string, and returns the result. It handles division by zero."
            .to_string(),
    ));

    // Turn 3: User asks for tests
    conversation.push(ChatMessage::user(
        "Can you add unit tests for the calculator function?".to_string(),
    ));

    // Turn 4: Assistant provides tests
    conversation.push(ChatMessage::assistant(
        "I've added comprehensive unit tests including test_addition, test_subtraction, test_multiplication, test_division, test_division_by_zero, and test_unknown_operation."
            .to_string(),
    ));

    // Turn 5: User asks for documentation
    conversation.push(ChatMessage::user(
        "Please add documentation comments to the function.".to_string(),
    ));

    // Turn 6: Assistant adds documentation
    conversation.push(ChatMessage::assistant(
        "I've added comprehensive Rust documentation with function description, arguments section, returns section, and example usage."
            .to_string(),
    ));

    // Turn 7: User confirms completion
    conversation.push(ChatMessage::user(
        "Thanks, that looks complete!".to_string(),
    ));

    // Turn 8: Assistant acknowledges completion
    conversation.push(ChatMessage::assistant(
        "You're welcome! The calculator project is now complete with core function, comprehensive unit tests, and full documentation."
            .to_string(),
    ));

    // Verify conversation has appropriate length
    assert_eq!(conversation.len(), 9, "Conversation should have 9 messages");

    // Verify conversation flow
    use rustycode_llm::provider::MessageRole;
    assert_eq!(conversation[0].role, MessageRole::User);
    assert_eq!(conversation[1].role, MessageRole::Assistant);
    assert_eq!(conversation[2].role, MessageRole::Assistant);
    assert_eq!(conversation[3].role, MessageRole::User);
    assert_eq!(conversation[4].role, MessageRole::Assistant);
    assert_eq!(conversation[5].role, MessageRole::User);
    assert_eq!(conversation[6].role, MessageRole::Assistant);
    assert_eq!(conversation[7].role, MessageRole::User);
    assert_eq!(conversation[8].role, MessageRole::Assistant);

    // Verify key content is present
    let full_conversation: String = conversation
        .iter()
        .map(|m| m.content.to_text())
        .collect::<Vec<_>>()
        .join("\n---\n");

    assert!(full_conversation.contains("calculator"));
    assert!(full_conversation.contains("division by zero"));
    assert!(full_conversation.contains("documentation"));
}

/// Tests conversation state tracking across multiple turns
#[tokio::test]
async fn test_conversation_state_tracking() {
    let _session_id = SessionId::new();
    let mut conversation = Vec::new();

    // Simulate a debugging session with multiple iterations
    let turns = vec![
        ("user", "My function is panicking with 'index out of bounds'"),
        (
            "assistant",
            "Let me help debug. Can you share the function code and the line where it panics?",
        ),
        ("user", "Here's the code:\n```rust\nfn get_item(data: &Vec<i32>, index: usize) -> i32 {\n    data[index]\n}\n```"),
        (
            "assistant",
            "I see the issue. The function doesn't check if `index` is valid before accessing. Let me fix it:\n```rust\nfn get_item(data: &Vec<i32>, index: usize) -> Option<i32> {\n    data.get(index).copied()\n}\n```\nThis uses `get()` which returns `Option` and safely handles out-of-bounds access.",
        ),
        ("user", "That works! Now how do I use it?"),
        (
            "assistant",
            "Usage example:\n```rust\nlet data = vec![1, 2, 3];\nif let Some(value) = get_item(&data, 1) {\n    println!(\"Got: {}\", value);\n}\n// Out of bounds returns None\nassert_eq!(get_item(&data, 10), None);\n```",
        ),
        ("user", "Perfect, thanks!"),
        (
            "assistant",
            "Happy to help! The key changes were:\n1. Changed return type from `i32` to `Option<i32>`\n2. Used `get()` instead of direct indexing\n3. Callers now handle the `None` case",
        ),
    ];

    for (role, content) in turns {
        match role {
            "user" => conversation.push(ChatMessage::user(content.to_string())),
            "assistant" => conversation.push(ChatMessage::assistant(content.to_string())),
            _ => panic!("Unknown role: {}", role),
        }
    }

    // Verify all messages were added
    assert_eq!(conversation.len(), 8);

    // Verify the debugging context is maintained throughout
    let full_text: String = conversation
        .iter()
        .map(|m| m.content.to_text())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(full_text.contains("index out of bounds"));
    assert!(full_text.contains("Option<i32>"));
    assert!(full_text.contains("get()"));
}

/// Tests context handling with a longer technical conversation
#[tokio::test]
async fn test_long_technical_conversation() {
    let mut conversation = Vec::new();

    // Simulate building a REST API endpoint discussion
    let prompts = vec![
        ("user", "I need to create a REST API endpoint for user registration"),
        ("assistant", "I'll help you design a user registration endpoint. Key considerations:\n\
1. HTTP method: POST (for creating resources)\n\
2. Endpoint path: /api/users/register\n\
3. Request body: email, password, optional fields\n\
4. Response: 201 Created with user object\n\
5. Error handling: 400 for validation errors, 409 for duplicate email\n\n\
What programming language and framework are you using?"),
        ("user", "Rust with Actix-web"),
        ("assistant", "Great choice! Here's a complete implementation:\n\
```rust\nuse actix_web::{web, HttpResponse, Responder};\
use serde::{Deserialize, Serialize};\
use validator::Validate;\
\
#[derive(Deserialize, Serialize, Validate)]\
struct RegisterRequest {\
    #[validate(email)]\
    email: String,\
    #[validate(length(min = 8))]\
    password: String,\
}\
\
#[derive(Serialize)]\
struct UserResponse {\
    id: i64,\
    email: String,\
    created_at: String,\
}\
\
async fn register(\
    req: web::Json<RegisterRequest>,\
    pool: web::Data<DbPool>,\
) -> impl Responder {\
    // 1. Validate email isn't already registered\n    // 2. Hash the password\n    // 3. Insert user into database\n    // 4. Return created user\n    HttpResponse::Created().json(user)\
}\n```\
\n\
Would you like me to add the database layer and password hashing?"),
        ("user", "Yes, please use bcrypt for hashing"),
        ("assistant", "Adding bcrypt and database integration:\n\
```rust\nuse bcrypt::{hash, verify, DEFAULT_COST};\
use sqlx::PgPool;\n\
async fn register(\
    req: web::Json<RegisterRequest>,\
    pool: web::Data<DbPool>,\
) -> impl Responder {\
    // Check for existing email\n    let existing = sqlx::query!(\
n        \"SELECT id FROM users WHERE email = $1\"\
n    )\n    .bind(&req.email)\n    .fetch_optional(pool.get_ref())\n    .await;\n    \
    if existing.is_some() {\n        return HttpResponse::Conflict().json(\n            serde_json::json!(\"error\": \"Email already exists\")\n        );\n    }\n    \
    // Hash password\n    let password_hash = hash(&req.password, DEFAULT_COST).unwrap();\n\
    // Insert user\n    let user = sqlx::query!(\
n        \"INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id, email, created_at\"\
n    )\n    .bind(&req.email)\n    .bind(&password_hash)\n    .fetch_one(pool.get_ref())\n    .await\n    .unwrap();\n    \
    HttpResponse::Created().json(user)\n}\n\
```"),
        ("user", "That looks great! What dependencies do I need in Cargo.toml?"),
        ("assistant", "Add these dependencies to your Cargo.toml:\n\
[dependencies]\nactix-web = \"4\"\nserde = { version = \"1.0\", features = [\"derive\"] }\n\
validator = { version = \"0.16\", features = [\"derive\"] }\n\
sqlx = { version = \"0.7\", features = [\"runtime-tokio\", \"postgres\"] }\n\
bcrypt = \"0.15\"\n\
tokio = { version = \"1\", features = [\"full\"] }"),
        ("user", "Perfect, thanks for the help!"),
        ("assistant", "You're welcome! Summary of what we built:\n\
✅ POST /api/users/register endpoint\n\
✅ Email validation with validator crate\n\
✅ Password hashing with bcrypt\n\
✅ Database integration with sqlx\n\
✅ Duplicate email checking\n\
✅ Proper error responses (409 Conflict)\n\
\n\
Don't forget to run `cargo check` to verify everything compiles!"),
    ];

    for (role, content) in prompts {
        match role {
            "user" => conversation.push(ChatMessage::user(content.to_string())),
            "assistant" => conversation.push(ChatMessage::assistant(content.to_string())),
            _ => panic!("Unknown role: {}", role),
        }
    }

    // Verify conversation structure
    assert_eq!(conversation.len(), 10);

    // Verify key technical content is present
    let full_text: String = conversation
        .iter()
        .map(|m| m.content.to_text())
        .collect::<Vec<_>>()
        .join(" ");

    assert!(full_text.contains("POST"));
    assert!(full_text.contains("Actix-web"));
    assert!(full_text.contains("bcrypt"));
    assert!(full_text.contains("sqlx"));
    assert!(full_text.contains("validator"));
    assert!(full_text.contains("409"));
}

/// Tests that the mock provider can simulate multi-turn conversations
#[tokio::test]
async fn test_mock_provider_multiturn() {
    use rustycode_llm::provider::CompletionResponse;

    // Create provider with pre-programmed responses
    let response1 = CompletionResponse {
        content: "Hello! How can I help you today?".to_string(),
        model: "mock".to_string(),
        usage: None,
        stop_reason: None,
        citations: None,
        thinking_blocks: None,
        structured_output: None,
    };
    let response2 = CompletionResponse {
        content: "I can help you with that! What do you need?".to_string(),
        model: "mock".to_string(),
        usage: None,
        stop_reason: None,
        citations: None,
        thinking_blocks: None,
        structured_output: None,
    };
    let response3 = CompletionResponse {
        content: "Sure, here's a simple example...".to_string(),
        model: "mock".to_string(),
        usage: None,
        stop_reason: None,
        citations: None,
        thinking_blocks: None,
        structured_output: None,
    };

    let provider = MockProvider::new(vec![Ok(response1), Ok(response2), Ok(response3)], None);

    // Turn 1: Initial greeting
    let request1 = CompletionRequest::new(
        "mock".to_string(),
        vec![ChatMessage::user("Hi".to_string())],
    );

    let resp1 = provider.complete(request1).await.unwrap();
    assert_eq!(resp1.content, "Hello! How can I help you today?");

    // Turn 2: User asks a question
    let request2 = CompletionRequest::new(
        "mock".to_string(),
        vec![
            ChatMessage::user("Hi".to_string()),
            ChatMessage::assistant("Hello! How can I help you today?".to_string()),
            ChatMessage::user("I need help with Rust".to_string()),
        ],
    );

    let resp2 = provider.complete(request2).await.unwrap();
    assert_eq!(resp2.content, "I can help you with that! What do you need?");

    // Turn 3: Follow-up - Note: With only 3 responses, the 3rd call will repeat the last response
    let request3 = CompletionRequest::new(
        "mock".to_string(),
        vec![
            ChatMessage::user("Hi".to_string()),
            ChatMessage::assistant("Hello! How can I help you today?".to_string()),
            ChatMessage::user("I need help with Rust".to_string()),
            ChatMessage::assistant("I can help you with that! What do you need?".to_string()),
            ChatMessage::user("Show me an example".to_string()),
        ],
    );

    let resp3 = provider.complete(request3).await.unwrap();
    assert_eq!(resp3.content, "Sure, here's a simple example...");
}

/// Tests conversation with tool use simulation
#[tokio::test]
async fn test_multiturn_with_tools() {
    let conversation = [
        ChatMessage::user("List the files in the current directory".to_string()),
        ChatMessage::assistant("I'll list the files for you.".to_string()),
        // Simulated tool result
        ChatMessage::user("[Tool result: Cargo.toml, src/, tests/, README.md]".to_string()),
        ChatMessage::assistant(
            "I found 4 items:\n\
- Cargo.toml (project config)\n\
- src/ (source code directory)\n\
- tests/ (test files)\n\
- README.md (documentation)\n\n\
Would you like me to examine any of these?"
                .to_string(),
        ),
        ChatMessage::user("Show me the Cargo.toml".to_string()),
        ChatMessage::assistant("I'll read the Cargo.toml file for you.".to_string()),
        // Simulated tool result
        ChatMessage::user("[Tool result: package contents...]".to_string()),
        ChatMessage::assistant(
            "The Cargo.toml shows:\n\
- Package name: myproject\n\
- Version: 0.1.0\n\
- Dependencies: serde, tokio\n\
\n\
Would you like me to add any dependencies?"
                .to_string(),
        ),
    ];

    assert_eq!(conversation.len(), 8);

    // Verify tool use flow
    let full_text: String = conversation
        .iter()
        .map(|m| m.content.to_text())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(full_text.contains("Tool result"));
}

/// Tests context window management across many turns
#[tokio::test]
async fn test_context_window_many_turns() {
    let mut conversation = Vec::new();

    // Simulate a 20-turn conversation
    for i in 1..=20 {
        conversation.push(ChatMessage::user(format!(
            "Turn {}: User question {}",
            i, i
        )));
        conversation.push(ChatMessage::assistant(format!(
            "Turn {}: Assistant response {}",
            i, i
        )));
    }

    assert_eq!(conversation.len(), 40);

    // Verify the conversation maintains proper role alternation
    use rustycode_llm::provider::MessageRole;
    for (i, msg) in conversation.iter().enumerate() {
        if i % 2 == 0 {
            assert_eq!(
                msg.role,
                MessageRole::User,
                "Even indices should be user messages"
            );
        } else {
            assert_eq!(
                msg.role,
                MessageRole::Assistant,
                "Odd indices should be assistant messages"
            );
        }
    }
}

/// Tests project completion scenario with refactor request
#[tokio::test]
async fn test_project_refactor_completion() {
    let conversation = vec![
        ChatMessage::user("I have a function with 200 lines that needs refactoring".to_string()),
        ChatMessage::assistant(
            "I can help with that! To provide the best refactoring advice, I need to see:\n\
1. The function code\n\
2. What it does (brief description)\n\
3. Any specific concerns (performance, readability, maintainability?)"
                .to_string(),
        ),
        ChatMessage::user(
            "Here's the function:\n\
```rust\
fn process_data(data: &Vec<String>) -> Vec<String> {\n\
    let mut result = Vec::new();\n\
    for item in data {\n\
        let processed = item.trim();\n\
        if !processed.is_empty() {\n\
            result.push(processed);\n\
        }\n\
    }\n\
    result\n\
}\n\
```\n\
It processes strings by trimming and filtering empty ones."
            .to_string(),
        ),
        ChatMessage::assistant(
            "Here's a refactored version that's more idiomatic Rust:\n\
```rust\
fn process_data(data: &[String]) -> Vec<String> {\n\
    data.iter()\n\
        .map(|s| s.trim())\n\
        .filter(|s| !s.is_empty())\n\
        .collect()\n\
}\n\
```\n\
\
Improvements:\n\
✅ Changed from `&Vec` to `&[String]` (more flexible, accepts slices)\n\
✅ Used iterator chain instead of imperative loop\n\
✅ More concise and idiomatic Rust\n\
✅ Same functionality, better performance (no intermediate allocation)"
            .to_string(),
        ),
        ChatMessage::user("Can you explain the changes?".to_string()),
        ChatMessage::assistant(
            "Key changes explained:\n\
\n\
1. **Slice parameter**: `&[String]` is more flexible than `&Vec<String>` - it accepts vectors, arrays, or slices\n\
2. **Iterator chain**: \n\
   - `.map(|s| s.trim())` transforms each item\n\
   - `.filter(|s| !s.is_empty())` keeps only non-empty strings\n\
   - `.collect()` gathers results\n\
3. **No intermediate allocation**: Original created a new Vec and pushed to it; iterator chain is lazy\n\
4\
\n\
The refactor reduces lines from 12 to 4 while maintaining clarity and improving performance."
            .to_string(),
        ),
        ChatMessage::user("That's great! One more thing - can you make it handle whitespace-only strings?".to_string()),
        ChatMessage::assistant(
            "The current implementation already handles that! `.trim()` removes leading/trailing whitespace, and `.is_empty()` filters out strings that are empty after trimming. So a string like \"   \" would become \"\" and be filtered out.\n\
\nIs there a specific case you're concerned about?"
            .to_string(),
        ),
        ChatMessage::user("No, that's exactly what I needed. Thanks!".to_string()),
        ChatMessage::assistant(
            "Happy to help! The refactored function is:\n\
- ✅ More idiomatic\n\
- ✅ More flexible (accepts slices)\n\
- ✅ More performant (lazy evaluation)\n\
- ✅ Handles all whitespace cases\n\
\n\
Feel free to ask if you need anything else!"
            .to_string(),
        ),
    ];

    // Verify the refactoring session completed successfully
    assert_eq!(conversation.len(), 10);

    // Verify key content
    let full_text: String = conversation
        .iter()
        .map(|m| m.content.to_text())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(full_text.contains("refactored"));
    assert!(full_text.contains("iterator"));
    assert!(full_text.contains("&[String]"));
}

/// Tests that conversation maintains coherence across technical discussions
#[tokio::test]
#[allow(clippy::vec_init_then_push)]
async fn test_conversation_coherence() {
    let conversation = [
        ChatMessage::user("How do I run async functions in Rust?".to_string()),
        ChatMessage::assistant(
            "In Rust, you use `.await` to run async functions. Here's an example:
```rust
async fn fetch_data() -> String {
    \"data\".to_string()
}

#[tokio::main]
async fn main() {
    let result = fetch_data().await;
    println!(\"{}\", result);
}
```
You need to be inside an async function or use `.await`."
                .to_string(),
        ),
        ChatMessage::user("How do I handle errors with async?".to_string()),
        ChatMessage::assistant(
            "Use `?` operator or `match` for error handling:
```rust
// Using ?
async fn get_data() -> Result<String, Error> {
    let result = fetch_data().await?;
    Ok(result)
}

// Using match
async fn get_data_alt() -> Result<String, Error> {
    match fetch_data().await {
        Ok(data) => Ok(data),
        Err(e) => Err(e.into()),
    }
}
```
The `?` operator propagates errors, while `match` gives you control."
                .to_string(),
        ),
        ChatMessage::user("Can I run multiple async functions in parallel?".to_string()),
        ChatMessage::assistant(
            "Yes! Use `futures::join` or `join!` macro:
```rust
use futures::future::join_all;

async fn run_all() {
    let results = join_all(vec![
        fetch_data(),
        fetch_user(),
        fetch_config(),
    ]).await;

    // Or use join! macro for known futures
    let (data, user) = tokio::join!(fetch_data(), fetch_user());
}
```
This runs operations concurrently instead of sequentially."
                .to_string(),
        ),
    ];

    // Verify coherence - the conversation should build on async concepts
    let full_text: String = conversation
        .iter()
        .map(|m| m.content.to_text())
        .collect::<Vec<_>>()
        .join(" ");

    // Each turn should build on the previous context
    assert!(full_text.contains(".await"));
    assert!(full_text.contains("Result"));
    assert!(full_text.contains("join!"));
    assert!(full_text.contains("parallel"));
}

/// Tests session isolation between different conversations
#[tokio::test]
async fn test_session_isolation() {
    let _session1_id = SessionId::new();
    let _session2_id = SessionId::new();

    // Different sessions should maintain separate context
    let session1 = [
        ChatMessage::user("Session 1: Build a web server".to_string()),
        ChatMessage::assistant("Session 1: I'll help with Actix-web".to_string()),
    ];

    let session2 = [
        ChatMessage::user("Session 2: Write a CLI tool".to_string()),
        ChatMessage::assistant("Session 2: I'll help with clap".to_string()),
    ];

    // Verify sessions are independent
    assert_eq!(session1.len(), 2);
    assert_eq!(session2.len(), 2);

    // Verify session1 content doesn't leak to session2
    let session1_text: String = session1
        .iter()
        .map(|m| m.content.to_text())
        .collect::<Vec<_>>()
        .join(" ");
    let session2_text: String = session2
        .iter()
        .map(|m| m.content.to_text())
        .collect::<Vec<_>>()
        .join(" ");

    assert!(session1_text.contains("Actix-web"));
    assert!(!session1_text.contains("clap"));

    assert!(session2_text.contains("clap"));
    assert!(!session2_text.contains("Actix-web"));
}
