//! VS Code-style fuzzy file finder with project indexing and modal overlay.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// Re-export the shared fuzzy matcher types for convenience
pub use super::fuzzy_matcher::{FuzzyMatcher as FileFuzzyMatcher, MatchScore as FuzzyScore};

// FILE INFO

/// Information about a file in the project
#[derive(Clone, Debug)]
pub struct FileInfo {
    /// File path (relative to project root)
    pub path: PathBuf,

    /// File name
    pub name: String,

    /// File extension
    pub extension: Option<String>,

    /// File size in bytes
    pub size: u64,

    /// Git status (if available)
    pub git_status: Option<GitStatus>,
}

/// Git status of a file
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitStatus {
    /// New file, not staged
    New,
    /// Modified, not staged
    Modified,
    /// Staged for commit
    Staged,
    /// Deleted
    Deleted,
    /// Renamed
    Renamed,
    /// Unmerged
    Unmerged,
    /// No changes
    Clean,
}

impl GitStatus {
    /// Get color for this status
    pub fn color(self) -> Color {
        match self {
            GitStatus::New => Color::Green,
            GitStatus::Modified => Color::Yellow,
            GitStatus::Staged => Color::Cyan,
            GitStatus::Deleted => Color::Red,
            GitStatus::Renamed => Color::Blue,
            GitStatus::Unmerged => Color::Magenta,
            GitStatus::Clean => Color::DarkGray,
        }
    }

    /// Get display character
    pub fn display_char(self) -> &'static str {
        match self {
            GitStatus::New => "+",
            GitStatus::Modified => "M",
            GitStatus::Staged => "●",
            GitStatus::Deleted => "D",
            GitStatus::Renamed => "R",
            GitStatus::Unmerged => "U",
            GitStatus::Clean => " ",
        }
    }
}

impl FileInfo {
    pub fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        let extension = path.extension().map(|e| e.to_string_lossy().to_string());

        Self {
            path,
            name,
            extension,
            size: 0,
            git_status: None,
        }
    }

    /// Format file size for display
    pub fn size_display(&self) -> String {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * KB;
        const GB: u64 = 1024 * MB;

        if self.size >= GB {
            format!("{} GB", self.size / GB)
        } else if self.size >= MB {
            format!("{} MB", self.size / MB)
        } else if self.size >= KB {
            format!("{} KB", self.size / KB)
        } else {
            format!("{} B", self.size)
        }
    }

    /// Get file icon based on extension
    pub fn icon(&self) -> &'static str {
        match self.extension.as_deref() {
            Some("rs") => "🦀",
            Some("js" | "ts" | "jsx" | "tsx") => "📜",
            Some("py") => "🐍",
            Some("go") => "🐹",
            Some("md") => "📝",
            Some("txt") => "📄",
            Some("json" | "yaml" | "yml" | "toml") => "⚙️",
            Some("html" | "css") => "🎨",
            Some("png" | "jpg" | "jpeg" | "gif" | "svg") => "🖼️",
            Some("mp4" | "mov" | "avi") => "🎬",
            Some("mp3" | "wav" | "flac") => "🎵",
            _ => "📁",
        }
    }
}

// FILE FINDER STATE

/// File finder state
#[derive(Debug, Clone)]
pub struct FileFinderState {
    /// Project root directory
    pub project_root: PathBuf,

    /// Current search query
    pub query: String,

    /// All indexed files
    pub files: Vec<FileInfo>,

    /// Filtered and ranked file indices (index into files)
    pub filtered_indices: Vec<usize>,

    /// Currently selected index (into filtered_indices)
    pub selected_index: usize,

    pub visible: bool,

    /// Fuzzy matcher
    matcher: FileFuzzyMatcher,

    /// Whether the user confirmed selection with Enter (vs dismissed with Escape)
    selection_confirmed: bool,
}

impl FileFinderState {
    /// Create new file finder state
    pub fn new(project_root: PathBuf) -> Self {
        // Index project files
        let files = Self::index_project(&project_root);
        let filtered_indices = (0..files.len()).collect();

        Self {
            project_root,
            query: String::new(),
            files,
            filtered_indices,
            selected_index: 0,
            visible: false,
            matcher: FileFuzzyMatcher::new(),
            selection_confirmed: false,
        }
    }

    /// Calculate match score for a query against a file (test helper)
    #[cfg(test)]
    fn match_score(&self, query: &str, file: &FileInfo) -> FuzzyScore {
        let query_lower = query.to_lowercase();
        let name_lower = file.name.to_lowercase();
        let path_str = file.path.to_string_lossy().to_lowercase();

        if query.is_empty() {
            return FuzzyScore::Substring; // Show all files when query is empty
        }

        // Use the shared matcher for name matching
        let name_score = self.matcher.match_score(query, &name_lower);
        if name_score == FuzzyScore::Exact {
            return FuzzyScore::Exact;
        }
        if name_score == FuzzyScore::Prefix {
            return FuzzyScore::Prefix;
        }
        if name_score == FuzzyScore::Substring {
            return FuzzyScore::Substring;
        }

        // Extension match (substring at end of name)
        if let Some(ext) = &file.extension {
            if ext.to_lowercase() == query_lower {
                return FuzzyScore::Substring;
            }
        }

        // Path substring match
        if path_str.contains(&query_lower) {
            return FuzzyScore::Substring;
        }

        FuzzyScore::None
    }

    /// Index all files in the project
    fn index_project(project_root: &Path) -> Vec<FileInfo> {
        let mut files = Vec::new();
        let mut visited = HashSet::new();

        // Common directories to skip
        let skip_dirs = [
            "node_modules",
            "target",
            "dist",
            "build",
            ".git",
            "vendor",
            ".venv",
            "venv",
            "__pycache__",
            ".idea",
            ".vscode",
        ];

        if let Ok(entries) = std::fs::read_dir(project_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                Self::index_directory(&path, &mut files, &mut visited, &skip_dirs, 0);
            }
        }

        // Sort files by path
        files.sort_by(|a, b| a.path.cmp(&b.path));

        files
    }

    /// Recursively index a directory
    fn index_directory(
        dir: &Path,
        files: &mut Vec<FileInfo>,
        visited: &mut HashSet<PathBuf>,
        skip_dirs: &[&str],
        depth: usize,
    ) {
        // Limit depth to prevent infinite loops
        if depth > 20 {
            return;
        }

        // Skip if already visited
        if visited.contains(dir) {
            return;
        }
        visited.insert(dir.to_path_buf());

        // Skip common directories
        if let Some(dir_name) = dir.file_name() {
            let name = dir_name.to_string_lossy();
            if skip_dirs.contains(&name.as_ref()) {
                return;
            }
        }

        // Skip hidden directories (except .github, .gitignore, etc.)
        if let Some(dir_name) = dir.file_name() {
            let name = dir_name.to_string_lossy();
            if name.starts_with('.')
                && !matches!(name.as_ref(), ".github" | ".gitignore" | ".gitattributes")
            {
                return;
            }
        }

        // Read directory entries
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                // Skip symlinks
                if path.is_symlink() {
                    continue;
                }

                if path.is_dir() {
                    Self::index_directory(&path, files, visited, skip_dirs, depth + 1);
                } else if path.is_file() {
                    // Get file metadata
                    let mut file_info = FileInfo::new(path);

                    if let Ok(metadata) = std::fs::metadata(&file_info.path) {
                        file_info.size = metadata.len();
                    }

                    // Skip very large files (>10MB)
                    if file_info.size > 10 * 1024 * 1024 {
                        continue;
                    }

                    // Skip binary files (by extension)
                    if let Some(ext) = &file_info.extension {
                        let binary_exts = [
                            "png", "jpg", "jpeg", "gif", "svg", "ico", "mp4", "mov", "avi", "mkv",
                            "mp3", "wav", "flac", "ogg", "zip", "tar", "gz", "7z", "rar", "exe",
                            "dll", "so", "dylib",
                        ];
                        if binary_exts.contains(&ext.as_str()) {
                            continue;
                        }
                    }

                    files.push(file_info);
                }
            }
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.query.clear();
        self.selected_index = 0;
        self.selection_confirmed = false;
        self.update_filtered();
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.query.clear();
        self.selected_index = 0;
        self.update_filtered();
    }

    pub fn toggle(&mut self) {
        if self.visible {
            self.selection_confirmed = false;
            self.hide();
        } else {
            self.show();
        }
    }

    fn update_filtered(&mut self) {
        self.filtered_indices = if self.query.is_empty() {
            // Show all files when query is empty
            (0..self.files.len()).collect()
        } else {
            // Filter and rank by relevance using the fuzzy matcher
            let query = self.query.clone();
            self.matcher
                .filter_and_rank(&query, &self.files, |file| {
                    // Match against file name and path
                    let name_score = self.matcher.match_score(&query, &file.name);
                    let path_score = self
                        .matcher
                        .match_score(&query, &file.path.to_string_lossy());
                    name_score.max(path_score)
                })
                .into_iter()
                .map(|(idx, _)| idx)
                .collect()
        };

        // Clamp selected index
        if self.filtered_indices.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.filtered_indices.len() - 1);
        }
    }

    pub fn selected_file(&self) -> Option<&FileInfo> {
        self.filtered_indices
            .get(self.selected_index)
            .and_then(|&idx| self.files.get(idx))
    }

    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.update_filtered();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.update_filtered();
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.update_filtered();
    }

    pub fn move_up(&mut self) {
        if !self.filtered_indices.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.filtered_indices.len() - 1);
        }
    }

    pub fn filtered_count(&self) -> usize {
        self.filtered_indices.len()
    }

    /// Re-indexes the project filesystem tree.
    pub fn reindex(&mut self) {
        self.files = Self::index_project(&self.project_root);
        self.update_filtered();
    }
}

// FILE FINDER RENDERER

/// File finder renderer
pub struct FileFinderRenderer {
    /// Visual state
    state: FileFinderState,
}

impl FileFinderRenderer {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            state: FileFinderState::new(project_root),
        }
    }

    pub fn state_mut(&mut self) -> &mut FileFinderState {
        &mut self.state
    }

    pub fn state(&self) -> &FileFinderState {
        &self.state
    }

    pub fn show(&mut self) {
        self.state.show();
    }

    pub fn hide(&mut self) {
        self.state.selection_confirmed = false;
        self.state.hide();
    }

    pub fn toggle(&mut self) {
        self.state.toggle();
    }

    /// Returns true if the event was handled.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            // Close finder on Escape
            (KeyCode::Esc, KeyModifiers::NONE) => {
                self.state.selection_confirmed = false;
                self.hide();
                true
            }

            // Navigate up
            (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
                self.state.move_up();
                true
            }

            // Navigate down
            (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
                self.state.move_down();
                true
            }

            (KeyCode::Enter, KeyModifiers::NONE) => {
                if self.state.selected_file().is_some() {
                    self.state.selection_confirmed = true;
                    self.state.hide();
                }
                true
            }

            // Typing characters
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.state.insert_char(c);
                true
            }

            // Backspace
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                self.state.backspace();
                true
            }

            // Clear query on Ctrl+U
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.state.clear_query();
                true
            }

            // Reindex on Ctrl+R
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                self.state.reindex();
                true
            }

            _ => false,
        }
    }

    /// Returns `None` until Enter confirms a selection.
    pub fn take_selected_file(&mut self) -> Option<FileInfo> {
        if !self.state.selection_confirmed {
            return None;
        }
        if let Some(_file) = self.state.selected_file() {
            let idx = self.state.filtered_indices[self.state.selected_index];
            self.state.selection_confirmed = false;
            Some(self.state.files[idx].clone())
        } else {
            None
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.state.visible {
            return;
        }
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Calculate modal size (70% width, 60% height)
        let width = (area.width * 70) / 100;
        let height = (area.height * 60) / 100;

        // Center the modal
        let x = area.x + area.width.saturating_sub(width) / 2;
        let y = area.y + area.height.saturating_sub(height) / 2;
        let modal_area = Rect::new(x, y, width, height);

        f.render_widget(Clear, area);
        f.render_widget(Clear, modal_area);

        // Split into search input and file list
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(modal_area);

        // Render search input
        self.render_search_input(f, chunks[0]);

        // Render file list
        self.render_file_list(f, chunks[1]);
    }

    fn render_search_input(&self, f: &mut Frame, area: Rect) {
        let file_count = self.state.filtered_count();
        let total_count = self.state.files.len();

        let title = Line::from(vec![
            Span::styled(
                " File Finder ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("({} of {} files) ", file_count, total_count),
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let paragraph = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Gray)),
            Span::styled(
                &self.state.query,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title),
        )
        .wrap(Wrap { trim: false });

        f.render_widget(paragraph, area);
    }

    fn render_file_list(&self, f: &mut Frame, area: Rect) {
        if self.state.filtered_indices.is_empty() {
            // Show "no results" message
            let paragraph = Paragraph::new(Line::from(vec![Span::styled(
                "No files found",
                Style::default().fg(Color::DarkGray),
            )]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });

            f.render_widget(paragraph, area);
            return;
        }

        // Create list items
        let items: Vec<ListItem> = self
            .state
            .filtered_indices
            .iter()
            .map(|&file_idx| {
                let file = &self.state.files[file_idx];

                // Git status indicator
                let git_span = if let Some(status) = file.git_status {
                    Span::styled(
                        format!(" {} ", status.display_char()),
                        Style::default().fg(status.color()),
                    )
                } else {
                    Span::raw("   ".to_string())
                };

                // File icon
                let icon_span = Span::raw(format!(" {} ", file.icon()));

                // Create name span (highlight if query matches)
                let name_span = if self.state.query.is_empty() {
                    Span::raw(file.name.clone())
                } else {
                    Span::styled(
                        &file.name,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                };

                // Create relative path display
                let relative_path = file
                    .path
                    .strip_prefix(&self.state.project_root)
                    .unwrap_or(&file.path)
                    .display()
                    .to_string();

                let path_span = Span::styled(
                    format!(" ({})", relative_path),
                    Style::default().fg(Color::DarkGray),
                );

                // Size display
                let size_span = Span::styled(
                    format!(" {} ", file.size_display()),
                    Style::default().fg(Color::DarkGray),
                );

                ListItem::new(vec![
                    Line::from(vec![git_span.clone(), icon_span, name_span, path_span]),
                    Line::from(vec![git_span, size_span]),
                ])
            })
            .collect();

        // Render list
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        // Create list state
        let mut list_state = ListState::default();
        list_state.select(Some(self.state.selected_index));

        f.render_stateful_widget(list, area, &mut list_state);
    }
}

// FILE FINDER (HIGH-LEVEL API)

/// Combines state and rendering into a single interface.
pub struct FileFinder {
    /// Renderer with embedded state
    renderer: FileFinderRenderer,
}

impl FileFinder {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            renderer: FileFinderRenderer::new(project_root),
        }
    }

    pub fn is_visible(&self) -> bool {
        self.renderer.state().visible
    }

    pub fn show(&mut self) {
        self.renderer.show();
    }

    pub fn hide(&mut self) {
        self.renderer.hide();
    }

    pub fn toggle(&mut self) {
        self.renderer.toggle();
    }

    /// Returns true if the event was handled.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.renderer.handle_key(key)
    }

    /// Returns `None` until Enter confirms a selection.
    pub fn take_selected(&mut self) -> Option<FileInfo> {
        self.renderer.take_selected_file()
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        self.renderer.render(f, area);
    }

    pub fn state_mut(&mut self) -> &mut FileFinderState {
        self.renderer.state_mut()
    }

    pub fn state(&self) -> &FileFinderState {
        self.renderer.state()
    }
}

// TESTS

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_info_creation() {
        let path = PathBuf::from("/test/file.rs");
        let file = FileInfo::new(path);

        assert_eq!(file.name, "file.rs");
        assert_eq!(file.extension, Some("rs".to_string()));
        assert_eq!(file.icon(), "🦀");
    }

    #[test]
    fn test_fuzzy_matcher() {
        let state = FileFinderState::new(std::env::temp_dir());
        let file = FileInfo::new(PathBuf::from("/test/example.rs"));

        // Exact match
        assert_eq!(state.match_score("example.rs", &file), FuzzyScore::Exact);

        // Prefix match
        assert_eq!(state.match_score("exam", &file), FuzzyScore::Prefix);

        // Substring match
        assert_eq!(state.match_score("xam", &file), FuzzyScore::Substring);

        // Extension match
        assert_eq!(state.match_score("rs", &file), FuzzyScore::Substring);

        // No match
        assert_eq!(state.match_score("xyz", &file), FuzzyScore::None);
    }

    #[test]
    fn test_file_finder_state() {
        let temp_dir = std::env::temp_dir();
        let mut state = FileFinderState::new(temp_dir.clone());

        assert!(!state.visible);
        assert!(state.files.len() < usize::MAX);

        state.show();
        assert!(state.visible);

        state.insert_char('t');
        assert_eq!(state.query, "t");

        state.backspace();
        assert_eq!(state.query, "");
    }

    #[test]
    fn test_keyboard_navigation() {
        let temp_dir = std::env::temp_dir();
        let mut finder = FileFinder::new(temp_dir);
        finder.show();

        let _initial_count = finder.state().filtered_count();

        // Navigate down
        finder.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        // Navigate up
        finder.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));

        // Escape hides finder
        finder.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!finder.is_visible());
    }

    #[test]
    fn test_enter_selection_returns_file_via_take_selected() {
        let temp_dir = std::env::temp_dir();
        let mut finder = FileFinder::new(temp_dir.clone());
        finder.show();
        if finder.state().filtered_count() == 0 {
            return;
        }

        finder.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!finder.is_visible());

        let selected = finder.take_selected();
        assert!(selected.is_some());

        assert!(finder.take_selected().is_none());
    }

    #[test]
    fn test_escape_cancels_selection() {
        let temp_dir = std::env::temp_dir();
        let mut finder = FileFinder::new(temp_dir);
        finder.show();
        finder.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        finder.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!finder.is_visible());
        assert!(finder.take_selected().is_none());
    }

    #[test]
    fn test_toggle_cancels_selection() {
        let temp_dir = std::env::temp_dir();
        let mut finder = FileFinder::new(temp_dir);
        finder.show();
        if finder.state().filtered_count() == 0 {
            return;
        }
        finder.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(finder.take_selected().is_some());

        finder.show();
        finder.state_mut().toggle();
        assert!(finder.take_selected().is_none());
    }
}
