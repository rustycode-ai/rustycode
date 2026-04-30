#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MemoryCommand {
    Show,
    Reload,
    Add(String),
    Prune,
    PruneConfirm,
    PruneDry,
    Stats,
    Usage,
}

pub fn parse_memory_command_args(args: Option<&str>) -> MemoryCommand {
    match args {
        None | Some("") | Some("show") | Some("status") => MemoryCommand::Show,
        Some("reload") | Some("refresh") => MemoryCommand::Reload,
        Some("prune") | Some("clean") => MemoryCommand::Prune,
        Some("prune confirm") | Some("prune --yes") | Some("prune yes") => {
            MemoryCommand::PruneConfirm
        }
        Some("prune dry") | Some("prune --dry") | Some("prune preview") => MemoryCommand::PruneDry,
        Some("stats") | Some("info") => MemoryCommand::Stats,
        Some(raw) if raw.starts_with("add ") => {
            let fact = raw[4..].trim();
            if fact.is_empty() {
                MemoryCommand::Usage
            } else {
                MemoryCommand::Add(fact.to_string())
            }
        }
        _ => MemoryCommand::Usage,
    }
}

pub fn memory_usage_text() -> &'static str {
    "❓ Usage: /memory [show|reload|add|prune|stats] (use 'prune confirm' to apply or 'prune dry' to preview)"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_show_defaults() {
        assert_eq!(parse_memory_command_args(None), MemoryCommand::Show);
        assert_eq!(parse_memory_command_args(Some("")), MemoryCommand::Show);
        assert_eq!(parse_memory_command_args(Some("show")), MemoryCommand::Show);
    }

    #[test]
    fn parse_reload_aliases() {
        assert_eq!(
            parse_memory_command_args(Some("reload")),
            MemoryCommand::Reload
        );
        assert_eq!(
            parse_memory_command_args(Some("refresh")),
            MemoryCommand::Reload
        );
    }

    #[test]
    fn parse_add_and_usage() {
        assert_eq!(
            parse_memory_command_args(Some("add remember this")),
            MemoryCommand::Add("remember this".to_string())
        );
        assert_eq!(
            parse_memory_command_args(Some("add ")),
            MemoryCommand::Usage
        );
        assert_eq!(parse_memory_command_args(Some("bad")), MemoryCommand::Usage);
    }

    #[test]
    fn parse_prune_variants() {
        assert_eq!(
            parse_memory_command_args(Some("prune")),
            MemoryCommand::Prune
        );
        assert_eq!(
            parse_memory_command_args(Some("clean")),
            MemoryCommand::Prune
        );
        assert_eq!(
            parse_memory_command_args(Some("prune confirm")),
            MemoryCommand::PruneConfirm
        );
        assert_eq!(
            parse_memory_command_args(Some("prune --yes")),
            MemoryCommand::PruneConfirm
        );
        assert_eq!(
            parse_memory_command_args(Some("prune dry")),
            MemoryCommand::PruneDry
        );
        assert_eq!(
            parse_memory_command_args(Some("prune --dry")),
            MemoryCommand::PruneDry
        );
    }

    #[test]
    fn usage_text_mentions_dry_and_confirm() {
        let s = memory_usage_text();
        assert!(s.contains("prune"));
        assert!(s.contains("confirm") || s.contains("dry"));
    }
}
