// Re-export the JSONC parser and error types
mod parser;

pub use parser::{JsoncParser, ParseError};
