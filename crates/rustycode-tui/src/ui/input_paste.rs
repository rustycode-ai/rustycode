//! Paste handling for clipboard operations.
//!
//! This module provides clipboard paste support for:
//! - Text paste (single-line and multi-line)
//! - Image paste with automatic preview generation
//!
//! ## Security
//!
//! All paste operations are subject to size limits to prevent memory exhaustion:
//! - Text paste: MAX_PASTE_SIZE_BYTES (10MB default)
//! - Image paste: MAX_IMAGE_SIZE_BYTES (5MB)

use crate::clipboard;
use anyhow::{Context, Result};
use ulid::Ulid;

/// Maximum text paste size in bytes (10MB default)
/// Prevents memory exhaustion from large text pastes
pub const MAX_PASTE_SIZE_BYTES: usize = 10 * 1024 * 1024;

// Re-export from sibling modules
use super::input_image::generate_image_preview;
use super::message_types::ImageAttachment;
use super::input_state::InputState;

// ── Paste Result ───────────────────────────────────────────────────────────────

/// Result of paste operation
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum PasteResult {
    Text,
    Image,
    None,
}

impl PasteResult {
    /// Get user-friendly message for this result
    pub fn message(&self) -> Option<&'static str> {
        match self {
            PasteResult::Text => Some("Text pasted"),
            PasteResult::Image => Some("Image attached"),
            PasteResult::None => None,
        }
    }
}

// ── Paste Handler ───────────────────────────────────────────────────────────────

/// Clipboard paste handler
///
/// Uses platform-specific clipboard functions for better image paste support.
pub struct PasteHandler {
    _phantom: std::marker::PhantomData<()>,
}

impl std::fmt::Debug for PasteHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasteHandler").finish()
    }
}

impl PasteHandler {
    /// Create new paste handler
    ///
    /// Defers clipboard access to actual paste operations to avoid
    /// thread-safety issues with arboard on macOS (AppKit is not
    /// safe to call from multiple threads simultaneously).
    pub fn new() -> Result<Self> {
        Ok(Self {
            _phantom: std::marker::PhantomData,
        })
    }

    /// Handle paste from clipboard
    ///
    /// Tries image first (with platform fallbacks), then text.
    pub fn handle_paste(&mut self, input_state: &mut InputState) -> Result<PasteResult> {
        // Try image first (better platform support)
        if let Ok(image) = clipboard::get_image_from_clipboard() {
            tracing::info!(
                "Pasted image: {}x{} ({} bytes, format: {})",
                image.width,
                image.height,
                image.size_bytes,
                image.format.name()
            );
            self.paste_clipboard_image(input_state, image)?;
            return Ok(PasteResult::Image);
        }

        // Fall back to text
        if let Ok(text) = clipboard::get_text_from_clipboard() {
            tracing::info!("Pasted text: {} characters", text.len());
            self.paste_text(input_state, &text)?;
            return Ok(PasteResult::Text);
        }

        tracing::debug!("Paste failed: no image or text in clipboard");
        Ok(PasteResult::None)
    }

    /// Paste text at cursor position
    fn paste_text(&self, input_state: &mut InputState, text: &str) -> Result<()> {
        // Validate paste size to prevent memory exhaustion
        let text_bytes = text.len();
        if text_bytes > MAX_PASTE_SIZE_BYTES {
            anyhow::bail!(
                "Paste too large ({} bytes). Maximum text paste size is {} bytes ({}MB).",
                text_bytes,
                MAX_PASTE_SIZE_BYTES,
                MAX_PASTE_SIZE_BYTES / (1024 * 1024)
            );
        }

        // Handle multi-line paste
        if text.contains('\n') {
            // Automatically switch to multi-line mode
            input_state.mode = super::input_state::InputMode::MultiLine;

            let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();

            // Insert at current cursor position (respecting grapheme boundaries)
            if input_state.cursor_row < input_state.lines.len() {
                let current_line = &mut input_state.lines[input_state.cursor_row];

                // Ensure cursor position is a valid char boundary
                let cursor_col = current_line
                    .floor_char_boundary(input_state.cursor_col.min(current_line.len()));
                let before = current_line[..cursor_col].to_string();
                let after = current_line[cursor_col..].to_string();

                if lines.len() == 1 {
                    // Single pasted line: before + line + after
                    *current_line = format!("{}{}{}", before, lines[0], after);
                } else {
                    // First line: before + first pasted line
                    *current_line = format!("{}{}", before, lines[0]);

                    // Middle lines: insert as-is
                    for (i, line) in lines.iter().skip(1).enumerate() {
                        if i + 1 < lines.len() - 1 {
                            input_state
                                .lines
                                .insert(input_state.cursor_row + 1 + i, line.clone());
                        }
                    }

                    // Last line: last pasted line + after
                    let last_idx = lines.len() - 1;
                    let last_pasted = &lines[last_idx];
                    input_state.lines.insert(
                        input_state.cursor_row + last_idx,
                        format!("{}{}", last_pasted, after),
                    );

                    // Move cursor to end of pasted content on last line
                    input_state.cursor_row += last_idx;
                    input_state.cursor_col = last_pasted.len();
                }
            }
        } else {
            // Single-line paste - insert the entire string at once
            if let Some(line) = input_state.lines.get_mut(input_state.cursor_row) {
                let col = line.floor_char_boundary(input_state.cursor_col.min(line.len()));
                line.insert_str(col, text);
                input_state.cursor_col = col + text.len();
            }
        }

        Ok(())
    }

    /// Paste image from clipboard (using ClipboardImage)
    fn paste_clipboard_image(
        &self,
        input_state: &mut InputState,
        image: clipboard::ClipboardImage,
    ) -> Result<()> {
        // Validate image size (prevent memory/performance issues)
        const MAX_IMAGE_SIZE: usize = 5 * 1024 * 1024; // 5MB
        if image.data.len() > MAX_IMAGE_SIZE {
            anyhow::bail!(
                "Image too large ({} bytes). Maximum size is {} bytes (5MB).",
                image.data.len(),
                MAX_IMAGE_SIZE
            );
        }

        // Save to temp file with proper extension
        let temp_dir = std::env::temp_dir();
        let file_id = Ulid::new().to_string();
        let extension = image.format.extension();
        let file_path = temp_dir.join(format!("rustycode_paste_{}.{}", file_id, extension));

        std::fs::write(&file_path, &image.data).context("Failed to write image to temp file")?;

        // Generate ASCII preview (24x6 chars)
        let preview = generate_image_preview(&file_path)?;

        // Add to attachments
        input_state.images.push(ImageAttachment {
            id: file_id.clone(),
            path: file_path.clone(),
            mime_type: image.format.mime_type().to_string(),
            preview: Some(preview),
            data_base64: None,
            width: None,
            height: None,
        });

        tracing::info!(
            "Attached image {} as {} ({}x{}, {})",
            file_id,
            file_path.display(),
            image.width,
            image.height,
            image.size_string()
        );

        Ok(())
    }

    /// Paste image from clipboard (legacy, for backward compatibility)
    pub fn paste_image(&self, input_state: &mut InputState, image_data: &[u8]) -> Result<()> {
        // Use ClipboardImage for better processing
        let clipboard_img = clipboard::ClipboardImage::new(image_data.to_vec())
            .context("Failed to process image data")?;
        self.paste_clipboard_image(input_state, clipboard_img)
    }

    /// Detect image type from bytes
    pub fn detect_image_type(&self, data: &[u8]) -> String {
        // Check for GIF first (only needs 6 bytes)
        if data.len() >= 6 && (&data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a") {
            return "image/gif".to_string();
        }

        // Check for JPEG (only needs 2 bytes)
        if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
            return "image/jpeg".to_string();
        }

        // Check for PNG (needs 8 bytes)
        if data.len() >= 8 && data[0..8] == [137, 80, 78, 71, 13, 10, 26, 10] {
            return "image/png".to_string();
        }

        // Check for WebP (needs 12 bytes)
        if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
            return "image/webp".to_string();
        }

        // Default to PNG
        "image/png".to_string()
    }
}

impl Default for PasteHandler {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            _phantom: std::marker::PhantomData,
        })
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_image_type_png() {
        let handler = PasteHandler::new().unwrap_or_default();
        let png_header = vec![137, 80, 78, 71, 13, 10, 26, 10, 0, 0];
        assert_eq!(handler.detect_image_type(&png_header), "image/png");
    }

    #[test]
    fn test_detect_image_type_jpeg() {
        let handler = PasteHandler::new().unwrap_or_default();
        let jpeg_header = vec![0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(handler.detect_image_type(&jpeg_header), "image/jpeg");
    }

    #[test]
    fn test_detect_image_type_gif() {
        let handler = PasteHandler::new().unwrap_or_default();
        let gif_header = b"GIF89a".to_vec();
        assert_eq!(handler.detect_image_type(&gif_header), "image/gif");
    }

    #[test]
    fn test_detect_image_type_webp() {
        let handler = PasteHandler::new().unwrap_or_default();
        let mut webp_header = vec![0x52, 0x49, 0x46, 0x46, 0x00, 0x00, 0x00, 0x00];
        webp_header.extend_from_slice(b"WEBP");
        assert_eq!(handler.detect_image_type(&webp_header), "image/webp");
    }

    #[test]
    fn test_detect_image_type_default() {
        let handler = PasteHandler::new().unwrap_or_default();
        let unknown = vec![0x00, 0x01, 0x02, 0x03];
        assert_eq!(handler.detect_image_type(&unknown), "image/png");
    }

    #[test]
    fn test_paste_result_messages() {
        assert_eq!(PasteResult::Text.message(), Some("Text pasted"));
        assert_eq!(PasteResult::Image.message(), Some("Image attached"));
        assert_eq!(PasteResult::None.message(), None);
    }
}
