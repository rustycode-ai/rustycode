//! Brutalist TUI rendering — distinctive, asymmetric, raw
//!
//! Design philosophy:
//! - Heavy left border for visual anchor
//! - Light separators for structure without clutter
//! - Single-character margins for maximum content space
//! - Lowercase typography for modern feel
//! - Inline tool display instead of wasted panel

use crate::app::context_usage::ContextUsage;
use crate::app::thinking_messages;
use crate::theme::ThemeColors;
use crate::ui::input::InputMode;
use crate::ui::message::{ExpansionLevel, Message, MessageRole, ToolExecution, ToolStatus};
use crate::ui::message_search::MatchPosition;
use chrono::Utc;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthStr;

use crate::app::render::brutalist_helpers::{
    count_consecutive, extract_tool_key_param, find_byte, find_byte_pair, find_consecutive,
    format_elapsed_short, format_tokens_compact, shorten_tool_param, tool_type_icon,
};

/// Maximum lines to show in a code block before truncating
pub(crate) const MAX_CODE_BLOCK_LINES: usize = 50;

/// Configuration for BrutalistRenderer construction
///
/// Use `BrutalistRendererBuilder` to construct this ergonomically.
#[derive(Default)]
#[non_exhaustive]
pub struct BrutalistRendererConfig<'a> {
    pub messages: &'a [Message],
    pub current_stream_content: &'a str,
    pub is_streaming: bool,
    pub scroll_offset_line: usize,
    pub user_scrolled: bool,
    pub selected_message: usize,
    pub viewport_height: usize,
    pub theme_colors: Option<Arc<Mutex<ThemeColors>>>,
    pub agent_status: &'a str,
    pub auto_memory_status: &'a str,
    pub input_mode: InputMode,
    pub rate_limit_until: Option<Instant>,
    pub chunks_received: usize,
    pub thinking_chunks_received: usize,
    pub animation_frame: usize,
    pub input_text: &'a str,
    pub context_usage: ContextUsage,
    pub active_tool_count: usize,
    pub active_tool_names: String,
    pub session_cost: f64,
    pub api_key_warning: String,
    pub header_collapsed: bool,
    pub footer_collapsed: bool,
    pub input_line_count: usize,
    pub has_queued_message: bool,
    /// Preview text of the queued message (shown during streaming)
    pub queued_message_preview: String,
    pub last_response_duration: Option<Duration>,
    pub stream_elapsed: Option<Duration>,
    pub current_model: &'a str,
    pub session_input_tokens: usize,
    pub session_output_tokens: usize,
    pub session_cache_read_tokens: usize,
    pub last_turn_input_tokens: usize,
    pub git_branch: &'a str,
    pub reverse_search_query: String,
    pub reverse_search_match: usize,
    pub reverse_search_total: usize,
    pub history_position: usize,
    pub history_total: usize,
    pub search_query: String,
    pub search_matches: Vec<MatchPosition>,
    pub search_current_match_index: usize,
    pub session_start: Option<Instant>,
    /// Cursor column position within input text (0-indexed, for cursor rendering)
    pub cursor_col: usize,
    /// Cursor row position within input text (0-indexed, for multiline cursor)
    pub cursor_row: usize,
    /// Current working directory (for display)
    pub cwd: PathBuf,
}

/// Builder for BrutalistRendererConfig
#[non_exhaustive]
pub struct BrutalistRendererBuilder<'a> {
    config: BrutalistRendererConfig<'a>,
}

impl<'a> BrutalistRendererBuilder<'a> {
    pub fn new(messages: &'a [Message], input_text: &'a str) -> Self {
        Self {
            config: BrutalistRendererConfig {
                messages,
                input_text,
                ..Default::default()
            },
        }
    }

    pub fn stream_content(mut self, content: &'a str) -> Self {
        self.config.current_stream_content = content;
        self
    }

    pub fn is_streaming(mut self, streaming: bool) -> Self {
        self.config.is_streaming = streaming;
        self
    }

    pub fn scroll(mut self, offset: usize, user_scrolled: bool) -> Self {
        self.config.scroll_offset_line = offset;
        self.config.user_scrolled = user_scrolled;
        self
    }

    pub fn selection(mut self, selected: usize, viewport: usize) -> Self {
        self.config.selected_message = selected;
        self.config.viewport_height = viewport;
        self
    }

    pub fn theme(mut self, colors: Arc<Mutex<ThemeColors>>) -> Self {
        self.config.theme_colors = Some(colors);
        self
    }

    pub fn statuses(mut self, agent: &'a str, auto_memory: &'a str) -> Self {
        self.config.agent_status = agent;
        self.config.auto_memory_status = auto_memory;
        self
    }

    pub fn input_mode(mut self, mode: InputMode) -> Self {
        self.config.input_mode = mode;
        self
    }

    pub fn rate_limit(mut self, until: Option<Instant>) -> Self {
        self.config.rate_limit_until = until;
        self
    }

    pub fn streaming_state(mut self, chunks: usize, thinking_chunks: usize, frame: usize) -> Self {
        self.config.chunks_received = chunks;
        self.config.thinking_chunks_received = thinking_chunks;
        self.config.animation_frame = frame;
        self
    }

    pub fn context_usage(mut self, usage: ContextUsage) -> Self {
        self.config.context_usage = usage;
        self
    }

    pub fn tool_status(mut self, count: usize, names: String) -> Self {
        self.config.active_tool_count = count;
        self.config.active_tool_names = names;
        self
    }

    pub fn session_info(
        mut self,
        cost: f64,
        input_tokens: usize,
        output_tokens: usize,
        cache_read_tokens: usize,
        last_turn_input: usize,
        model: &'a str,
    ) -> Self {
        self.config.session_cost = cost;
        self.config.session_input_tokens = input_tokens;
        self.config.session_output_tokens = output_tokens;
        self.config.session_cache_read_tokens = cache_read_tokens;
        self.config.last_turn_input_tokens = last_turn_input;
        self.config.current_model = model;
        self
    }

    pub fn warnings(mut self, api_key: String) -> Self {
        self.config.api_key_warning = api_key;
        self
    }

    pub fn collapsed(mut self, header: bool, footer: bool) -> Self {
        self.config.header_collapsed = header;
        self.config.footer_collapsed = footer;
        self
    }

    pub fn input_state(
        mut self,
        line_count: usize,
        has_queued: bool,
        queued_preview: String,
    ) -> Self {
        self.config.input_line_count = line_count;
        self.config.has_queued_message = has_queued;
        self.config.queued_message_preview = queued_preview;
        self
    }

    pub fn timing(
        mut self,
        last_response: Option<Duration>,
        stream_elapsed: Option<Duration>,
    ) -> Self {
        self.config.last_response_duration = last_response;
        self.config.stream_elapsed = stream_elapsed;
        self
    }

    pub fn git_branch(mut self, branch: &'a str) -> Self {
        self.config.git_branch = branch;
        self
    }

    pub fn reverse_search(mut self, query: String, match_idx: usize, total: usize) -> Self {
        self.config.reverse_search_query = query;
        self.config.reverse_search_match = match_idx;
        self.config.reverse_search_total = total;
        self
    }

    pub fn history_browsing(mut self, position: usize, total: usize) -> Self {
        self.config.history_position = position;
        self.config.history_total = total;
        self
    }

    pub fn search(
        mut self,
        query: String,
        matches: Vec<MatchPosition>,
        current_match: usize,
    ) -> Self {
        self.config.search_query = query;
        self.config.search_matches = matches;
        self.config.search_current_match_index = current_match;
        self
    }

    pub fn session_start(mut self, start: Option<Instant>) -> Self {
        self.config.session_start = start;
        self
    }

    pub fn cursor_position(mut self, col: usize, row: usize) -> Self {
        self.config.cursor_col = col;
        self.config.cursor_row = row;
        self
    }

    pub fn cwd(mut self, cwd: PathBuf) -> Self {
        self.config.cwd = cwd;
        self
    }

    pub fn build(self) -> BrutalistRenderer<'a> {
        BrutalistRenderer::from_config(self.config)
    }
}

/// Brutalist renderer for distinctive TUI appearance
#[non_exhaustive]
pub struct BrutalistRenderer<'a> {
    /// Messages to display
    pub messages: &'a [Message],
    pub current_stream_content: &'a str,
    /// Whether currently streaming
    pub is_streaming: bool,
    /// Scroll offset (line-based)
    pub scroll_offset_line: usize,
    /// Whether user has manually scrolled (disables auto-scroll)
    pub user_scrolled: bool,
    /// Selected message index
    pub selected_message: usize,
    pub viewport_height: usize,
    pub theme_colors: Arc<Mutex<ThemeColors>>,
    pub agent_status: &'a str,
    /// Auto-memory status
    pub auto_memory_status: &'a str,
    pub input_mode: InputMode,
    /// Rate limit until time
    pub rate_limit_until: Option<Instant>,
    pub chunks_received: usize,
    pub thinking_chunks_received: usize,
    /// Animation frame (for streaming pulse)
    pub animation_frame: usize,
    pub input_text: &'a str,
    /// Context usage tracking (token counts)
    pub context_usage: ContextUsage,
    /// Number of active/running tools
    pub active_tool_count: usize,
    /// Comma-separated names of active tools (for status bar, capped at 3)
    pub active_tool_names: String,
    /// Session cost in USD
    pub session_cost: f64,
    /// Pre-computed API key warning (empty if no warning needed)
    pub api_key_warning: String,
    /// Whether header/status bar is collapsed (Ctrl+Shift+H toggle)
    pub header_collapsed: bool,
    /// Whether footer is collapsed (Ctrl+Shift+H toggle)
    pub footer_collapsed: bool,
    /// Number of input content lines (for dynamic input area sizing)
    pub input_line_count: usize,
    /// Whether a message is queued for sending after stream completes
    pub has_queued_message: bool,
    /// Preview text of the queued message (shown during streaming)
    pub queued_message_preview: String,
    /// Duration of the last completed response
    pub last_response_duration: Option<Duration>,
    /// Elapsed time since stream started (for live timing during streaming)
    pub stream_elapsed: Option<Duration>,
    /// Current model name (for header display)
    pub current_model: &'a str,
    /// Input tokens used this session (for context bar split display)
    pub session_input_tokens: usize,
    /// Output tokens used this session (for context bar split display)
    pub session_output_tokens: usize,
    /// Cache hit tokens this session (prompt caching savings)
    pub session_cache_read_tokens: usize,
    /// Input tokens from last API call (actual current context size)
    pub last_turn_input_tokens: usize,
    /// Cached git branch name (avoids running git rev-parse per frame)
    pub git_branch: &'a str,
    /// Reverse search query (empty when not in reverse search mode)
    pub reverse_search_query: String,
    /// Reverse search match position (1-indexed, 0 when not searching)
    pub reverse_search_match: usize,
    /// Reverse search total matches (0 when not searching)
    pub reverse_search_total: usize,
    /// History browsing position (0 when not browsing, 1-indexed)
    pub history_position: usize,
    /// Total history items (0 when not browsing)
    pub history_total: usize,
    /// Active search query (empty when search not visible)
    pub search_query: String,
    /// Search match positions for highlighting
    pub search_matches: Vec<MatchPosition>,
    /// Current search match index (for current-match highlighting)
    pub search_current_match_index: usize,
    /// Session start time (for duration display)
    pub session_start: Option<Instant>,
    /// Cursor column position within input text (0-indexed)
    pub cursor_col: usize,
    /// Cursor row position within input text (0-indexed)
    pub cursor_row: usize,
    /// Current working directory (for display)
    pub cwd: PathBuf,
}

impl<'a> BrutalistRenderer<'a> {
    /// Create a new brutalist renderer from configuration
    fn from_config(config: BrutalistRendererConfig<'a>) -> Self {
        Self {
            messages: config.messages,
            current_stream_content: config.current_stream_content,
            is_streaming: config.is_streaming,
            scroll_offset_line: config.scroll_offset_line,
            user_scrolled: config.user_scrolled,
            selected_message: config.selected_message,
            viewport_height: config.viewport_height,
            theme_colors: config.theme_colors.unwrap_or_else(|| {
                Arc::new(Mutex::new(ThemeColors::from(
                    &crate::theme::builtin::Theme::default(),
                )))
            }),
            agent_status: config.agent_status,
            auto_memory_status: config.auto_memory_status,
            input_mode: config.input_mode,
            rate_limit_until: config.rate_limit_until,
            chunks_received: config.chunks_received,
            thinking_chunks_received: config.thinking_chunks_received,
            animation_frame: config.animation_frame,
            input_text: config.input_text,
            context_usage: config.context_usage,
            active_tool_count: config.active_tool_count,
            active_tool_names: config.active_tool_names,
            session_cost: config.session_cost,
            api_key_warning: config.api_key_warning,
            header_collapsed: config.header_collapsed,
            footer_collapsed: config.footer_collapsed,
            input_line_count: config.input_line_count,
            has_queued_message: config.has_queued_message,
            queued_message_preview: config.queued_message_preview,
            last_response_duration: config.last_response_duration,
            stream_elapsed: config.stream_elapsed,
            current_model: config.current_model,
            session_input_tokens: config.session_input_tokens,
            session_output_tokens: config.session_output_tokens,
            session_cache_read_tokens: config.session_cache_read_tokens,
            last_turn_input_tokens: config.last_turn_input_tokens,
            git_branch: config.git_branch,
            reverse_search_query: config.reverse_search_query,
            reverse_search_match: config.reverse_search_match,
            reverse_search_total: config.reverse_search_total,
            history_position: config.history_position,
            history_total: config.history_total,
            search_query: config.search_query,
            search_matches: config.search_matches,
            search_current_match_index: config.search_current_match_index,
            session_start: config.session_start,
            cursor_col: config.cursor_col,
            cursor_row: config.cursor_row,
            cwd: config.cwd,
        }
    }
}

// Header rendering
include!("brutalist_render/header.rs");

// Welcome screen rendering
include!("brutalist_render/welcome.rs");

// Message rendering, layout, and auto-scroll
include!("brutalist_render/messages.rs");

// Tool line rendering
include!("brutalist_render/tool_line.rs");

// Input area rendering
include!("brutalist_render/input.rs");

// Footer rendering
include!("brutalist_render/footer.rs");

// Inline markdown parsing and rendering
include!("brutalist_render/markdown.rs");

// Markdown table helpers shared by message rendering and height estimation
include!("brutalist_render/table.rs");

// Height estimation
include!("brutalist_render/height.rs");
