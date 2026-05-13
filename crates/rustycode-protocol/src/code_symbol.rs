use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, JsonSchema)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Class,
    Enum,
    Trait,
    Interface,
    Module,
    Constant,
    TypeAlias,
    Variable,
    Macro,
    Impl,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, JsonSchema)]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Internal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SymbolRange {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CodeSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
    pub end_line: usize,
    pub range: SymbolRange,
    pub signature: String,
    pub doc_comment: Option<String>,
    pub visibility: Visibility,
    pub children: Vec<CodeSymbol>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl CodeSymbol {
    pub fn find_by_name_recursive(&self, name: &str) -> Option<&CodeSymbol> {
        if self.name == name {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_name_recursive(name) {
                return Some(found);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileOutline {
    pub path: std::path::PathBuf,
    pub language: String,
    pub symbols: Vec<CodeSymbol>,
    pub imports: Vec<String>,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Function => write!(f, "fn"),
            Self::Method => write!(f, "method"),
            Self::Struct => write!(f, "struct"),
            Self::Class => write!(f, "class"),
            Self::Enum => write!(f, "enum"),
            Self::Trait => write!(f, "trait"),
            Self::Interface => write!(f, "interface"),
            Self::Module => write!(f, "mod"),
            Self::Constant => write!(f, "const"),
            Self::TypeAlias => write!(f, "type"),
            Self::Variable => write!(f, "var"),
            Self::Macro => write!(f, "macro"),
            Self::Impl => write!(f, "impl"),
        }
    }
}

impl SymbolKind {
    pub fn to_variant_name(&self) -> &'static str {
        match self {
            Self::Function => "Function",
            Self::Method => "Method",
            Self::Struct => "Struct",
            Self::Class => "Class",
            Self::Enum => "Enum",
            Self::Trait => "Trait",
            Self::Interface => "Interface",
            Self::Module => "Module",
            Self::Constant => "Constant",
            Self::TypeAlias => "TypeAlias",
            Self::Variable => "Variable",
            Self::Macro => "Macro",
            Self::Impl => "Impl",
        }
    }
}
