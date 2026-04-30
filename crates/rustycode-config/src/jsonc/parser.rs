// RustyCode JSONC Parser
//
// Handles parsing of JSON with comments (JSONC) and trailing commas.

use serde_json::Value;
use std::result::Result as StdResult;

pub struct JsoncParser {
    #[allow(dead_code)] // Kept for future use
    allow_comments: bool,
    allow_trailing_commas: bool,
}

impl Default for JsoncParser {
    fn default() -> Self {
        Self::new()
    }
}

impl JsoncParser {
    pub const fn new() -> Self {
        Self {
            allow_comments: true,
            allow_trailing_commas: true,
        }
    }

    pub fn parse_str(&self, input: &str) -> StdResult<Value, ParseError> {
        // Pre-allocate output string with estimated capacity (input length is a good estimate)
        let mut output = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        let mut depth: i32 = 0;

        while let Some(ch) = chars.next() {
            match ch {
                '/' => {
                    // Check for comment
                    if let Some(&next_ch) = chars.peek() {
                        match next_ch {
                            '/' => {
                                // Line comment - consume until newline
                                chars.next(); // consume '/'
                                for ch in chars.by_ref() {
                                    if ch == '\n' {
                                        output.push(ch);
                                        break;
                                    }
                                }
                            }
                            '*' => {
                                // Block comment
                                chars.next(); // consume '*'
                                let mut comment_depth = 1;
                                while let Some(ch) = chars.next() {
                                    match ch {
                                        '/' if chars.peek() == Some(&'*') => {
                                            chars.next();
                                            comment_depth += 1;
                                        }
                                        '*' if chars.peek() == Some(&'/') => {
                                            chars.next();
                                            comment_depth -= 1;
                                            if comment_depth == 0 {
                                                break;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            _ => {
                                output.push(ch);
                            }
                        }
                    } else {
                        output.push(ch);
                    }
                }
                '"' => {
                    // String - handle escapes
                    output.push(ch);
                    while let Some(ch) = chars.next() {
                        output.push(ch);
                        if ch == '\\' {
                            // Escape sequence
                            if let Some(next_ch) = chars.next() {
                                output.push(next_ch);
                            }
                        } else if ch == '"' {
                            break;
                        }
                    }
                }
                '{' | '[' => {
                    output.push(ch);
                    depth += 1;
                }
                '}' | ']' => {
                    // Before closing, remove any trailing comma (possibly after whitespace)
                    if self.allow_trailing_commas {
                        // Trim trailing whitespace
                        while output.ends_with(|c: char| c.is_whitespace()) {
                            output.pop();
                        }

                        // If the last non-whitespace char is a comma, remove it
                        if output.ends_with(',') {
                            output.pop();
                        }
                    }

                    output.push(ch);
                    depth = depth.saturating_sub(1);
                }
                ',' => {
                    // Only add comma if we're not at closing bracket
                    if !matches!(chars.peek(), Some(&('}' | ']' | ')'))) {
                        output.push(ch);
                    }
                }
                _ => {
                    output.push(ch);
                }
            }
        }

        // Parse as JSON
        serde_json::from_str(&output).map_err(ParseError::JsonError)
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    #[error("JSON parsing error: {0}")]
    JsonError(serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_jsonc() {
        let parser = JsoncParser::new();
        let input = r#"
        {
            // This is a comment
            "model": "claude-3-5-sonnet",
            "temperature": 0.1,
            "features": ["git", "watcher",], // trailing comma
        }
        "#;

        let result = parser.parse_str(input);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert_eq!(json["model"], "claude-3-5-sonnet");
        assert_eq!(json["temperature"], 0.1);
        assert_eq!(json["features"][0], "git");
        assert_eq!(json["features"][1], "watcher");
    }

    #[test]
    fn test_parse_with_block_comments() {
        let parser = JsoncParser::new();
        let input = r#"
        {
          "model": "claude-3-5-sonnet", /* inline comment */
          "temperature": 0.1
        }
        "#;

        let result = parser.parse_str(input);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert_eq!(json["model"], "claude-3-5-sonnet");
    }

    #[test]
    fn test_trailing_commas() {
        let parser = JsoncParser::new();
        let input = r#"
        {
          "features": ["git", "watcher",],
        }
        "#;

        let result = parser.parse_str(input);
        assert!(result.is_ok());
    }
}
