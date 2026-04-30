//! Accessibility Features for RustyCode TUI
//!
//! This module provides accessibility features including:
//! - Font scaling (virtual spacing adjustment)
//! - High contrast mode
//! - Reduced motion mode
//! - Screen reader friendly output
//!
//! Note: Since TUIs run in terminal emulators, actual font size is controlled
//! by the terminal application. This module implements "virtual font scaling"
//! through spacing and layout adjustments.

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

/// Font scale factor for accessibility
///
/// Represents how much to increase spacing to simulate larger text.
/// Actual font size is controlled by the terminal emulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum FontScale {
    /// No additional spacing (default)
    #[default]
    Normal,
    /// 1.25x spacing (slightly larger)
    Medium,
    /// 1.5x spacing (larger)
    Large,
    /// 2x spacing (extra large)
    ExtraLarge,
}

impl FontScale {
    /// Get all available font scales
    pub fn all() -> Vec<FontScale> {
        vec![
            FontScale::Normal,
            FontScale::Medium,
            FontScale::Large,
            FontScale::ExtraLarge,
        ]
    }

    /// Get the display name for this scale
    pub fn display_name(&self) -> &str {
        match self {
            FontScale::Normal => "Normal",
            FontScale::Medium => "Medium (1.25x)",
            FontScale::Large => "Large (1.5x)",
            FontScale::ExtraLarge => "Extra Large (2x)",
        }
    }

    /// Get the spacing multiplier for this scale
    ///
    /// This determines how much extra vertical spacing to add between lines.
    pub fn spacing_multiplier(&self) -> f32 {
        match self {
            FontScale::Normal => 1.0,
            FontScale::Medium => 1.25,
            FontScale::Large => 1.5,
            FontScale::ExtraLarge => 2.0,
        }
    }

    /// Get the number of extra blank lines between content sections
    pub fn section_spacing(&self) -> usize {
        match self {
            FontScale::Normal => 1,
            FontScale::Medium => 1,
            FontScale::Large => 2,
            FontScale::ExtraLarge => 2,
        }
    }

    /// Get the padding around content
    pub fn content_padding(&self) -> u16 {
        match self {
            FontScale::Normal => 1,
            FontScale::Medium => 2,
            FontScale::Large => 2,
            FontScale::ExtraLarge => 3,
        }
    }

    /// Calculate the visible area reduction for larger fonts
    ///
    /// Returns a reduced Rect that accounts for larger spacing needs.
    pub fn adjust_visible_area(&self, area: Rect) -> Rect {
        let effective_height = (area.height as f32 / self.spacing_multiplier()) as u16;
        Rect {
            x: area.x + self.content_padding(),
            y: area.y + self.content_padding(),
            width: area.width.saturating_sub(2 * self.content_padding()),
            height: effective_height.saturating_sub(2 * self.content_padding()),
        }
    }

    /// Get the next larger scale
    pub fn increase(&self) -> FontScale {
        match self {
            FontScale::Normal => FontScale::Medium,
            FontScale::Medium => FontScale::Large,
            FontScale::Large => FontScale::ExtraLarge,
            FontScale::ExtraLarge => FontScale::ExtraLarge, // Max
        }
    }

    /// Get the next smaller scale
    pub fn decrease(&self) -> FontScale {
        match self {
            FontScale::Normal => FontScale::Normal, // Min
            FontScale::Medium => FontScale::Normal,
            FontScale::Large => FontScale::Medium,
            FontScale::ExtraLarge => FontScale::Large,
        }
    }
}

impl std::str::FromStr for FontScale {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "normal" | "1" => Ok(FontScale::Normal),
            "medium" | "1.25" | "1.25x" => Ok(FontScale::Medium),
            "large" | "1.5" | "1.5x" => Ok(FontScale::Large),
            "xl" | "extra" | "extra_large" | "2" | "2x" => Ok(FontScale::ExtraLarge),
            _ => Err(format!("Unknown font scale: {}", s)),
        }
    }
}

/// Accessibility settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilitySettings {
    /// Font scale factor
    pub font_scale: FontScale,
    /// High contrast mode
    pub high_contrast: bool,
    /// Reduced motion (disable animations)
    pub reduced_motion: bool,
    /// Screen reader mode (simplified output)
    pub screen_reader_mode: bool,
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        Self {
            font_scale: FontScale::Normal,
            high_contrast: false,
            reduced_motion: false,
            screen_reader_mode: false,
        }
    }
}

impl AccessibilitySettings {
    /// Create new accessibility settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set font scale
    pub fn with_font_scale(mut self, scale: FontScale) -> Self {
        self.font_scale = scale;
        self
    }

    /// Set high contrast mode
    pub fn with_high_contrast(mut self, enabled: bool) -> Self {
        self.high_contrast = enabled;
        self
    }

    /// Set reduced motion mode
    pub fn with_reduced_motion(mut self, enabled: bool) -> Self {
        self.reduced_motion = enabled;
        self
    }

    /// Set screen reader mode
    pub fn with_screen_reader(mut self, enabled: bool) -> Self {
        self.screen_reader_mode = enabled;
        self
    }

    /// Increase font scale
    pub fn increase_font_scale(&mut self) {
        self.font_scale = self.font_scale.increase();
    }

    /// Decrease font scale
    pub fn decrease_font_scale(&mut self) {
        self.font_scale = self.font_scale.decrease();
    }

    /// Toggle high contrast mode
    pub fn toggle_high_contrast(&mut self) {
        self.high_contrast = !self.high_contrast;
    }

    /// Toggle reduced motion
    pub fn toggle_reduced_motion(&mut self) {
        self.reduced_motion = !self.reduced_motion;
    }

    /// Toggle screen reader mode
    pub fn toggle_screen_reader(&mut self) {
        self.screen_reader_mode = !self.screen_reader_mode;
    }
}

/// Shared accessibility settings
pub type SharedAccessibility = Arc<RwLock<AccessibilitySettings>>;

/// Create new shared accessibility settings
pub fn create_accessibility() -> SharedAccessibility {
    Arc::new(RwLock::new(AccessibilitySettings::new()))
}

/// Helper to apply accessibility adjustments to rendering
pub struct AccessibilityRenderer {
    settings: SharedAccessibility,
}

impl AccessibilityRenderer {
    /// Create a new accessibility renderer
    pub fn new(settings: SharedAccessibility) -> Self {
        Self { settings }
    }

    /// Get the current settings
    pub fn settings(&self) -> AccessibilitySettings {
        self.settings
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| {
                // If lock is poisoned, return default settings
                AccessibilitySettings::default()
            })
    }

    /// Calculate adjusted area for content
    pub fn content_area(&self, area: Rect) -> Rect {
        let settings = self.settings();
        settings.font_scale.adjust_visible_area(area)
    }

    /// Get the number of blank lines between sections
    pub fn section_spacing(&self) -> usize {
        let settings = self.settings();
        settings.font_scale.section_spacing()
    }

    /// Check if animations should be disabled
    pub fn should_disable_animations(&self) -> bool {
        let settings = self.settings();
        settings.reduced_motion
    }

    /// Check if high contrast mode is enabled
    pub fn is_high_contrast(&self) -> bool {
        let settings = self.settings();
        settings.high_contrast
    }

    /// Check if screen reader mode is enabled
    pub fn is_screen_reader_mode(&self) -> bool {
        let settings = self.settings();
        settings.screen_reader_mode
    }
}

/// Generate accessible text output (for screen readers)
///
/// Formats content in a way that's more friendly to screen readers
/// by adding descriptive prefixes and simplifying complex layouts.
pub fn format_accessible_text(prefix: &str, content: &str) -> String {
    format!("{}: {}", prefix, content)
}

/// Generate accessible list output
///
/// Formats list items with clear numbering/bullets for screen readers.
pub fn format_accessible_list(items: &[String], numbered: bool) -> String {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            if numbered {
                format!("Item {}: {}", i + 1, item)
            } else {
                format!("• {}", item)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Screen reader announcement types
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnnouncementPriority {
    /// Low priority (informational)
    Low,
    /// Medium priority (status updates)
    Medium,
    /// High priority (important changes)
    High,
    /// Critical (errors, confirmations)
    Critical,
}

/// Screen readable UI element
///
/// Represents a UI element with screen reader metadata.
#[derive(Debug, Clone)]
pub struct ScreenReadableElement {
    /// Element type (e.g., "button", "text", "list")
    pub element_type: String,
    /// Accessible label
    pub label: String,
    /// Current value/state
    pub value: Option<String>,
    /// Additional description
    pub description: Option<String>,
    /// Whether this element can be interacted with
    pub interactive: bool,
    /// Current state (e.g., "checked", "expanded")
    pub state: Option<String>,
}

impl ScreenReadableElement {
    /// Create a new screen readable element
    pub fn new(element_type: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            element_type: element_type.into(),
            label: label.into(),
            value: None,
            description: None,
            interactive: false,
            state: None,
        }
    }

    /// Set the value
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Set the description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Mark as interactive
    pub fn interactive(mut self) -> Self {
        self.interactive = true;
        self
    }

    /// Set the state
    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    /// Generate screen reader announcement
    pub fn announce(&self) -> String {
        let mut parts = Vec::new();

        // Add element type if not text
        if self.element_type != "text" {
            parts.push(self.element_type.clone());
        }

        // Add label
        parts.push(self.label.clone());

        // Add state if present
        if let Some(ref state) = self.state {
            parts.push(format!("({})", state));
        }

        // Add value if present
        if let Some(ref value) = self.value {
            parts.push(format!(": {}", value));
        }

        // Add description if present
        if let Some(ref desc) = self.description {
            parts.push(format!(". {}", desc));
        }

        parts.join(" ")
    }
}

/// Screen reader focus history for context tracking
#[derive(Debug, Clone)]
pub struct FocusHistory {
    elements: Vec<ScreenReadableElement>,
    max_size: usize,
}

impl FocusHistory {
    /// Create a new focus history
    pub fn new(max_size: usize) -> Self {
        Self {
            elements: Vec::new(),
            max_size,
        }
    }

    /// Add an element to history
    pub fn push(&mut self, element: ScreenReadableElement) {
        self.elements.push(element);
        if self.elements.len() > self.max_size {
            self.elements.remove(0);
        }
    }

    /// Get the last focused element
    pub fn last(&self) -> Option<&ScreenReadableElement> {
        self.elements.last()
    }

    /// Get the previous element (before last)
    pub fn previous(&self) -> Option<&ScreenReadableElement> {
        if self.elements.len() >= 2 {
            self.elements.get(self.elements.len() - 2)
        } else {
            None
        }
    }

    /// Clear history
    pub fn clear(&mut self) {
        self.elements.clear();
    }
}

/// Screen reader announcement queue
///
/// Manages announcements to be read by screen readers.
#[derive(Debug, Clone)]
pub struct AnnouncementQueue {
    announcements: Vec<(String, AnnouncementPriority)>,
    max_size: usize,
}

impl AnnouncementQueue {
    /// Create a new announcement queue
    pub fn new(max_size: usize) -> Self {
        Self {
            announcements: Vec::new(),
            max_size,
        }
    }

    /// Add an announcement
    pub fn announce(&mut self, message: impl Into<String>, priority: AnnouncementPriority) {
        self.announcements.push((message.into(), priority));
        if self.announcements.len() > self.max_size {
            self.announcements.remove(0);
        }
    }

    /// Get all pending announcements in priority order
    pub fn get_all(&self) -> Vec<String> {
        let mut announcements = self.announcements.clone();
        // Sort by priority (critical first)
        announcements.sort_by(|a, b| match (&a.1, &b.1) {
            (AnnouncementPriority::Critical, AnnouncementPriority::Critical) => {
                std::cmp::Ordering::Equal
            }
            (AnnouncementPriority::Critical, _) => std::cmp::Ordering::Less,
            (_, AnnouncementPriority::Critical) => std::cmp::Ordering::Greater,
            (AnnouncementPriority::High, AnnouncementPriority::High) => std::cmp::Ordering::Equal,
            (AnnouncementPriority::High, _) => std::cmp::Ordering::Less,
            (_, AnnouncementPriority::High) => std::cmp::Ordering::Greater,
            (AnnouncementPriority::Medium, AnnouncementPriority::Medium) => {
                std::cmp::Ordering::Equal
            }
            (AnnouncementPriority::Medium, _) => std::cmp::Ordering::Less,
            (_, AnnouncementPriority::Medium) => std::cmp::Ordering::Greater,
            (AnnouncementPriority::Low, AnnouncementPriority::Low) => std::cmp::Ordering::Equal,
        });
        announcements.into_iter().map(|(msg, _)| msg).collect()
    }

    /// Clear all announcements
    pub fn clear(&mut self) {
        self.announcements.clear();
    }

    /// Get the count of pending announcements
    pub fn len(&self) -> usize {
        self.announcements.len()
    }

    /// Check if there are no pending announcements
    pub fn is_empty(&self) -> bool {
        self.announcements.is_empty()
    }
}

/// Generate accessible button label
pub fn button_label(label: &str, shortcut: Option<&str>) -> String {
    if let Some(key) = shortcut {
        format!("Button: {}. Press {} to activate.", label, key)
    } else {
        format!("Button: {}. Press Enter to activate.", label)
    }
}

/// Generate accessible link label
pub fn link_label(text: &str, url: Option<&str>) -> String {
    if let Some(u) = url {
        format!("Link: {}. Points to {}.", text, u)
    } else {
        format!("Link: {}.", text)
    }
}

/// Generate accessible text field label
pub fn text_field_label(
    label: &str,
    current_value: Option<&str>,
    placeholder: Option<&str>,
) -> String {
    let mut result = format!("Text field: {}. ", label);
    if let Some(value) = current_value {
        result.push_str(&format!("Current value: {}. ", value));
    } else if let Some(ph) = placeholder {
        result.push_str(&format!("Placeholder: {}. ", ph));
    }
    result.push_str("Type to enter text.");
    result
}

/// Generate accessible checkbox label
pub fn checkbox_label(label: &str, checked: bool) -> String {
    format!(
        "Checkbox: {}. {}.",
        label,
        if checked { "Checked" } else { "Not checked" }
    )
}

/// Generate accessible progress announcement
pub fn progress_announcement(current: u32, total: u32, description: &str) -> String {
    let percentage = if total > 0 {
        (current as f32 / total as f32 * 100.0) as u32
    } else {
        0
    };
    format!(
        "{}: {} of {} ({}%)",
        description, current, total, percentage
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_scale_all() {
        let scales = FontScale::all();
        assert_eq!(scales.len(), 4);
        assert!(scales.contains(&FontScale::Normal));
        assert!(scales.contains(&FontScale::Medium));
        assert!(scales.contains(&FontScale::Large));
        assert!(scales.contains(&FontScale::ExtraLarge));
    }

    #[test]
    fn test_font_scale_display_names() {
        assert_eq!(FontScale::Normal.display_name(), "Normal");
        assert_eq!(FontScale::Medium.display_name(), "Medium (1.25x)");
        assert_eq!(FontScale::Large.display_name(), "Large (1.5x)");
        assert_eq!(FontScale::ExtraLarge.display_name(), "Extra Large (2x)");
    }

    #[test]
    fn test_font_scale_spacing_multiplier() {
        assert_eq!(FontScale::Normal.spacing_multiplier(), 1.0);
        assert_eq!(FontScale::Medium.spacing_multiplier(), 1.25);
        assert_eq!(FontScale::Large.spacing_multiplier(), 1.5);
        assert_eq!(FontScale::ExtraLarge.spacing_multiplier(), 2.0);
    }

    #[test]
    fn test_font_scale_increase() {
        assert_eq!(FontScale::Normal.increase(), FontScale::Medium);
        assert_eq!(FontScale::Medium.increase(), FontScale::Large);
        assert_eq!(FontScale::Large.increase(), FontScale::ExtraLarge);
        assert_eq!(FontScale::ExtraLarge.increase(), FontScale::ExtraLarge);
    }

    #[test]
    fn test_font_scale_decrease() {
        assert_eq!(FontScale::Normal.decrease(), FontScale::Normal);
        assert_eq!(FontScale::Medium.decrease(), FontScale::Normal);
        assert_eq!(FontScale::Large.decrease(), FontScale::Medium);
        assert_eq!(FontScale::ExtraLarge.decrease(), FontScale::Large);
    }

    #[test]
    fn test_font_scale_adjust_area() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };

        let normal_area = FontScale::Normal.adjust_visible_area(area);
        assert!(normal_area.height > 90); // Most height preserved

        let xl_area = FontScale::ExtraLarge.adjust_visible_area(area);
        assert!(xl_area.height < normal_area.height); // Reduced for larger scale
    }

    #[test]
    fn test_accessibility_settings_default() {
        let settings = AccessibilitySettings::default();
        assert_eq!(settings.font_scale, FontScale::Normal);
        assert!(!settings.high_contrast);
        assert!(!settings.reduced_motion);
        assert!(!settings.screen_reader_mode);
    }

    #[test]
    fn test_accessibility_settings_builder() {
        let settings = AccessibilitySettings::new()
            .with_font_scale(FontScale::Large)
            .with_high_contrast(true)
            .with_reduced_motion(true);

        assert_eq!(settings.font_scale, FontScale::Large);
        assert!(settings.high_contrast);
        assert!(settings.reduced_motion);
        assert!(!settings.screen_reader_mode);
    }

    #[test]
    fn test_accessibility_settings_toggle() {
        let mut settings = AccessibilitySettings::new();

        settings.toggle_high_contrast();
        assert!(settings.high_contrast);

        settings.toggle_reduced_motion();
        assert!(settings.reduced_motion);

        settings.toggle_screen_reader();
        assert!(settings.screen_reader_mode);
    }

    #[test]
    fn test_accessibility_settings_font_scale() {
        let mut settings = AccessibilitySettings::new();
        assert_eq!(settings.font_scale, FontScale::Normal);

        settings.increase_font_scale();
        assert_eq!(settings.font_scale, FontScale::Medium);

        settings.increase_font_scale();
        assert_eq!(settings.font_scale, FontScale::Large);

        settings.decrease_font_scale();
        assert_eq!(settings.font_scale, FontScale::Medium);
    }

    #[test]
    fn test_font_scale_from_str() {
        assert_eq!("normal".parse::<FontScale>().unwrap(), FontScale::Normal);
        assert_eq!("medium".parse::<FontScale>().unwrap(), FontScale::Medium);
        assert_eq!("large".parse::<FontScale>().unwrap(), FontScale::Large);
        assert_eq!("xl".parse::<FontScale>().unwrap(), FontScale::ExtraLarge);
    }

    #[test]
    fn test_format_accessible_text() {
        let result = format_accessible_text("Error", "File not found");
        assert_eq!(result, "Error: File not found");
    }

    #[test]
    fn test_format_accessible_list() {
        let items = vec!["Item 1".to_string(), "Item 2".to_string()];

        let numbered = format_accessible_list(&items, true);
        assert!(numbered.contains("Item 1:"));
        assert!(numbered.contains("Item 2:"));

        let bulleted = format_accessible_list(&items, false);
        assert!(bulleted.contains("• Item 1"));
        assert!(bulleted.contains("• Item 2"));
    }

    #[test]
    fn test_screen_readable_element_creation() {
        let element = ScreenReadableElement::new("button", "Submit");
        assert_eq!(element.element_type, "button");
        assert_eq!(element.label, "Submit");
        assert!(!element.interactive);
    }

    #[test]
    fn test_screen_readable_element_with_value() {
        let element = ScreenReadableElement::new("slider", "Volume")
            .with_value("75%")
            .interactive();
        assert_eq!(element.value, Some("75%".to_string()));
        assert!(element.interactive);
    }

    #[test]
    fn test_screen_readable_element_announce() {
        let element = ScreenReadableElement::new("button", "Save")
            .interactive()
            .with_state("enabled");

        let announcement = element.announce();
        assert!(announcement.contains("button"));
        assert!(announcement.contains("Save"));
        assert!(announcement.contains("(enabled)"));
    }

    #[test]
    fn test_focus_history() {
        let mut history = FocusHistory::new(3);

        let element1 = ScreenReadableElement::new("button", "Button 1");
        let element2 = ScreenReadableElement::new("button", "Button 2");

        history.push(element1.clone());
        history.push(element2.clone());

        assert_eq!(history.last().map(|e| &e.label), Some(&element2.label));
        assert_eq!(history.previous().map(|e| &e.label), Some(&element1.label));
    }

    #[test]
    fn test_focus_history_max_size() {
        let mut history = FocusHistory::new(2);

        for i in 0..5 {
            history.push(ScreenReadableElement::new(
                "button",
                format!("Button {}", i),
            ));
        }

        // Should only keep last 2
        assert_eq!(history.last().unwrap().label, "Button 4");
        assert_eq!(history.previous().unwrap().label, "Button 3");
    }

    #[test]
    fn test_announcement_queue() {
        let mut queue = AnnouncementQueue::new(5);

        queue.announce("Message 1", AnnouncementPriority::Low);
        queue.announce("Error!", AnnouncementPriority::Critical);
        queue.announce("Message 2", AnnouncementPriority::Medium);

        assert_eq!(queue.len(), 3);

        let all = queue.get_all();
        // Critical should be first
        assert_eq!(all[0], "Error!");
    }

    #[test]
    fn test_announcement_queue_priority_sorting() {
        let mut queue = AnnouncementQueue::new(10);

        queue.announce("Low 1", AnnouncementPriority::Low);
        queue.announce("High 1", AnnouncementPriority::High);
        queue.announce("Critical 1", AnnouncementPriority::Critical);
        queue.announce("Medium 1", AnnouncementPriority::Medium);
        queue.announce("Low 2", AnnouncementPriority::Low);

        let all = queue.get_all();
        assert_eq!(all[0], "Critical 1");
        assert_eq!(all[1], "High 1");
        assert_eq!(all[2], "Medium 1");
    }

    #[test]
    fn test_button_label() {
        let label = button_label("Submit", Some("Enter"));
        assert!(label.contains("Button: Submit"));
        assert!(label.contains("Enter"));
    }

    #[test]
    fn test_link_label() {
        let label = link_label("Documentation", Some("https://example.com"));
        assert!(label.contains("Link: Documentation"));
        assert!(label.contains("https://example.com"));
    }

    #[test]
    fn test_text_field_label() {
        let label = text_field_label("Search", Some("test"), None);
        assert!(label.contains("Text field: Search"));
        assert!(label.contains("test"));
    }

    #[test]
    fn test_checkbox_label() {
        let checked = checkbox_label("Remember me", true);
        assert!(checked.contains("Checkbox: Remember me"));
        assert!(checked.contains("Checked"));

        let unchecked = checkbox_label("Remember me", false);
        assert!(unchecked.contains("Not checked"));
    }

    #[test]
    fn test_progress_announcement() {
        let progress = progress_announcement(25, 100, "Loading");
        assert_eq!(progress, "Loading: 25 of 100 (25%)");
    }
}
