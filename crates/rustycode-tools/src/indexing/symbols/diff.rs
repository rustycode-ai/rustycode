use crate::indexing::symbols::{CodeSymbol, FileOutline};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize)]
pub struct OutlineDiff {
    pub added: Vec<CodeSymbol>,
    pub removed: Vec<CodeSymbol>,
    pub modified: Vec<SymbolChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SymbolChange {
    pub name: String,
    pub kind: String,
    pub old_signature: String,
    pub new_signature: String,
}

pub fn diff_outlines(old: &FileOutline, new: &FileOutline) -> OutlineDiff {
    let old_map = flatten_to_map(&old.symbols);
    let new_map = flatten_to_map(&new.symbols);

    let old_keys: HashSet<_> = old_map.keys().collect();
    let new_keys: HashSet<_> = new_map.keys().collect();

    let mut added = Vec::new();
    for key in new_keys.difference(&old_keys) {
        added.push((*new_map.get(*key).unwrap()).clone());
    }

    let mut removed = Vec::new();
    for key in old_keys.difference(&new_keys) {
        removed.push((*old_map.get(*key).unwrap()).clone());
    }

    let mut modified = Vec::new();
    for key in old_keys.intersection(&new_keys) {
        let s_old = old_map.get(*key).unwrap();
        let s_new = new_map.get(*key).unwrap();
        if s_old.signature != s_new.signature {
            modified.push(SymbolChange {
                name: s_old.name.clone(),
                kind: format!("{}", s_old.kind),
                old_signature: s_old.signature.clone(),
                new_signature: s_new.signature.clone(),
            });
        }
    }

    OutlineDiff { added, removed, modified }
}

fn flatten_to_map(symbols: &[CodeSymbol]) -> HashMap<String, &CodeSymbol> {
    let mut map = HashMap::new();
    for sym in symbols {
        flatten_recursive(sym, None, &mut map);
    }
    map
}

fn flatten_recursive<'a>(sym: &'a CodeSymbol, parent: Option<String>, map: &mut HashMap<String, &'a CodeSymbol>) {
    let full_name = if let Some(p) = parent {
        format!("{}::{}", p, sym.name)
    } else {
        sym.name.clone()
    };
    
    map.insert(full_name.clone(), sym);
    
    for child in &sym.children {
        flatten_recursive(child, Some(full_name.clone()), map);
    }
}
