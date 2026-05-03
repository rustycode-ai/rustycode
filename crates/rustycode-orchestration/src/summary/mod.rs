//! Result summarization for tool outputs.
//!
//! Distills verbose tool outputs (bash, JSON, file content, etc.) into concise
//! summaries before feeding them back to the LLM, reducing token consumption
//! while preserving actionable information.

pub mod result_summarizer;
pub mod summary_config;

pub use result_summarizer::ResultSummarizer;
pub use summary_config::SummaryConfig;
