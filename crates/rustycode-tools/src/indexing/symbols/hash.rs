use crate::indexing::symbols::{CodeSymbol, FileOutline};
use sha2::{Digest, Sha256};

pub fn compute_structural_hash(outline: &FileOutline) -> String {
    let mut hasher = Sha256::new();
    for symbol in &outline.symbols {
        hash_symbol_recursive(symbol, &mut hasher);
    }
    let hash_bytes = hasher.finalize();
    let mut hex = String::with_capacity(hash_bytes.len() * 2);
    for b in hash_bytes {
        use std::fmt::Write;
        write!(hex, "{:02x}", b).unwrap();
    }
    hex
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
