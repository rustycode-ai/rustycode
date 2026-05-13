use rustycode_protocol::code_symbol::FileOutline;

#[derive(Default)]
pub struct SymbolOutlinePanel {
    pub visible: bool,
    pub outline: Option<FileOutline>,
}

impl SymbolOutlinePanel {
    pub fn new() -> Self {
        Self::default()
    }
}
