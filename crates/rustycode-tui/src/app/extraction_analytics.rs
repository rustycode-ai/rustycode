//! Extraction Analytics Module
//!
//! Tracks and reports statistics on automatic task/todo extraction

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// Extraction analytics statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractionStats {
    /// Total number of extraction attempts
    pub total_extractions: usize,
    /// Total todos extracted
    pub todos_extracted: usize,
    /// Total tasks extracted
    pub tasks_extracted: usize,
    /// Number of times extraction was undone (user corrections)
    pub undos: usize,
    /// Pattern usage counts
    pub pattern_usage: HashMap<String, usize>,
    /// First extraction timestamp
    #[serde(skip)]
    pub started_at: Option<SystemTime>,
    /// Last update timestamp
    pub updated_at: Option<SystemTime>,
}

impl ExtractionStats {
    /// Create new analytics
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an extraction event
    pub fn record_extraction(
        &mut self,
        todos_count: usize,
        tasks_count: usize,
        patterns: Vec<String>,
    ) {
        self.total_extractions += 1;
        self.todos_extracted += todos_count;
        self.tasks_extracted += tasks_count;

        // Track pattern usage
        for pattern in patterns {
            *self.pattern_usage.entry(pattern).or_insert(0) += 1;
        }

        if self.started_at.is_none() {
            self.started_at = Some(SystemTime::now());
        }
        self.updated_at = Some(SystemTime::now());
    }

    /// Record an undo event (user correction)
    pub fn record_undo(&mut self) {
        self.undos += 1;
        self.updated_at = Some(SystemTime::now());
    }

    /// Calculate extraction success rate (percentage of extractions with items)
    pub fn success_rate(&self) -> f64 {
        if self.total_extractions == 0 {
            return 0.0;
        }
        let successful_extractions = self.total_extractions.saturating_sub(self.undos);
        (successful_extractions as f64 / self.total_extractions as f64) * 100.0
    }

    /// Calculate average items per extraction
    pub fn avg_items_per_extraction(&self) -> f64 {
        if self.total_extractions == 0 {
            return 0.0;
        }
        let total_items = self.todos_extracted + self.tasks_extracted;
        (total_items as f64) / (self.total_extractions as f64)
    }

    /// Get the most used patterns
    pub fn top_patterns(&self, limit: usize) -> Vec<(String, usize)> {
        let mut patterns: Vec<_> = self
            .pattern_usage
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        patterns.sort_by_key(|a| std::cmp::Reverse(a.1));
        patterns.into_iter().take(limit).collect()
    }

    /// Save analytics to file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load analytics from file
    pub fn load<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let json = fs::read_to_string(path)?;
        let stats: ExtractionStats = serde_json::from_str(&json)?;
        Ok(stats)
    }

    /// Format stats as a human-readable string
    pub fn format_summary(&self) -> String {
        let mut output = String::new();

        output.push_str("📊 Extraction Analytics\n");
        output.push_str(&format!(
            "   Total extractions: {}\n",
            self.total_extractions
        ));
        output.push_str(&format!("   Todos extracted: {}\n", self.todos_extracted));
        output.push_str(&format!("   Tasks extracted: {}\n", self.tasks_extracted));
        output.push_str(&format!("   User corrections: {}\n", self.undos));
        output.push_str(&format!("   Success rate: {:.1}%\n", self.success_rate()));
        output.push_str(&format!(
            "   Avg items/extraction: {:.1}\n",
            self.avg_items_per_extraction()
        ));

        if !self.pattern_usage.is_empty() {
            output.push_str("\n   Top patterns:\n");
            for (pattern, count) in self.top_patterns(5) {
                output.push_str(&format!("     • {}: {}x\n", pattern, count));
            }
        }

        output
    }

    /// Format stats as TUI lines for sidebar display
    pub fn format_tui_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Header
        lines.push(Line::from(vec![
            Span::styled("📊 ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "Extraction Analytics",
                Style::default().fg(Color::Cyan).bold(),
            ),
        ]));

        // Stats
        lines.push(Line::from(vec![
            Span::styled("  Extractions: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", self.total_extractions),
                Style::default().fg(Color::White),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("  Todos: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", self.todos_extracted),
                Style::default().fg(Color::Green),
            ),
            Span::styled(" Tasks: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", self.tasks_extracted),
                Style::default().fg(Color::Blue),
            ),
        ]));

        // Success rate with color coding
        let rate = self.success_rate();
        let rate_color = if rate >= 80.0 {
            Color::Green
        } else if rate >= 50.0 {
            Color::Yellow
        } else {
            Color::Red
        };

        lines.push(Line::from(vec![
            Span::styled("  Success: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.1}%", rate), Style::default().fg(rate_color)),
        ]));

        // Corrections
        if self.undos > 0 {
            lines.push(Line::from(vec![
                Span::styled("  Corrections: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", self.undos),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
        }

        // Average items
        let avg = self.avg_items_per_extraction();
        lines.push(Line::from(vec![
            Span::styled("  Avg/items: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.1}", avg), Style::default().fg(Color::White)),
        ]));

        // Top patterns
        if !self.pattern_usage.is_empty() {
            let top = self.top_patterns(3);
            if !top.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    "  Top patterns:",
                    Style::default().fg(Color::DarkGray),
                )]));
                for (pattern, count) in top {
                    lines.push(Line::from(vec![
                        Span::styled("    • ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            format!("{}: {}x", pattern, count),
                            Style::default().fg(Color::White),
                        ),
                    ]));
                }
            }
        }

        lines
    }

    /// Get a quick one-line summary
    pub fn quick_summary_tui(&self) -> Line<'static> {
        let rate = self.success_rate();
        let _rate_color = if rate >= 80.0 {
            Color::Green
        } else if rate >= 50.0 {
            Color::Yellow
        } else {
            Color::Red
        };

        Line::from(vec![
            Span::styled("📊 ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!(
                    "{} extractions, {:.0}% success",
                    self.total_extractions, rate
                ),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!(" ({} corrections)", self.undos),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    }
}

/// Record an undo event in the analytics file
/// This is a convenience function that handles loading, updating, and saving
pub fn record_undo() -> std::io::Result<()> {
    let analytics_path = dirs::home_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No home directory"))?
        .join(".local/share/rustycode/extraction-analytics.json");

    let mut stats = if analytics_path.exists() {
        ExtractionStats::load(&analytics_path)?
    } else {
        ExtractionStats::new()
    };

    stats.record_undo();
    stats.save(&analytics_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_extraction() {
        let mut stats = ExtractionStats::new();
        stats.record_extraction(2, 1, vec!["bullet".to_string(), "i_will".to_string()]);

        assert_eq!(stats.total_extractions, 1);
        assert_eq!(stats.todos_extracted, 2);
        assert_eq!(stats.tasks_extracted, 1);
        assert_eq!(stats.pattern_usage.get("bullet"), Some(&1)); // Fixed: was incorrectly expecting 2
        assert_eq!(stats.pattern_usage.get("i_will"), Some(&1));
    }

    #[test]
    fn test_success_rate() {
        let mut stats = ExtractionStats::new();
        assert_eq!(stats.success_rate(), 0.0);

        stats.record_extraction(1, 1, vec![]);
        assert_eq!(stats.success_rate(), 100.0);

        stats.record_undo();
        assert_eq!(stats.success_rate(), 0.0); // 1 extraction, 1 undo
    }

    #[test]
    fn test_top_patterns() {
        let mut stats = ExtractionStats::new();
        stats.record_extraction(
            0,
            0,
            vec![
                "bullet".to_string(),
                "bullet".to_string(),
                "i_will".to_string(),
                "numbered".to_string(),
            ],
        );

        let top = stats.top_patterns(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "bullet");
        assert_eq!(top[0].1, 2);
    }

    #[test]
    fn test_format_summary() {
        let stats = ExtractionStats {
            total_extractions: 10,
            todos_extracted: 15,
            tasks_extracted: 5,
            undos: 2,
            pattern_usage: { [("bullet".to_string(), 8)].iter().cloned().collect() },
            started_at: None,
            updated_at: None,
        };

        let summary = stats.format_summary();
        assert!(summary.contains("Total extractions: 10"));
        assert!(summary.contains("Success rate: 80.0%")); // (10-2)/10
        assert!(summary.contains("bullet: 8x"));
    }
}
