//! Shared text processing utilities.

/// Strip ANSI CSI escape sequences from a string, keeping visible text.
pub fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    if ('\x30'..='\x3f').contains(&next) || ('\x20'..='\x2f').contains(&next) {
                        chars.next();
                    } else {
                        break;
                    }
                }
                if chars.peek().is_some_and(|c| ('\x40'..='\x7e').contains(c)) {
                    chars.next();
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_simple_color() {
        assert_eq!(strip_ansi_escapes("\x1b[31mError\x1b[0m"), "Error");
    }

    #[test]
    fn strip_multiple() {
        assert_eq!(
            strip_ansi_escapes("\x1b[1;32mOK\x1b[0m \x1b[33mw\x1b[0m"),
            "OK w"
        );
    }

    #[test]
    fn strip_plain() {
        assert_eq!(strip_ansi_escapes("plain text"), "plain text");
    }
}
