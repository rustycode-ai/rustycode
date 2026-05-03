//! Board command handler for task board management.

use crate::commands::cli_args::BoardCommand;
use anyhow::Result;
use rustycode_protocol::board::{BoardItemId, BoardKind, BoardPriority, BoardStatus};
use rustycode_storage::BoardStore;
use std::path::Path;

fn parse_kind(s: &str) -> Result<BoardKind> {
    s.parse().map_err(|_| {
        anyhow::anyhow!("invalid kind: {s} (expected: task, feat, bug, chore, us, test, perf, doc)")
    })
}

fn parse_priority(s: &str) -> Result<BoardPriority> {
    s.parse().map_err(|_| {
        anyhow::anyhow!("invalid priority: {s} (expected: critical, high, normal, low, background)")
    })
}

fn print_item(item: &rustycode_protocol::board::BoardItem, indent: &str) {
    let status_icon = match item.status {
        BoardStatus::Pending => "[ ]",
        BoardStatus::InProgress => "[~]",
        BoardStatus::Completed => "[x]",
        BoardStatus::Failed => "[!]",
        BoardStatus::Blocked => "[-]",
        BoardStatus::Cancelled => "[/]",
        _ => "[?]",
    };
    let priority_label = match item.priority {
        BoardPriority::Critical => "P0",
        BoardPriority::High => "P1",
        BoardPriority::Normal => "P2",
        BoardPriority::Low => "P3",
        BoardPriority::Background => "P4",
        _ => "P?",
    };
    println!(
        "{}{} {} [{}] {} ({})",
        indent, item.id, status_icon, priority_label, item.title, item.kind,
    );
    if !item.description.is_empty() {
        println!("{}    {}", indent, item.description);
    }
    if !item.notes.is_empty() {
        for note in &item.notes {
            println!("{}    note: {}", indent, note);
        }
    }
}

/// Execute a `board` subcommand.
#[allow(clippy::unused_async)]
pub async fn execute(cwd: &Path, command: BoardCommand, format: &str) -> Result<()> {
    let store = BoardStore::new(cwd);

    match command {
        BoardCommand::List {
            kind,
            status,
            format: list_format,
        } => {
            let board = store.load()?;
            if board.items.is_empty() {
                println!("Board is empty. Use `rc board add <kind> <title>` to add items.");
                return Ok(());
            }

            let fmt = if list_format == "json" || format == "json" {
                "json"
            } else {
                "human"
            };

            let mut items: Vec<_> = board.items.iter().collect();

            if let Some(kind_str) = kind {
                let filter_kind = parse_kind(&kind_str)?;
                items.retain(|item| item.kind == filter_kind);
            }
            if let Some(status_str) = status {
                let filter_status: BoardStatus = status_str.parse().map_err(|_| {
                    anyhow::anyhow!(
                        "invalid status: {status_str} (expected: pending, in_progress, completed, failed, blocked, cancelled)"
                    )
                })?;
                items.retain(|item| item.status == filter_status);
            }

            if fmt == "json" {
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else {
                if !board.title.is_empty() {
                    println!("=== {} ===", board.title);
                }
                // Group top-level items (no parent) first, then show children indented
                let top_level: Vec<_> = items.iter().filter(|i| i.id.parent().is_none()).collect();
                if top_level.is_empty() {
                    println!("No matching items.");
                } else {
                    for item in &top_level {
                        print_item(item, "");
                        if !item.children.is_empty() {
                            for child_id in &item.children {
                                if let Some(child) = board.find(child_id) {
                                    print_item(child, "  ");
                                }
                            }
                        }
                    }
                }
            }
        }

        BoardCommand::Add {
            kind,
            title,
            description,
            priority,
        } => {
            let board_kind = parse_kind(&kind)?;
            if title.trim().is_empty() {
                return Err(anyhow::anyhow!("title must not be empty"));
            }
            let board_priority = if let Some(p) = priority {
                parse_priority(&p)?
            } else {
                BoardPriority::Normal
            };
            let desc = description.unwrap_or_default();

            let mut board = store.load()?;
            let id = store.add_item(&mut board, board_kind, &title, &desc, board_priority)?;

            if format == "json" {
                let item = board.find(&id).expect("just added");
                println!("{}", serde_json::to_string_pretty(&item)?);
            } else {
                println!("Created {} : {}", id, title);
            }
        }

        BoardCommand::Show { id } => {
            let board = store.load()?;
            let item_id = BoardItemId::parse(&id)
                .map_err(|e| anyhow::anyhow!("invalid item ID '{id}': {e}"))?;
            let item = board
                .find(&item_id)
                .ok_or_else(|| anyhow::anyhow!("Item {id} not found"))?;

            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&item)?);
            } else {
                println!("ID:          {}", item.id);
                println!("Title:       {}", item.title);
                println!("Kind:        {}", item.kind);
                println!("Status:      {}", item.status);
                println!("Priority:    {}", item.priority);
                if !item.description.is_empty() {
                    println!("Description: {}", item.description);
                }
                if !item.acceptance_criteria.is_empty() {
                    println!("Acceptance Criteria:");
                    for ac in &item.acceptance_criteria {
                        println!("  - {}", ac);
                    }
                }
                if let Some(ref assignee) = item.assignee {
                    println!("Assignee:    {}", assignee);
                }
                if !item.blocks.is_empty() {
                    println!(
                        "Blocks:      {}",
                        item.blocks
                            .iter()
                            .map(|b| b.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                if !item.blocked_by.is_empty() {
                    println!(
                        "Blocked by:  {}",
                        item.blocked_by
                            .iter()
                            .map(|b| b.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                if !item.children.is_empty() {
                    println!(
                        "Children:    {}",
                        item.children
                            .iter()
                            .map(|c| c.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                if !item.notes.is_empty() {
                    println!("Notes:");
                    for note in &item.notes {
                        println!("  - {}", note);
                    }
                }
                println!("Created:     {}", item.created_at.to_rfc3339());
                println!("Updated:     {}", item.updated_at.to_rfc3339());
                if let Some(started) = item.started_at {
                    println!("Started:     {}", started.to_rfc3339());
                }
                if let Some(completed) = item.completed_at {
                    println!("Completed:   {}", completed.to_rfc3339());
                }
            }
        }

        BoardCommand::Done { id } => {
            let item_id = BoardItemId::parse(&id).map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut board = store.load()?;
            store.update_status(&mut board, &item_id, BoardStatus::Completed)?;
            println!("{} marked as completed.", item_id);
        }

        BoardCommand::Fail { id } => {
            let item_id = BoardItemId::parse(&id).map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut board = store.load()?;
            store.update_status(&mut board, &item_id, BoardStatus::Failed)?;
            println!("{} marked as failed.", item_id);
        }

        BoardCommand::Block { id } => {
            let item_id = BoardItemId::parse(&id).map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut board = store.load()?;
            store.update_status(&mut board, &item_id, BoardStatus::Blocked)?;
            println!("{} marked as blocked.", item_id);
        }

        BoardCommand::Note { id, text } => {
            let item_id = BoardItemId::parse(&id).map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut board = store.load()?;
            store.add_note(&mut board, &item_id, &text)?;
            println!("Note added to {}.", item_id);
        }

        BoardCommand::Rm { id } => {
            let item_id = BoardItemId::parse(&id).map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut board = store.load()?;
            store.remove_item(&mut board, &item_id)?;
            println!("{} removed.", item_id);
        }

        BoardCommand::Title { title } => {
            let mut board = store.load()?;
            let old = board.title.clone();
            store.set_title(&mut board, &title)?;
            if old.is_empty() {
                println!("Board title set to: {}", title);
            } else {
                println!("Board title changed: {} -> {}", old, title);
            }
        }

        BoardCommand::Subtask {
            parent,
            title,
            description,
        } => {
            if title.trim().is_empty() {
                return Err(anyhow::anyhow!("title must not be empty"));
            }
            let parent_id = BoardItemId::parse(&parent).map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut board = store.load()?;
            board
                .find(&parent_id)
                .ok_or_else(|| anyhow::anyhow!("Parent item {parent} not found"))?;
            let desc = description.unwrap_or_default();
            let child_id = store.add_child(&mut board, &parent_id, &title, &desc)?;
            println!("Created {} : {}", child_id, title);
        }

        BoardCommand::Start { id } => {
            let item_id = BoardItemId::parse(&id).map_err(|e| anyhow::anyhow!("{e}"))?;
            let mut board = store.load()?;
            store.update_status(&mut board, &item_id, BoardStatus::InProgress)?;
            println!("{} marked as in_progress.", item_id);
        }
    }

    Ok(())
}
