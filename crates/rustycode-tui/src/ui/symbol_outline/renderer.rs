use crate::ui::symbol_outline::SymbolOutlinePanel;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use rustycode_protocol::code_symbol::{CodeSymbol, SymbolKind};

pub fn render_symbol_outline(f: &mut Frame, area: Rect, panel: &SymbolOutlinePanel) {
    if !panel.visible {
        return;
    }
    if area.width == 0 || area.height == 0 {
        return;
    }

    let block = Block::default()
        .title(" Symbol Outline ")
        .borders(Borders::ALL);

    let mut items = Vec::new();
    if let Some(outline) = &panel.outline {
        for symbol in &outline.symbols {
            render_symbol_to_list(symbol, 0, &mut items);
        }
    } else {
        items.push(ListItem::new("Fetching outline..."));
    }

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn render_symbol_to_list(symbol: &CodeSymbol, depth: usize, items: &mut Vec<ListItem>) {
    let indent = "  ".repeat(depth);
    let kind_str = format!("{:?}", symbol.kind);
    let text = format!("{indent}[{}] {}", kind_str, symbol.name);
    
    let style = match symbol.kind {
        SymbolKind::Function | SymbolKind::Method => Style::default().fg(Color::Cyan),
        SymbolKind::Struct | SymbolKind::Class => Style::default().fg(Color::Yellow),
        _ => Style::default(),
    };

    items.push(ListItem::new(text).style(style));
    for child in &symbol.children {
        render_symbol_to_list(child, depth + 1, items);
    }
}
