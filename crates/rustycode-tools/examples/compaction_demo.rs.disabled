//! Demonstration of the Conversation Compaction Service
//!
//! This example shows how to use the compaction service to manage
//! context window size by summarizing older messages when thresholds
//! are exceeded.

use rustycode_protocol::Message;
use rustycode_tools::compaction::{CompactionConfig, Compactor, ConversationMessage, MessageRole};
use std::time::Instant;

fn main() {
    println!("=== Conversation Compaction Demo ===\n");

    // Create a compactor with custom thresholds
    let max_tokens = 1000; // Compact after 1000 tokens
    let max_turns = 5; // Compact after 5 turns
    let max_messages = 10; // Compact after 10 messages
    let retention_window = 3; // Keep last 3 messages

    let config = CompactionConfig {
        max_tokens,
        max_turns,
        max_messages,
        retention_window,
        ..Default::default()
    };

    let mut compactor = Compactor::new(config);

    println!("Configuration:");
    println!("  Max tokens: {}", max_tokens);
    println!("  Max turns: {}", max_turns);
    println!("  Max messages: {}", max_messages);
    println!("  Retention window: {}", retention_window);
    println!();

    // Simulate a conversation
    println!("=== Simulating Conversation ===\n");

    // Add some user messages
    for i in 1..=4 {
        let msg = ConversationMessage {
            role: MessageRole::User,
            content: format!("User message {}", i),
            token_count: 100,
            timestamp: Instant::now(),
        };

        let action = compactor.add_message(msg);
        println!(
            "Added user message {}: {} messages, {} tokens, {} turns",
            i,
            compactor.message_count(),
            compactor.token_count(),
            compactor.turn_count()
        );

        if action.should_compact() {
            println!("  ⚠️  Compaction triggered!");
        }
    }

    println!();

    // Add some assistant messages
    for i in 1..=4 {
        let msg = ConversationMessage {
            role: MessageRole::Assistant,
            content: format!("Assistant response {}", i),
            token_count: 150,
            timestamp: Instant::now(),
        };

        let action = compactor.add_message(msg);
        println!(
            "Added assistant message {}: {} messages, {} tokens, {} turns",
            i,
            compactor.message_count(),
            compactor.token_count(),
            compactor.turn_count()
        );

        if action.should_compact() {
            println!("  ⚠️  Compaction triggered!");
            if let Some(result) = compactor.compact() {
                println!(
                    "  ✅ Compacted: {} messages removed, {} tokens saved",
                    result.messages_removed, result.tokens_saved
                );
                println!(
                    "  📝 Summary: {}",
                    result.summary.chars().take(100).collect::<String>()
                );
            }
            println!(
                "  After compaction: {} messages, {} tokens, {} turns",
                compactor.message_count(),
                compactor.token_count(),
                compactor.turn_count()
            );
        }
    }

    println!();

    // Demonstrate protocol Message integration
    println!("=== Protocol Message Integration ===\n");

    let proto_msg = Message::user("Hello from protocol!");
    let action = compactor.add_message_from_protocol(proto_msg);

    println!("Added protocol message:");
    println!("  Total messages: {}", compactor.message_count());
    println!("  Total tokens: {}", compactor.token_count());
    println!("  Compaction needed: {}", action.should_compact());

    println!();

    // Demonstrate threshold checking
    println!("=== Threshold Checking ===\n");

    println!("Current state:");
    println!(
        "  Messages: {}/{} (max)",
        compactor.message_count(),
        max_messages
    );
    println!("  Turns: {}/{} (max)", compactor.turn_count(), max_turns);
    println!("  Tokens: {}/{} (max)", compactor.token_count(), max_tokens);
    println!("  Should compact: {}", compactor.should_compact());

    println!();

    // Demonstrate reset
    println!("=== Reset ===\n");

    println!("Before reset: {} messages", compactor.message_count());
    compactor.reset();
    println!("After reset: {} messages", compactor.message_count());

    println!();

    // Demonstrate different compaction triggers
    println!("=== Different Compaction Triggers ===\n");

    let mut token_compactor = Compactor::new(CompactionConfig {
        max_tokens: 300,
        ..Default::default()
    });

    println!("Token-based compaction:");
    for i in 1..=4 {
        let msg = ConversationMessage {
            role: MessageRole::User,
            content: format!("Large message {}", i),
            token_count: 100,
            timestamp: Instant::now(),
        };

        let action = token_compactor.add_message(msg);
        println!(
            "  Message {}: {} tokens, should_compact={}",
            i,
            token_compactor.token_count(),
            action.should_compact()
        );
    }

    println!();

    let mut turn_compactor = Compactor::new(CompactionConfig {
        max_turns: 3,
        ..Default::default()
    });

    println!("Turn-based compaction:");
    for i in 1..=4 {
        let msg = ConversationMessage {
            role: MessageRole::User,
            content: format!("Turn {}", i),
            token_count: 10,
            timestamp: Instant::now(),
        };

        let action = turn_compactor.add_message(msg);
        println!(
            "  Turn {}: {} turns, should_compact={}",
            i,
            turn_compactor.turn_count(),
            action.should_compact()
        );
    }

    println!();

    let mut message_compactor = Compactor::new(CompactionConfig {
        max_messages: 5,
        retention_window: 2,
        ..Default::default()
    });

    println!("Message count-based compaction:");
    for i in 1..=6 {
        let msg = ConversationMessage {
            role: MessageRole::User,
            content: format!("Message {}", i),
            token_count: 10,
            timestamp: Instant::now(),
        };

        let action = message_compactor.add_message(msg);
        println!(
            "  Message {}: {} messages, should_compact={}",
            i,
            message_compactor.message_count(),
            action.should_compact()
        );

        if action.should_compact() {
            if let Some(_result) = message_compactor.compact() {
                println!(
                    "    ✅ Compacted to {} messages",
                    message_compactor.message_count()
                );
            }
        }
    }

    println!();
    println!("=== Demo Complete ===");
}
