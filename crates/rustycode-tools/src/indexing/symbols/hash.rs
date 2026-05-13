use crate::indexing::symbols::{CodeSymbol, FileOutline};
use sha2::{Digest, Sha256};

pub fn compute_structural_hash(outline: &FileOutline) -> String {
    let mut hasher = Sha256::new();
    for symbol in &outline.symbols {
        hash_symbol_recursive(symbol, &mut hasher);
    }
    format!("{:02x}", hasher.finalize())
}

fn hash_symbol_recursive(symbol: &CodeSymbol, hasher: &mut impl Digest) {
    // We hash the name, kind, and signature.
    // We EXCLUDE line numbers and doc comments to detect only logic/API changes.
    hasher.update(symbol.name.as_bytes());
    hasher.update(format!("{}", symbol.kind).as_bytes());
    hasher.update(symbol.signature.as_bytes());

    for child in &symbol.children {
        hash_symbol_recursive(child, hasher);
    }
}
