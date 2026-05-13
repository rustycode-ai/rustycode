use rustycode_protocol::code_symbol::FileOutline;

pub struct SymbolOutlinePanel {
    pub visible: bool,
    pub outline: Option<FileOutline>,
}

impl SymbolOutlinePanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            outline: None,
        }
    }
}
