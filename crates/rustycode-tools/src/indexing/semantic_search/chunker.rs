//! Text chunking logic for semantic indexing
//!
//! Symbol extraction functions for detecting code boundaries across languages.

/// Extract Rust symbol name from a line
pub(crate) fn extract_rust_symbol(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Function definition: look for "fn name(" pattern
    if let Some(fn_pos) = trimmed.find("fn ") {
        let after_fn = trimmed.get(fn_pos + 3..)?;
        // Get the identifier before '('
        if let Some(paren_pos) = after_fn.find('(') {
            let name = after_fn.get(..paren_pos)?.trim();
            if !name.is_empty() && !name.contains(' ') {
                return Some((name.to_string(), "function".to_string()));
            }
        }
    }

    // Struct/impl/enum
    for keyword in &[
        "pub struct",
        "pub enum",
        "pub impl",
        "struct",
        "enum",
        "impl",
    ] {
        if let Some(rest) = trimmed.strip_prefix(keyword) {
            let rest = rest.trim();
            let name = rest.split_whitespace().next()?.split('<').next()?;
            return Some((
                name.to_string(),
                keyword
                    .split_whitespace()
                    .last()
                    .unwrap_or("type")
                    .to_string(),
            ));
        }
    }

    None
}

/// Extract Python symbol name from a line
pub(crate) fn extract_python_symbol(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Function definition
    if let Some(rest) = trimmed.strip_prefix("def ") {
        let name = rest.split('(').next()?.trim();
        return Some((name.to_string(), "function".to_string()));
    }

    // Class definition
    if let Some(rest) = trimmed.strip_prefix("class ") {
        let name = rest.split('(').next()?.split(':').next()?.trim();
        return Some((name.to_string(), "class".to_string()));
    }

    None
}

/// Extract Java symbol name from a line
pub(crate) fn extract_java_symbol(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Class definition - check early since it's a clear pattern
    if trimmed.contains("class ") {
        if let Some(class_pos) = trimmed.find("class ") {
            let after_class = trimmed.get(class_pos + 6..)?;
            let name = after_class
                .split_whitespace()
                .next()?
                .split('{')
                .next()?
                .trim();
            if !name.is_empty() && !name.contains('(') {
                return Some((name.to_string(), "class".to_string()));
            }
        }
    }

    // Interface definition
    if trimmed.contains("interface ") {
        if let Some(iface_pos) = trimmed.find("interface ") {
            let after_iface = trimmed.get(iface_pos + 10..)?;
            let name = after_iface
                .split_whitespace()
                .next()?
                .split('{')
                .next()?
                .trim();
            if !name.is_empty() {
                return Some((name.to_string(), "interface".to_string()));
            }
        }
    }

    // Method definition: look for method_name( pattern with type-like prefix
    // Patterns: "public void foo(", "private String bar(", "int baz(", "void test("
    // Find the opening parenthesis
    let paren_pos = trimmed.find('(')?;
    let before_paren = trimmed.get(..paren_pos)?.trim();

    // Split by whitespace and get the last part (method name)
    let parts: Vec<&str> = before_paren.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let method_name = parts.last()?;

    // Skip if it looks like a control flow statement
    if *method_name == "if"
        || *method_name == "while"
        || *method_name == "for"
        || *method_name == "switch"
        || *method_name == "catch"
    {
        return None;
    }

    // Skip if it looks like a constructor call (new ClassName)
    if before_paren.contains("new ") {
        return None;
    }

    // Validate it looks like a method name (camelCase, starts with lowercase or uppercase for constructors)
    if !method_name.chars().next()?.is_alphabetic() {
        return None;
    }

    // Determine if it's a constructor (starts with uppercase) or method
    let sym_type = if method_name
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
    {
        "constructor"
    } else {
        "method"
    };

    Some((method_name.to_string(), sym_type.to_string()))
}

/// Extract Go symbol name from a line
pub(crate) fn extract_go_symbol(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Function: func name( or func (receiver) name(
    if let Some(after_func) = trimmed.strip_prefix("func ") {
        // Check for method with receiver: func (r Receiver) MethodName(
        if after_func.trim_start().starts_with('(') {
            // Method with receiver
            if let Some(paren_end) = after_func.find(')') {
                let after_receiver = after_func.get(paren_end + 1..)?.trim();
                if let Some(paren_pos) = after_receiver.find('(') {
                    let name = after_receiver.get(..paren_pos)?.trim();
                    if !name.is_empty() {
                        return Some((name.to_string(), "method".to_string()));
                    }
                }
            }
        } else {
            // Regular function
            if let Some(paren_pos) = after_func.find('(') {
                let name = after_func.get(..paren_pos)?.trim();
                if !name.is_empty() {
                    return Some((name.to_string(), "function".to_string()));
                }
            }
        }
    }

    // Type definitions
    if let Some(after_type) = trimmed.strip_prefix("type ") {
        let after_type = after_type.trim();
        let parts: Vec<&str> = after_type.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }
        let name = parts[0];
        let sym_type = if parts.len() > 1 {
            match parts[1] {
                "struct" => "struct",
                "interface" => "interface",
                _ => "type",
            }
        } else {
            "type"
        };
        return Some((name.to_string(), sym_type.to_string()));
    }

    None
}

/// Extract JavaScript/TypeScript symbol name from a line
pub(crate) fn extract_javascript_symbol(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Function declaration: function name(
    if let Some(after_func) = trimmed.strip_prefix("function ") {
        let name = after_func.split('(').next()?.trim();
        if !name.is_empty() {
            return Some((name.to_string(), "function".to_string()));
        }
    }

    // Async function: async function name(
    if let Some(after_async) = trimmed.strip_prefix("async function ") {
        let name = after_async.split('(').next()?.trim();
        if !name.is_empty() {
            return Some((name.to_string(), "async_function".to_string()));
        }
    }

    // Arrow function: const/let/var name = (...) => or name = async () =>
    if trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ") {
        let eq_pos = trimmed.find('=')?;
        let before_eq = trimmed.get(..eq_pos)?.trim();
        // Remove type annotation for TypeScript: const name: Type = ...
        let name_part = before_eq.split(':').next()?;
        // Remove const/let/var keyword
        let name = name_part.split_whitespace().last()?.trim();

        let after_eq = trimmed.get(eq_pos + 1..)?.trim();
        if after_eq.starts_with("async") || after_eq.contains("=>") || after_eq.starts_with('(') {
            let sym_type = if after_eq.starts_with("async") {
                "async_function"
            } else {
                "arrow_function"
            };
            return Some((name.to_string(), sym_type.to_string()));
        }
    }

    // Class definition: class Name { or class Name extends ...
    if let Some(after_class) = trimmed.strip_prefix("class ") {
        let name = after_class
            .split_whitespace()
            .next()?
            .split('{')
            .next()?
            .trim();
        if !name.is_empty() {
            return Some((name.to_string(), "class".to_string()));
        }
    }

    // TypeScript interface: interface Name {
    if let Some(after_iface) = trimmed.strip_prefix("interface ") {
        let name = after_iface
            .split_whitespace()
            .next()?
            .split('{')
            .next()?
            .trim();
        if !name.is_empty() {
            return Some((name.to_string(), "interface".to_string()));
        }
    }

    None
}
