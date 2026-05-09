//! Headless agent loop for `run --auto` mode.
//!
//! Drives the LLM → tool call → result → LLM cycle without a TUI.
//! This is a minimal version of the TUI's streaming response loop,
//! stripped of UI concerns (caching, undo snapshots, etc.).
#![allow(dead_code, unused_imports)]

use anyhow::{Context, Result};
use futures::StreamExt;
use rustycode_llm::provider::{ChatMessage, CompletionRequest, LLMProvider, MessageRole};
use rustycode_protocol::{ContentBlock, MessageContent};
use serde_json;
use std::path::Path;

use crate::runtime::monitor::{
    detect_and_truncate_repeated_blocks, strip_repeated_preamble_phrases,
};

pub mod callbacks;
pub mod config;
pub mod events;
pub mod helpers;
pub mod heuristics;
pub mod hints;
pub mod runner;
pub mod types;
pub mod utils;

pub(crate) use self::callbacks::HeadlessStreamCallbacks;
pub(crate) use self::config::*;
pub(crate) use self::helpers::normalize_tool_name;
pub use self::helpers::{
    dispatch_agent_action, enrich_tool_output, enrich_tool_output_with_args, execute_headless_tool,
    summarize_tool_args, summarize_tool_args_for,
};
pub(crate) use self::heuristics::{
    detect_tool_loop, strip_repeated_prefix, text_indicates_completion, text_indicates_giving_up,
};
pub use self::runner::{run_headless_task, run_headless_task_core};
pub use self::types::HeadlessTaskResult;
pub use self::utils::{clean_assistant_text, prune_messages};
use rustycode_tools::ToolRegistry;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_assistant_text_removes_tool_markers() {
        let input =
            "Here is my analysis.\n[Tool use]\n[tool_result:bash:abc123] hello\nMore text.\n";
        let cleaned = clean_assistant_text(input);
        assert!(!cleaned.contains("[Tool use]"));
        assert!(!cleaned.contains("[tool_result:"));
        assert!(cleaned.contains("Here is my analysis."));
        assert!(cleaned.contains("More text."));
    }

    #[test]
    fn test_normalize_tool_name_aliases() {
        assert_eq!(normalize_tool_name("Edit"), "Edit");
        assert_eq!(normalize_tool_name("Read"), "Read");
        assert_eq!(normalize_tool_name("Write"), "Write");
        assert_eq!(normalize_tool_name("Create"), "Write");
        assert_eq!(normalize_tool_name("Bash"), "Bash");
        assert_eq!(normalize_tool_name("Shell"), "Bash");
        assert_eq!(normalize_tool_name("Grep"), "Grep");
        assert_eq!(normalize_tool_name("Search"), "Grep");
        assert_eq!(normalize_tool_name("Glob"), "Glob");
        assert_eq!(normalize_tool_name("Find"), "Glob");
        // Unknown names pass through unchanged
        assert_eq!(normalize_tool_name("Edit"), "Edit");
        assert_eq!(normalize_tool_name("WebFetch"), "WebFetch");
    }

    #[test]
    fn test_clean_assistant_text_preserves_normal_text() {
        let input = "Hello world\nThis is normal text\nNo markers here";
        let cleaned = clean_assistant_text(input);
        assert_eq!(cleaned, input);
    }

    #[test]
    fn test_strip_repeated_prefix_removes_duplicated_intro() {
        let previous = "I'll help you build a gRPC KV store server.\n\
                        Let me start by planning:\n\
                        1. Install grpcio\n\
                        2. Create proto file\n\
                        Now let me begin:";
        let current = "I'll help you build a gRPC KV store server.\n\
                       Let me start by planning:\n\
                       1. Install grpcio\n\
                       2. Create proto file\n\
                       Now let me begin:\n\
                       The server is running on port 5328.";
        let result = strip_repeated_prefix(current, previous);
        // Should strip the matching 5 lines, keep only the new content
        assert!(
            !result.contains("I'll help you"),
            "Should strip intro: {}",
            result
        );
        assert!(
            result.contains("running on port 5328"),
            "Should keep new content: {}",
            result
        );
    }

    #[test]
    fn test_strip_repeated_prefix_keeps_short_matches() {
        // Less than 3 matching lines should not strip
        let previous = "Hello\nWorld";
        let current = "Hello\nWorld\nNew content here";
        let result = strip_repeated_prefix(current, previous);
        assert_eq!(result, current, "Should not strip with <3 matching lines");
    }

    #[test]
    fn test_strip_repeated_prefix_handles_empty() {
        assert_eq!(strip_repeated_prefix("", "some text"), "");
        assert_eq!(strip_repeated_prefix("some text", ""), "some text");
        assert_eq!(strip_repeated_prefix("", ""), "");
    }

    #[test]
    fn test_strip_repeated_preamble_removes_repeated_sentences() {
        // GLM pattern: same sentence before every tool call within a single turn
        let input = "I'll help you compile and install pyknotid with NumPy 2.3.0 compatibility. \
            Let me start by examining the current state and fixing the compatibility issues. \
            First, let me grep for patterns. \
            I'll help you compile and install pyknotid with NumPy 2.3.0 compatibility. \
            Let me start by examining the current state and fixing the compatibility issues. \
            Now I'll fix the files. \
            I'll help you compile and install pyknotid with NumPy 2.3.0 compatibility. \
            Let me start by examining the current state and fixing the compatibility issues. \
            Done!";
        let result = strip_repeated_preamble_phrases(input);
        // The repeated 2-sentence preamble should appear only once
        let count = result.matches("I'll help you compile").count();
        assert_eq!(
            count, 1,
            "Should keep only one occurrence of repeated preamble"
        );
        assert!(
            result.contains("First, let me grep"),
            "Should keep non-repeated content"
        );
        assert!(result.contains("Done!"), "Should keep the ending");
    }

    #[test]
    fn test_strip_repeated_preamble_keeps_unique_text() {
        let input = "This is unique text. And another sentence. No repeats here.";
        let result = strip_repeated_preamble_phrases(input);
        assert_eq!(result, input, "Should not modify text without repeats");
    }

    #[test]
    fn test_detect_repeated_blocks_truncates_loop() {
        // Simulates the regex-log pattern: same 4-paragraph block repeated many times
        let block = "I'll validate the date components carefully, ensuring month and day ranges \
            are correct while preventing unintended matches through strategic boundary checks.\n\n\
            The regex uses negative lookarounds to prevent digit adjacency, ensuring precise \
            date pattern matching.\n\n\
            The IPv4 address validation follows a similar precise approach, checking each \
            octet's range and preventing leading zeros while allowing single zero values.\n\n\
            The regex strategy involves a positive lookahead to confirm an IP address exists \
            in the line, then greedily consuming content to match the final date.";

        // Repeat the block 5 times (above the 3-repetition threshold)
        let repeated = [block; 5].join("\n\n");
        let result = detect_and_truncate_repeated_blocks(&repeated);

        assert!(result.is_some(), "Should detect repetition");
        let truncated = result.unwrap();
        assert!(
            truncated.len() < repeated.len() / 3,
            "Should truncate to roughly one block: {} vs {}",
            truncated.len(),
            repeated.len()
        );
        // Should contain exactly one copy of the block
        assert_eq!(
            truncated.matches("date components").count(),
            1,
            "Should have exactly one copy: {}",
            truncated
        );
    }

    #[test]
    fn test_detect_repeated_blocks_preserves_normal_text() {
        let normal = "First paragraph about the problem.\n\n\
            Second paragraph with analysis.\n\n\
            Third paragraph with solution.\n\n\
            Fourth paragraph with verification.\n\n\
            Fifth paragraph about next steps.";
        let result = detect_and_truncate_repeated_blocks(normal);
        assert!(result.is_none(), "Should not truncate non-repeating text");
    }

    #[test]
    fn test_detect_repeated_blocks_with_trailing_content() {
        // Use a 4-paragraph block repeated 4 times (meets the 3-repetition threshold)
        let block = "The regex uses negative lookarounds to prevent digit adjacency.\n\n\
            The IPv4 address validation checks each octet's range precisely.\n\n\
            The regex strategy involves a positive lookahead to confirm IP exists.\n\n\
            The key is balancing precise validation with flexible matching.";
        // 4 repetitions + unique trailing content
        let text = format!(
            "{}\n\n{}\n\n{}\n\n{}\n\nFinal unique conclusion here.",
            block, block, block, block
        );
        let result = detect_and_truncate_repeated_blocks(&text);
        assert!(
            result.is_some(),
            "Should detect repetition with trailing content"
        );
        let truncated = result.unwrap();
        assert!(
            truncated.contains("Final unique conclusion"),
            "Should preserve trailing non-repeating content: {}",
            truncated
        );
    }

    /// Verify the message construction at each turn of the headless agent loop.
    /// This test simulates the exact flow: user task → assistant+tool → user(tool_result) → ...
    /// and verifies:
    /// 1. No consecutive same-role messages (Anthropic API rejects these)
    /// 2. System prompt is NOT in the messages array (goes via system_prompt field)
    /// 3. Tool results have matching tool_use_ids
    /// 4. Message content serializes to valid Anthropic API format
    #[test]
    fn test_message_construction_turn_by_turn() {
        // === Turn 0: Initial state ===
        // Headless mode starts with user task as messages[0], system prompt via system_prompt field
        let task = "Fix the bug in main.py";
        let task_with_context = format!(
            "Working directory: /workspace (contains 5 files/dirs)\n\nmain.py\n\n---\n\n{}",
            task
        );

        let mut messages: Vec<ChatMessage> = vec![ChatMessage::user(task_with_context.clone())];

        // Verify: only 1 message, role=User
        assert_eq!(messages.len(), 1, "Turn 0: should have 1 message");
        assert_eq!(
            messages[0].role,
            MessageRole::User,
            "Turn 0: first message should be User role"
        );
        assert!(matches!(&messages[0].content, MessageContent::Simple(t) if t.contains(task)));

        // === Turn 1: LLM responds with text + tool_use ===
        // Simulate the assistant response with text + bash tool call
        let assistant_text = "I'll fix the bug. Let me read the file first.";
        let tool_id_1 = "toolu_abc123";
        let tool_name_1 = "Bash";
        let _tool_json_1 = r#"{"command": "cat main.py"}"#;

        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        assistant_blocks.push(ContentBlock::text(assistant_text));
        assistant_blocks.push(ContentBlock::ToolUse {
            id: tool_id_1.to_string(),
            name: tool_name_1.to_string(),
            input: serde_json::json!({"command": "cat main.py"}),
        });

        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(assistant_blocks),
        });

        // Verify: messages[1] is Assistant with blocks
        assert_eq!(messages.len(), 2, "Turn 1: should have 2 messages");
        assert_eq!(
            messages[1].role,
            MessageRole::Assistant,
            "Turn 1: second message should be Assistant"
        );
        if let MessageContent::Blocks(blocks) = &messages[1].content {
            assert_eq!(
                blocks.len(),
                2,
                "Turn 1: assistant should have text + tool_use blocks"
            );
            assert!(blocks[0].is_text(), "Turn 1: first block should be text");
            assert!(
                blocks[1].is_tool_use(),
                "Turn 1: second block should be tool_use"
            );
        } else {
            panic!(
                "Turn 1: assistant content should be Blocks, got {:?}",
                messages[1].content
            );
        }

        // === Turn 1 continued: Tool results as user message ===
        let tool_output_1 = "def main():\n    print('hello')\n";

        let tool_result_blocks: Vec<ContentBlock> =
            vec![ContentBlock::tool_result(tool_id_1, tool_output_1)];

        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(tool_result_blocks),
        });

        // Verify: messages[2] is User with tool_result
        assert_eq!(
            messages.len(),
            3,
            "Turn 1: should have 3 messages after tool result"
        );
        assert_eq!(
            messages[2].role,
            MessageRole::User,
            "Turn 1: tool results should be User role"
        );
        if let MessageContent::Blocks(blocks) = &messages[2].content {
            assert_eq!(blocks.len(), 1, "Turn 1: should have 1 tool_result block");
            // Verify the tool_result has matching tool_use_id
            if let ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } = &blocks[0]
            {
                assert_eq!(tool_use_id, tool_id_1, "Turn 1: tool_use_id should match");
                assert_eq!(
                    content, tool_output_1,
                    "Turn 1: tool result content should match"
                );
            } else {
                panic!("Turn 1: block should be ToolResult");
            }
        } else {
            panic!("Turn 1: tool results should be Blocks");
        }

        // === Verify role alternation so far ===
        let roles: Vec<String> = messages.iter().map(|m| format!("{:?}", m.role)).collect();
        assert_eq!(
            roles,
            vec!["User", "Assistant", "User"],
            "Turn 1: roles should alternate User-Assistant-User, got {:?}",
            roles
        );

        // === Turn 2: LLM responds with text + edit_file tool ===
        let tool_id_2 = "toolu_def456";
        let edit_input = serde_json::json!({
            "path": "main.py",
            "old_string": "print('hello')",
            "new_string": "print('world')"
        });

        let assistant_blocks_2: Vec<ContentBlock> = vec![
            ContentBlock::text("I'll fix the print statement."),
            ContentBlock::ToolUse {
                id: tool_id_2.to_string(),
                name: "Edit".to_string(),
                input: edit_input,
            },
        ];

        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(assistant_blocks_2),
        });

        // Turn 2 tool results
        let tool_output_2 = "File edited successfully (line 2)";

        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::tool_result(
                tool_id_2,
                tool_output_2,
            )]),
        });

        // Verify role alternation
        let roles: Vec<String> = messages.iter().map(|m| format!("{:?}", m.role)).collect();
        assert_eq!(
            roles,
            vec!["User", "Assistant", "User", "Assistant", "User"],
            "Turn 2: roles should alternate correctly, got {:?}",
            roles
        );

        // === Verify serialization of each message to JSON ===
        // This catches issues with the serde derive that would cause API errors
        for (i, msg) in messages.iter().enumerate() {
            let json = serde_json::to_value(msg)
                .unwrap_or_else(|e| panic!("Message {} failed to serialize: {}", i, e));

            // Every message must have a role
            assert!(
                json.get("role").is_some(),
                "Message {} missing role field",
                i
            );

            // Every message must have content
            assert!(
                json.get("content").is_some(),
                "Message {} missing content field",
                i
            );
        }

        // === Test course correction message ===
        // After a force_stop, a warning is injected as a User message
        let warning = "WARNING: You appear to be stuck in a loop. Please try a different approach.";
        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(warning.to_string()),
        });

        // Verify: course correction doesn't break role alternation
        // The previous message was User (tool_result), so this would be two consecutive User messages
        // This is actually valid for Anthropic — the API allows consecutive same-role messages
        // but it's worth noting
        assert_eq!(
            messages.len(),
            6,
            "Should have 6 messages after course correction"
        );
        assert_eq!(
            messages[5].role,
            MessageRole::User,
            "Course correction should be User role"
        );

        // === Test message pruning ===
        let pruned = prune_messages(messages.clone());
        // After pruning, the first message (user task) should still be there
        assert!(!pruned.is_empty(), "Pruned messages should not be empty");
        assert_eq!(
            pruned[0].role,
            MessageRole::User,
            "First pruned message should be User task"
        );
        if let MessageContent::Simple(t) = &pruned[0].content {
            assert!(
                t.contains(task),
                "First pruned message should contain the task"
            );
        } else {
            panic!("First pruned message should be Simple text");
        }
    }

    /// Verify that tool result blocks have the correct serialized format for Anthropic API.
    /// The API expects: {"type": "tool_result", "tool_use_id": "...", "content": "..."}
    #[test]
    fn test_tool_result_serialization_format() {
        let block = ContentBlock::tool_result("toolu_abc123", "hello world");
        let json = serde_json::to_value(&block).expect("Failed to serialize tool_result");

        assert_eq!(
            json["type"], "tool_result",
            "type field should be 'tool_result'"
        );
        assert_eq!(
            json["tool_use_id"], "toolu_abc123",
            "tool_use_id should match"
        );
        assert_eq!(json["content"], "hello world", "content should match");
        // is_error should be absent (skip_serializing_if = false)
        assert!(
            json.get("is_error").is_none(),
            "is_error should not be present when false"
        );
    }

    /// Verify tool_use block serialization format.
    #[test]
    fn test_tool_use_serialization_format() {
        let block = ContentBlock::ToolUse {
            id: "toolu_xyz789".to_string(),
            name: "Bash".to_string(),
            input: serde_json::json!({"command": "echo hello"}),
        };
        let json = serde_json::to_value(&block).expect("Failed to serialize tool_use");

        assert_eq!(json["type"], "tool_use", "type field should be 'tool_use'");
        assert_eq!(json["id"], "toolu_xyz789", "id should match");
        assert_eq!(json["name"], "Bash", "name should match");
        assert_eq!(json["input"]["command"], "echo hello", "input should match");
    }

    /// Verify text block serialization format.
    #[test]
    fn test_text_block_serialization_format() {
        let block = ContentBlock::text("Hello world");
        let json = serde_json::to_value(&block).expect("Failed to serialize text block");

        assert_eq!(json["type"], "text", "type field should be 'text'");
        assert_eq!(json["text"], "Hello world", "text should match");
        // cache_control should be absent (skip_serializing_if = None)
        assert!(
            json.get("cache_control").is_none(),
            "cache_control should not be present when None"
        );
    }

    /// Verify the error tool_result format.
    #[test]
    fn test_tool_error_serialization_format() {
        let block = ContentBlock::tool_error("toolu_err123", "Command failed: exit code 1");
        let json = serde_json::to_value(&block).expect("Failed to serialize tool_error");

        assert_eq!(
            json["type"], "tool_result",
            "type field should be 'tool_result'"
        );
        assert_eq!(
            json["tool_use_id"], "toolu_err123",
            "tool_use_id should match"
        );
        assert_eq!(
            json["content"], "Command failed: exit code 1",
            "content should match"
        );
        assert_eq!(json["is_error"], true, "is_error should be true");
    }

    /// Simulate multi-turn conversation with multiple tools per turn
    /// and verify message structure integrity.
    #[test]
    fn test_multi_tool_per_turn_message_structure() {
        let mut messages: Vec<ChatMessage> = vec![ChatMessage::user("Fix all the bugs")];

        // Assistant responds with 3 tool calls in one turn
        let tools = vec![
            ("toolu_1", "Bash", r#"{"command": "grep -r BUG src/"}"#),
            ("toolu_2", "Read", r#"{"path": "src/main.rs"}"#),
            ("toolu_3", "Bash", r#"{"command": "cargo test"}"#),
        ];

        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        assistant_blocks.push(ContentBlock::text("Let me investigate the bugs."));

        for (id, name, _json) in &tools {
            assistant_blocks.push(ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({"command": format!("cmd for {}", id)}),
            });
        }

        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(assistant_blocks),
        });

        // All 3 tool results go into a single User message
        let mut result_blocks: Vec<ContentBlock> = Vec::new();
        for (id, _, _) in &tools {
            result_blocks.push(ContentBlock::tool_result(*id, "result output"));
        }

        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(result_blocks),
        });

        // Verify structure
        assert_eq!(messages.len(), 3, "Should have 3 messages");

        // Assistant message should have 4 blocks (1 text + 3 tool_use)
        if let MessageContent::Blocks(blocks) = &messages[1].content {
            assert_eq!(
                blocks.len(),
                4,
                "Assistant should have 1 text + 3 tool_use = 4 blocks"
            );
            assert!(blocks[0].is_text());
            assert!(blocks[1].is_tool_use());
            assert!(blocks[2].is_tool_use());
            assert!(blocks[3].is_tool_use());
        } else {
            panic!("Assistant content should be Blocks");
        }

        // User tool_result message should have 3 blocks
        if let MessageContent::Blocks(blocks) = &messages[2].content {
            assert_eq!(blocks.len(), 3, "User should have 3 tool_result blocks");
            for (i, block) in blocks.iter().enumerate() {
                if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                    assert_eq!(
                        tool_use_id, tools[i].0,
                        "tool_use_id should match at index {}",
                        i
                    );
                } else {
                    panic!("Block {} should be ToolResult", i);
                }
            }
        } else {
            panic!("User tool results should be Blocks");
        }

        // Verify all messages serialize cleanly
        for (i, msg) in messages.iter().enumerate() {
            let json = serde_json::to_string(msg)
                .unwrap_or_else(|e| panic!("Message {} failed to serialize: {}", i, e));
            assert!(!json.is_empty(), "Message {} serialized to empty string", i);
        }
    }

    /// Verify that the system prompt is NOT part of the messages array
    /// (it should go through CompletionRequest::system_prompt instead)
    #[test]
    fn test_system_prompt_not_in_messages() {
        let messages: Vec<ChatMessage> = vec![ChatMessage::user("do the task")];

        // No message should have System role
        for (i, msg) in messages.iter().enumerate() {
            assert_ne!(
                msg.role,
                MessageRole::System,
                "Message {} should not be System role — system prompts go via system_prompt field",
                i
            );
        }
    }

    /// Verify the is_modifying helper correctly identifies file-modifying commands.
    #[test]
    fn test_is_modifying_detects_write_tools() {
        let is_modifying = |name: &str, json: &str| -> bool {
            if name == "Write" || name == "Edit" || name == "apply_patch" {
                return true;
            }
            if name == "Bash" {
                let cmd = json.to_lowercase();
                if cmd.contains("sed -i") || cmd.contains("awk -i") || cmd.contains("awk --inplace")
                {
                    return true;
                }
                if cmd.contains("> ")
                    || cmd.contains(">>")
                    || cmd.contains("cat >")
                    || cmd.contains("tee ")
                {
                    return true;
                }
                if cmd.contains("pip install")
                    || cmd.contains("pip3 install")
                    || cmd.contains("cargo ")
                    || cmd.contains("apt-get install")
                    || cmd.contains("apt install")
                    || cmd.contains("yum install")
                    || cmd.contains("dnf install")
                    || cmd.contains("npm install")
                    || cmd.contains("yarn install")
                    || cmd.contains("pnpm install")
                    || cmd.contains("bun install")
                    || cmd.contains("go install")
                    || cmd.contains("gem install")
                {
                    return true;
                }
                if cmd.contains("make ")
                    || cmd.contains("gcc ")
                    || cmd.contains("g++")
                    || cmd.contains("cmake ")
                {
                    return true;
                }
                if cmd.contains("git add")
                    || cmd.contains("git commit")
                    || cmd.contains("git merge")
                    || cmd.contains("git checkout")
                    || cmd.contains("git clone")
                    || cmd.contains("git rebase")
                    || cmd.contains("git cherry-pick")
                    || cmd.contains("git apply")
                    || cmd.contains("git am")
                    || cmd.contains("git stash")
                    || cmd.contains("git rm")
                    || cmd.contains("git mv")
                {
                    return true;
                }
                if cmd.contains("mv ")
                    || cmd.contains("cp ")
                    || cmd.contains("rm ")
                    || cmd.contains("chmod")
                    || cmd.contains("chown")
                    || cmd.contains("mkdir ")
                    || cmd.contains("ln ")
                    || cmd.contains("install ")
                    || cmd.contains("dd ")
                {
                    return true;
                }
                if cmd.contains("python -c")
                    || cmd.contains("python3 -c")
                    || cmd.contains("perl -i")
                {
                    return true;
                }
                if cmd.contains("patch")
                    || cmd.contains("service ")
                    || cmd.contains("systemctl ")
                    || cmd.contains("nohup ")
                    || cmd.contains("setup.py ")
                    || cmd.contains("docker build")
                    || cmd.contains("docker run")
                    || cmd.contains("docker-compose")
                    || cmd.contains("docker compose")
                    || cmd.contains("tar ")
                    || cmd.contains("unzip ")
                    // curl/wget only count as modifying when downloading to a file
                    || (cmd.contains("curl ") && (cmd.contains("-o ") || cmd.contains("--output") || cmd.contains("> ") || cmd.contains("-o")))
                    || (cmd.contains("wget ") && (cmd.contains("-o ") || cmd.contains("--output") || cmd.contains("-O")))
                {
                    return true;
                }
            }
            false
        };

        // Modifying commands
        assert!(is_modifying(
            "Write",
            r#"{"path": "/app/test.py", "content": "hello"}"#
        ));
        assert!(is_modifying(
            "Edit",
            r#"{"path": "main.py", "old": "x", "new": "y"}"#
        ));
        assert!(is_modifying("apply_patch", r#"{"path": "a.py"}"#));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "sed -i 's/old/new/g' file.py"}"#
        ));
        assert!(is_modifying("Bash", r#"{"command": "pip install numpy"}"#));
        assert!(is_modifying("Bash", r#"{"command": "git add ."}"#));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "git commit -m 'fix'"}"#
        ));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "cat > file.py << 'EOF'"}"#
        ));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "python -c \"import os\""}"#
        ));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "python3 -c \"open('f','w')\""}"#
        ));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "service nginx start"}"#
        ));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "systemctl start postfix"}"#
        ));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "nohup python app.py &"}"#
        ));
        assert!(is_modifying("Bash", r#"{"command": "mkdir -p /app/data"}"#));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "echo hello > out.txt"}"#
        ));
        assert!(is_modifying("Bash", r#"{"command": "make install"}"#));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "cargo build --release"}"#
        ));
        assert!(is_modifying("Bash", r#"{"command": "git stash"}"#));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "git clone https://github.com/example/repo.git"}"#
        ));

        // Non-modifying commands
        assert!(!is_modifying("Read", r#"{"path": "/app/test.py"}"#));
        assert!(!is_modifying("Bash", r#"{"command": "ls -la"}"#));
        assert!(!is_modifying("Bash", r#"{"command": "cat file.py"}"#));
        assert!(!is_modifying(
            "Bash",
            r#"{"command": "grep -r pattern src/"}"#
        ));
        assert!(!is_modifying("Bash", r#"{"command": "git status"}"#));
        assert!(!is_modifying("Bash", r#"{"command": "git log --oneline"}"#));
        assert!(!is_modifying("Bash", r#"{"command": "pip list"}"#));
        assert!(!is_modifying("Bash", r#"{"command": "pip show numpy"}"#));
        assert!(!is_modifying("Glob", r#"{"pattern": "**/*.py"}"#));
        assert!(!is_modifying("Grep", r#"{"pattern": "TODO"}"#));
        // curl/wget checking (not downloading to file) should NOT be modifying
        assert!(!is_modifying(
            "Bash",
            r#"{"command": "curl http://localhost:8080/health"}"#
        ));
        assert!(!is_modifying(
            "Bash",
            r#"{"command": "wget -qO- http://localhost:8080/"}"#
        ));
        // curl downloading to file SHOULD be modifying
        assert!(is_modifying(
            "Bash",
            r#"{"command": "curl -o data.json http://example.com/data"}"#
        ));
        assert!(is_modifying(
            "Bash",
            r#"{"command": "wget -O data.json http://example.com/data"}"#
        ));
    }

    /// Verify the system prompt contains critical rules about not claiming completion early.
    #[test]
    fn test_system_prompt_contains_core_guidance() {
        assert!(
            HEADLESS_SYSTEM_PROMPT.contains("Task completed"),
            "System prompt should define completion criteria"
        );
        assert!(
            HEADLESS_SYSTEM_PROMPT.contains("verify"),
            "System prompt should mention verification"
        );
        assert!(
            HEADLESS_SYSTEM_PROMPT.contains("tool call"),
            "System prompt should require tool calls"
        );
    }

    /// Verify that the verification detection logic correctly identifies
    /// test/verification bash commands vs non-verification commands.
    #[test]
    fn test_verification_detection_identifies_test_commands() {
        // These should be detected as verification commands
        let verification_cmds = [
            r#"{"command": "cd /tmp && python -c \"import pyknotid\""}"#,
            r#"{"command": "pytest tests/ -v"}"#,
            r#"{"command": "python -m pytest tests/ --tb=short"}"#,
            r#"{"command": "cargo test"}"#,
            r#"{"command": "go test ./..."}"#,
            r#"{"command": "make test"}"#,
            r#"{"command": "npm test"}"#,
            r#"{"command": "python3 -c \"print('hello')\""}"#,
            r#"{"command": "node -e \"console.log('test')\""}"#,
            r#"{"command": "python /app/test_outputs.py"}"#,
            r#"{"command": "python3 /app/task_file/scripts/optimized_packer.py"}"#,
            r#"{"command": "uv run python /app/compress.py /app/c4_sample /app/test_output"}"#,
        ];

        for cmd_json in &verification_cmds {
            let cmd = cmd_json.to_lowercase();
            let is_verification = cmd.contains("pytest")
                || cmd.contains("python -c")
                || cmd.contains("python3 -c")
                || cmd.contains("node -e")
                || cmd.contains("cargo test")
                || cmd.contains("go test")
                || cmd.contains("make test")
                || cmd.contains("npm test")
                || cmd.contains("python ") && cmd.contains(".py")
                || cmd.contains("python3 ") && cmd.contains(".py")
                || cmd.contains("uv run")
                || cmd.contains("uv sync");
            assert!(
                is_verification,
                "Should detect as verification: {}",
                cmd_json
            );
        }

        // These should NOT be detected as verification commands
        let non_verification_cmds = [
            r#"{"command": "grep -r pattern src/"}"#,
            r#"{"command": "cat file.py"}"#,
            r#"{"command": "ls -la"}"#,
            r#"{"command": "git status"}"#,
            r#"{"command": "pip install numpy"}"#,
        ];

        for cmd_json in &non_verification_cmds {
            let cmd = cmd_json.to_lowercase();
            let is_verification = cmd.contains("pytest")
                || (cmd.contains("python -c") && !cmd.contains("Grep"))
                || cmd.contains("cargo test")
                || cmd.contains("go test");
            assert!(
                !is_verification,
                "Should NOT detect as verification: {}",
                cmd_json
            );
        }
    }

    #[test]
    fn test_on_stream_usage_saturating_add() {
        let total_input = std::cell::Cell::new(u64::MAX - 10u64);
        let total_output = std::cell::Cell::new(0u64);

        let usage = rustycode_llm::provider::Usage::new(100, 50);

        // Should not panic on overflow — saturating_add caps at u64::MAX
        total_input.set(total_input.get().saturating_add(usage.input_tokens as u64));
        total_output.set(
            total_output
                .get()
                .saturating_add(usage.output_tokens as u64),
        );

        assert_eq!(total_input.get(), u64::MAX); // Saturated, not wrapped
        assert_eq!(total_output.get(), 50);
    }

    #[test]
    fn test_detect_tool_loop_no_loop() {
        let tools = vec!["Read".to_string(), "Edit".to_string(), "Bash".to_string()];
        assert!(detect_tool_loop(&tools, 4).is_none());
    }

    #[test]
    fn test_detect_tool_loop_period_1() {
        let tools = vec![
            "Read".to_string(),
            "Read".to_string(),
            "Read".to_string(),
            "Read".to_string(),
        ];
        let result = detect_tool_loop(&tools, 4);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Read"));
    }

    #[test]
    fn test_detect_tool_loop_period_2() {
        let tools = vec![
            "Read".to_string(),
            "Edit".to_string(),
            "Read".to_string(),
            "Edit".to_string(),
            "Read".to_string(),
            "Edit".to_string(),
        ];
        let result = detect_tool_loop(&tools, 4);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Read -> Edit"));
    }

    #[test]
    fn test_detect_tool_loop_too_few() {
        let tools = vec!["Read".to_string()];
        assert!(detect_tool_loop(&tools, 4).is_none());
    }

    #[test]
    fn test_detect_tool_loop_breaks_on_change() {
        // Same tool 3 times, then different tool breaks the pattern
        let tools = vec![
            "Read".to_string(),
            "Read".to_string(),
            "Read".to_string(),
            "Bash".to_string(),
        ];
        assert!(detect_tool_loop(&tools, 4).is_none());
    }
}
