use rustycode_llm::provider::ChatMessage;

/// Result of running a headless task.
pub struct HeadlessTaskResult {
    /// The final text response from the LLM.
    pub final_text: String,
    /// Conversation messages from this iteration (for carry-forward across retries).
    /// Contains the full message history so the next iteration can continue
    /// from where this one left off instead of starting from scratch.
    pub messages: Vec<ChatMessage>,
    /// Total token usage from this iteration.
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    /// Cache token usage from this iteration.
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
}
