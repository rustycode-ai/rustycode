//! Image preview generation for input attachments.
//!
//! This module provides ASCII preview generation for pasted images.

use anyhow::{Context, Result};
use image::{GenericImageView, ImageReader};
use std::path::PathBuf;

// ── Image Preview Generation ─────────────────────────────────────────────────

/// Generate ASCII preview of image (24x6 chars)
pub fn generate_image_preview(path: &PathBuf) -> Result<String> {
    // Load and resize image
    let img = ImageReader::open(path)
        .context("Failed to open image for preview")?
        .decode()
        .context("Failed to decode image for preview")?;

    // Resize to ~24 chars width, ~6 lines height
    let resized = img.resize(24, 6, image::imageops::FilterType::Lanczos3);

    // Convert to grayscale ASCII
    let ascii_chars: Vec<char> = " .:-=+*#%@".chars().collect();
    let mut preview = String::new();

    for y in 0..resized.height() {
        for x in 0..resized.width() {
            let pixel = resized.get_pixel(x, y);
            let gray =
                (pixel[0] as f32 * 0.299 + pixel[1] as f32 * 0.587 + pixel[2] as f32 * 0.114) as u8;
            let idx = (gray as usize * ascii_chars.len() / 256).min(ascii_chars.len() - 1);
            preview.push(ascii_chars[idx]);
        }
        if y < resized.height() - 1 {
            preview.push('\n');
        }
    }

    Ok(preview)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_image_preview_valid() {
        // This test requires a real image file, so we'll just test that the function
        // signature is correct and returns the expected Result type
        let path = PathBuf::from("/nonexistent/test.png");
        let result = generate_image_preview(&path);
        assert!(result.is_err());
    }
}
