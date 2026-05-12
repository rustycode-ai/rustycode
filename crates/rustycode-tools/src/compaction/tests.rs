use super::*;
use rustycode_protocol::Message;
use std::time::Instant;

fn make_message(role: MessageRole, content: &str, tokens: usize) -> ConversationMessage {
    ConversationMessage {
        role,
        content: content.to_string(),
        token_count: tokens,
        timestamp: Instant::now(),
    }
}

#[test]
fn test_no_compaction_under_threshold() {
    let config = CompactionConfig {
        max_messages: 20,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    for i in 0..10 {
        let action =
            compactor.add_message(make_message(MessageRole::User, &format!("msg {i}"), 100));
        assert_eq!(action, CompactionAction::None);
    }
}

#[test]
fn test_compaction_triggers_on_message_count() {
    let config = CompactionConfig {
        max_messages: 10,
        retention_window: 3,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    for i in 0..10 {
        compactor.add_message(make_message(MessageRole::User, &format!("msg {i}"), 100));
    }

    assert!(compactor.should_compact());
}

#[test]
fn test_compact_retains_recent_messages() {
    let config = CompactionConfig {
        max_messages: 10,
        retention_window: 3,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    for i in 0..10 {
        compactor.add_message(make_message(MessageRole::User, &format!("msg {i}"), 100));
    }

    let result = compactor.compact().unwrap();
    assert_eq!(result.messages_removed, 7);
    assert_eq!(compactor.message_count(), 3);
}

#[test]
fn test_compaction_reduces_token_count() {
    let config = CompactionConfig {
        max_messages: 10,
        retention_window: 3,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    for i in 0..10 {
        compactor.add_message(make_message(MessageRole::User, &format!("msg {i}"), 100));
    }

    let _ = compactor.compact();
    assert_eq!(compactor.token_count(), 300); // 3 messages * 100 tokens each
}

#[test]
fn test_no_compact_when_few_messages() {
    let config = CompactionConfig {
        max_messages: 100,
        retention_window: 10,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    for i in 0..5 {
        compactor.add_message(make_message(MessageRole::User, &format!("msg {i}"), 100));
    }

    assert!(compactor.compact().is_none());
}

#[test]
fn test_turn_counting() {
    let config = CompactionConfig {
        max_turns: 5,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    // Add user messages (should count as turns)
    for _i in 0..3 {
        compactor.add_message(make_message(MessageRole::User, "user", 100));
    }
    assert_eq!(compactor.turn_count(), 3);

    // Add tool messages (should not count as turns)
    compactor.add_message(make_message(MessageRole::Tool, "tool", 50));
    assert_eq!(compactor.turn_count(), 3);

    // Add assistant messages (should count as turns)
    compactor.add_message(make_message(MessageRole::Assistant, "assistant", 100));
    assert_eq!(compactor.turn_count(), 4);
}

#[test]
fn test_compaction_on_turn_threshold() {
    let config = CompactionConfig {
        max_turns: 3,
        retention_window: 2,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    // Add 4 user messages (4 turns)
    for _i in 0..4 {
        compactor.add_message(make_message(MessageRole::User, "user", 100));
    }

    assert!(compactor.should_compact());
}

#[test]
fn test_compaction_on_token_threshold() {
    let config = CompactionConfig {
        max_tokens: 500,
        retention_window: 2,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    // Add messages totaling 600 tokens
    for _i in 0..6 {
        compactor.add_message(make_message(MessageRole::User, "user", 100));
    }

    assert!(compactor.should_compact());
}

#[test]
fn test_protocol_message_conversion() {
    let proto_msg = Message::user("Hello, world!");
    let conv_msg = ConversationMessage::from(proto_msg);

    assert_eq!(conv_msg.role, MessageRole::User);
    assert_eq!(conv_msg.content, "Hello, world!");
    assert!(conv_msg.token_count > 0);
}

#[test]
fn test_compaction_action_should_compact() {
    assert!(!CompactionAction::None.should_compact());
    assert!(CompactionAction::Compact.should_compact());
}

#[test]
fn test_reset() {
    let mut compactor = Compactor::new(CompactionConfig::default());

    compactor.add_message(make_message(MessageRole::User, "test", 100));
    assert_eq!(compactor.message_count(), 1);

    compactor.reset();
    assert_eq!(compactor.message_count(), 0);
    assert_eq!(compactor.token_count(), 0);
    assert_eq!(compactor.turn_count(), 0);
}

#[test]
fn test_middle_out_indices_zero_percent() {
    let indices: Vec<usize> = (0..10).collect();
    let result = Compactor::middle_out_indices(&indices, 0);
    assert!(result.is_empty());
}

#[test]
fn test_middle_out_indices_saturating_no_overflow() {
    // Verify saturating_mul prevents overflow with large remove_percent
    let indices: Vec<usize> = (0..100).collect();
    let result = Compactor::middle_out_indices(&indices, u32::MAX);
    assert!(!result.is_empty());
}

// ── Progressive Compaction Tests ──────────────────────────────────────

#[test]
fn test_progressive_compaction_removes_tool_responses() {
    let config = CompactionConfig {
        max_messages: 10,
        max_tokens: 1_000_000, // Don't trigger on tokens
        max_turns: 1_000_000,  // Don't trigger on turns
        retention_window: 2,
        progressive_compaction: true,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    // Add conversation with tool responses
    compactor.add_message(make_message(MessageRole::User, "user msg", 50));
    compactor.add_message(make_message(MessageRole::Tool, "tool result 1", 200));
    compactor.add_message(make_message(MessageRole::Tool, "tool result 2", 200));
    compactor.add_message(make_message(MessageRole::Tool, "tool result 3", 200));
    compactor.add_message(make_message(MessageRole::Assistant, "response", 50));
    compactor.add_message(make_message(MessageRole::User, "follow-up", 50));
    compactor.add_message(make_message(MessageRole::Tool, "tool result 4", 200));
    compactor.add_message(make_message(MessageRole::Assistant, "final", 50));

    // Force compaction by exceeding max_messages
    compactor.add_message(make_message(MessageRole::User, "overflow", 50));
    compactor.add_message(make_message(MessageRole::User, "overflow2", 50));

    assert!(compactor.should_compact());
    let result = compactor.compact();

    assert!(result.is_some());
    let r = result.unwrap();
    // Should have removed some tool responses
    assert!(r.messages_removed > 0);
    assert!(r.tokens_saved > 0);
    // Should use progressive strategy
    assert!(matches!(
        r.strategy,
        CompactionStrategy::ProgressiveToolRemoval(_)
    ));
}

#[test]
fn test_middle_out_indices() {
    let indices: Vec<usize> = vec![2, 5, 8, 12, 15, 20, 25];

    // Remove 50% = 3 items from middle
    let result = Compactor::middle_out_indices(&indices, 50);
    assert!(result.len() >= 3);
    // All returned values should be valid indices from the input
    for idx in &result {
        assert!(indices.contains(idx));
    }
}

#[test]
fn test_middle_out_indices_empty() {
    let indices: Vec<usize> = vec![];
    let result = Compactor::middle_out_indices(&indices, 50);
    assert!(result.is_empty());
}

#[test]
fn test_middle_out_preserves_edges() {
    let indices: Vec<usize> = vec![0, 10, 20, 30, 40, 50, 60, 70, 80, 90];

    // Remove 20% = 2 items from middle
    let result = Compactor::middle_out_indices(&indices, 20);

    // Middle-out should prefer middle indices
    // Should not remove the very first or very last
    assert!(!result.contains(&0));
    assert!(!result.contains(&90));
}

#[test]
fn test_should_auto_compact_threshold() {
    let config = CompactionConfig {
        max_tokens: 1000,
        auto_compact_threshold: 0.8,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    // Add 500 tokens (50% of 1000) - should NOT auto-compact
    compactor.add_message(make_message(MessageRole::User, "msg", 500));
    assert!(!compactor.should_auto_compact(1000));

    // Add 400 more tokens (total 900 = 90% of 1000) - SHOULD auto-compact
    compactor.add_message(make_message(MessageRole::User, "msg2", 400));
    assert!(compactor.should_auto_compact(1000));
}

#[test]
fn test_compaction_strategy_default_has_progressive() {
    let config = CompactionConfig::default();
    assert!(config.progressive_compaction);
    assert_eq!(config.auto_compact_threshold, 0.8);
}

#[test]
fn test_tool_summarization_cutoff() {
    let config = CompactionConfig::default();
    let compactor = Compactor::new(config);

    let cutoff = compactor.tool_summarization_cutoff(100_000);
    assert!(cutoff >= 10);
    assert!(cutoff <= 500);
}

#[test]
fn test_tool_ids_to_summarize_basic() {
    let config = CompactionConfig {
        max_messages: 100,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    // Add user msg + 16 tool msgs + user msg
    compactor.add_message(make_message(MessageRole::User, "start", 10));
    for i in 0..16 {
        compactor.add_message(make_message(MessageRole::Tool, &format!("tool_{}", i), 100));
    }
    compactor.add_message(make_message(MessageRole::User, "end", 10));

    // cutoff=5: 16 eligible, 16 > 5+10 → batch of 10
    let result = compactor.tool_ids_to_summarize(5, 0, DEFAULT_BATCH_SIZE);
    assert_eq!(result.len(), DEFAULT_BATCH_SIZE);
}

#[test]
fn test_tool_ids_to_summarize_protects_current_turn() {
    let config = CompactionConfig {
        max_messages: 100,
        ..Default::default()
    };
    let mut compactor = Compactor::new(config);

    // 20 tool messages
    compactor.add_message(make_message(MessageRole::User, "start", 10));
    for i in 0..20 {
        compactor.add_message(make_message(MessageRole::Tool, &format!("tool_{}", i), 100));
    }

    // Protect last 8: 12 eligible, 12 <= 2+10 → nothing
    let result = compactor.tool_ids_to_summarize(2, 8, DEFAULT_BATCH_SIZE);
    assert!(result.is_empty(), "Should not summarize when protected");

    // Protect last 7: 13 eligible, 13 > 2+10 → batch
    let result = compactor.tool_ids_to_summarize(2, 7, DEFAULT_BATCH_SIZE);
    assert_eq!(result.len(), DEFAULT_BATCH_SIZE);
}

#[test]
fn test_tool_ids_to_summarize_no_tools() {
    let config = CompactionConfig::default();
    let mut compactor = Compactor::new(config);

    compactor.add_message(make_message(MessageRole::User, "hello", 10));
    compactor.add_message(make_message(MessageRole::Assistant, "hi", 10));

    let result = compactor.tool_ids_to_summarize(5, 0, DEFAULT_BATCH_SIZE);
    assert!(result.is_empty());
}

// ── Age-Aware Compaction Stage Tests ────────────────────────────────────

#[test]
fn stage_for_age_recent_turns_are_full_fidelity() {
    let stage = CompactionStage::stage_for_age(45, 50, 10);
    assert_eq!(stage, CompactionStage::FullFidelity);
}

#[test]
fn stage_for_age_old_turns_are_history_snip() {
    let stage = CompactionStage::stage_for_age(1, 50, 10);
    assert_eq!(stage, CompactionStage::HistorySnip);
}

#[test]
fn stage_for_age_mid_old_are_context_collapse() {
    let stage = CompactionStage::stage_for_age(10, 50, 10);
    assert_eq!(stage, CompactionStage::ContextCollapse);
}

#[test]
fn stage_for_age_mid_are_microcompact() {
    let stage = CompactionStage::stage_for_age(30, 50, 10);
    assert_eq!(stage, CompactionStage::Microcompact);
}

#[test]
fn stage_for_age_short_conversation_is_full_fidelity() {
    let stage = CompactionStage::stage_for_age(1, 5, 10);
    assert_eq!(stage, CompactionStage::FullFidelity);
}

#[test]
fn stage_for_age_zero_total_is_full_fidelity() {
    let stage = CompactionStage::stage_for_age(0, 0, 10);
    assert_eq!(stage, CompactionStage::FullFidelity);
}

#[test]
fn microcompact_summarizes_tool_output() {
    let msg = ConversationMessage {
        role: MessageRole::Tool,
        content: "Very long tool output that spans many lines\nline2\nline3\nline4\nline5"
            .to_string(),
        token_count: 100,
        timestamp: Instant::now(),
    };
    let summarized = CompactionStage::microcompact_message(&msg);
    assert!(summarized.len() < msg.content.len());
    assert!(summarized.contains("[tool output"));
}

#[test]
fn microcompact_preserves_non_tool_messages() {
    let msg = ConversationMessage {
        role: MessageRole::User,
        content: "User question".to_string(),
        token_count: 5,
        timestamp: Instant::now(),
    };
    let summarized = CompactionStage::microcompact_message(&msg);
    assert_eq!(summarized, msg.content);
}

#[test]
fn context_collapse_reduces_size() {
    let msgs: Vec<ConversationMessage> = (0..10)
        .map(|i| ConversationMessage {
            role: if i % 2 == 0 {
                MessageRole::User
            } else {
                MessageRole::Assistant
            },
            content: format!("Message {i} with some content about various things"),
            token_count: 20,
            timestamp: Instant::now(),
        })
        .collect();

    let collapsed = CompactionStage::context_collapse_messages(&msgs);
    let original_total: usize = msgs.iter().map(|m| m.content.len()).sum();
    assert!(
        collapsed.len() < original_total,
        "collapsed should be shorter than original"
    );
    assert!(collapsed.contains("[context collapsed"));
}

#[test]
fn context_collapse_empty_returns_empty() {
    let collapsed = CompactionStage::context_collapse_messages(&[]);
    assert!(collapsed.is_empty());
}
