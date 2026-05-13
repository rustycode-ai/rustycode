pub trait SymbolDisplay {
    fn to_variant_name(&self) -> &str;
}

impl SymbolDisplay for crate::indexing::symbols::SymbolKind {
    fn to_variant_name(&self) -> &str {
        match self {
            Self::Function => "Function",
            Self::Method => "Method",
            Self::Struct => "Struct",
            Self::Enum => "Enum",
            Self::Trait => "Trait",
            Self::Impl => "Impl",
            Self::Class => "Class",
            Self::Module => "Module",
            Self::Constant => "Constant",
            Self::TypeAlias => "TypeAlias",
            Self::Variable => "Variable",
            Self::Macro => "Macro",
            Self::Interface => "Interface",
        }
    }
}
