//! Clipboard image detection and processing for TUI

use anyhow::{Context, Result};
use image::{GenericImageView, ImageFormat};
use std::io::Cursor;
use std::process::Command;

/// Maximum image size in bytes (10MB)
const MAX_IMAGE_SIZE: usize = 10 * 1024 * 1024;

/// Supported image formats
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFormatType {
    Png,
    Jpeg,
    Gif,
    WebP,
    Unknown,
}

impl ImageFormatType {
    /// Detect format from magic bytes
    pub fn from_magic_bytes(data: &[u8]) -> Self {
        if data.len() < 8 {
            return ImageFormatType::Unknown;
        }

        // PNG: 89 50 4E 47 0D 0A 1A 0A
        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return ImageFormatType::Png;
        }

        // JPEG: FF D8 FF
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return ImageFormatType::Jpeg;
        }

        // GIF: 47 49 46 38 (GIF8)
        if data.starts_with(&[0x47, 0x49, 0x46, 0x38]) {
            return ImageFormatType::Gif;
        }

        // WebP: RIFF....WEBP
        if data.len() >= 12
            && data.starts_with(&[0x52, 0x49, 0x46, 0x46]) // RIFF
            && data[8..12] == [0x57, 0x45, 0x42, 0x50]
        // WEBP
        {
            return ImageFormatType::WebP;
        }

        ImageFormatType::Unknown
    }

    /// Get MIME type for the format
    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageFormatType::Png => "image/png",
            ImageFormatType::Jpeg => "image/jpeg",
            ImageFormatType::Gif => "image/gif",
            ImageFormatType::WebP => "image/webp",
            ImageFormatType::Unknown => "application/octet-stream",
        }
    }

    /// Get file extension for the format
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormatType::Png => "png",
            ImageFormatType::Jpeg => "jpg",
            ImageFormatType::Gif => "gif",
            ImageFormatType::WebP => "webp",
            ImageFormatType::Unknown => "bin",
        }
    }

    /// Convert to image crate's ImageFormat
    pub fn to_image_format(self) -> Option<ImageFormat> {
        match self {
            ImageFormatType::Png => Some(ImageFormat::Png),
            ImageFormatType::Jpeg => Some(ImageFormat::Jpeg),
            ImageFormatType::Gif => Some(ImageFormat::Gif),
            ImageFormatType::WebP => Some(ImageFormat::WebP),
            ImageFormatType::Unknown => None,
        }
    }

    /// Human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            ImageFormatType::Png => "PNG",
            ImageFormatType::Jpeg => "JPEG",
            ImageFormatType::Gif => "GIF",
            ImageFormatType::WebP => "WebP",
            ImageFormatType::Unknown => "Unknown",
        }
    }
}

/// Image data from clipboard
#[derive(Clone, Debug)]
pub struct ClipboardImage {
    /// Raw image bytes
    pub data: Vec<u8>,
    /// Detected format
    pub format: ImageFormatType,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    pub size_bytes: usize,
    /// Whether image was compressed
    pub was_compressed: bool,
}

impl ClipboardImage {
    pub fn new(data: Vec<u8>) -> Result<Self> {
        Self::new_with_compression(data, false)
    }

    /// Create with optional compression
    pub fn new_with_compression(data: Vec<u8>, allow_compression: bool) -> Result<Self> {
        // If image is small enough, load normally
        if data.len() <= MAX_IMAGE_SIZE {
            return Self::new_internal(data);
        }

        // Image is too large
        if !allow_compression {
            anyhow::bail!(
                "Image too large: {} bytes (max {} bytes). Use a smaller image or enable compression.",
                data.len(),
                MAX_IMAGE_SIZE
            );
        }

        tracing::info!(
            "Image too large ({} bytes), attempting compression",
            data.len()
        );

        // Load image
        let img = image::load_from_memory(&data).context("Failed to load image for compression")?;

        let (width, height) = img.dimensions();

        // Resize if dimensions are too large
        let max_dim = width.max(height);
        let img = if max_dim > 1920 {
            let scale = 1920.0 / max_dim as f32;
            let new_width = (width as f32 * scale).round() as u32;
            let new_height = (height as f32 * scale).round() as u32;
            tracing::info!(
                "Resizing image from {}x{} to {}x{}",
                width,
                height,
                new_width,
                new_height
            );
            img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };

        // Encode as JPEG for better compression
        let mut compressed_data = Vec::new();
        let mut cursor = Cursor::new(&mut compressed_data);

        img.write_to(&mut cursor, ImageFormat::Jpeg)
            .context("Failed to encode image")?;

        // Check if compression helped
        if compressed_data.len() <= MAX_IMAGE_SIZE {
            tracing::info!(
                "Compressed image from {} bytes to {} bytes",
                data.len(),
                compressed_data.len()
            );
            return Self::new_internal_with_flag(compressed_data, true);
        }

        // Still too large - try more aggressive resize
        let scale = 1280.0 / img.width().max(img.height()) as f32;
        let new_width = (img.width() as f32 * scale).round() as u32;
        let new_height = (img.height() as f32 * scale).round() as u32;

        tracing::info!(
            "Aggressive resize: {}x{} -> {}x{}",
            img.width(),
            img.height(),
            new_width,
            new_height
        );
        let resized = img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3);

        compressed_data.clear();
        let mut cursor2 = Cursor::new(&mut compressed_data);
        resized
            .write_to(&mut cursor2, ImageFormat::Jpeg)
            .context("Failed to encode resized image")?;

        if compressed_data.len() <= MAX_IMAGE_SIZE {
            tracing::info!("Resized and compressed to {} bytes", compressed_data.len());
            return Self::new_internal_with_flag(compressed_data, true);
        }

        // Still too large - give up
        anyhow::bail!(
            "Image too large even after compression ({} bytes). Please use a smaller image (< 5MB recommended)",
            compressed_data.len()
        )
    }

    /// Internal constructor without compression
    fn new_internal(data: Vec<u8>) -> Result<Self> {
        Self::new_internal_with_flag(data, false)
    }

    /// Internal constructor with compression flag
    fn new_internal_with_flag(data: Vec<u8>, was_compressed: bool) -> Result<Self> {
        // Check size
        if data.len() > MAX_IMAGE_SIZE {
            anyhow::bail!(
                "Image too large: {} bytes (max {} bytes).",
                data.len(),
                MAX_IMAGE_SIZE
            );
        }

        if data.is_empty() {
            anyhow::bail!("Image data is empty");
        }

        // Detect format
        let format = ImageFormatType::from_magic_bytes(&data);
        if format == ImageFormatType::Unknown {
            anyhow::bail!("Unsupported or unrecognized image format");
        }

        // Load image to get dimensions and validate
        let cursor = Cursor::new(&data);
        let image_format = format
            .to_image_format()
            .ok_or_else(|| anyhow::anyhow!("Failed to convert image format: {:?}", format))?;
        let image_reader = image::ImageReader::with_format(cursor, image_format);

        let dimensions = image_reader
            .into_dimensions()
            .context("Failed to read image dimensions - image may be corrupt")?;

        let size_bytes = data.len();
        Ok(Self {
            data,
            format,
            width: dimensions.0,
            height: dimensions.1,
            size_bytes,
            was_compressed,
        })
    }

    /// Convert to base64 data URL for vision APIs
    pub fn to_data_url(&self) -> String {
        use base64::Engine;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&self.data);
        format!("data:{};base64,{}", self.format.mime_type(), base64_data)
    }

    /// Get human-readable size string
    pub fn size_string(&self) -> String {
        let bytes = self.size_bytes as f64;
        if bytes >= 1024.0 * 1024.0 {
            format!("{:.2} MB", bytes / (1024.0 * 1024.0))
        } else if bytes >= 1024.0 {
            format!("{:.2} KB", bytes / 1024.0)
        } else {
            format!("{} B", self.size_bytes)
        }
    }

    /// Get dimensions string
    pub fn dimensions_string(&self) -> String {
        format!("{}x{} px", self.width, self.height)
    }

    /// Get compression status
    pub fn compression_status(&self) -> &'static str {
        if self.was_compressed {
            "✓ Compressed"
        } else {
            ""
        }
    }
}

pub fn has_image_in_clipboard() -> Result<bool> {
    // Create a new clipboard instance each time to avoid stale connections
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to create clipboard instance: {}", e);
            return Err(e).context("Failed to access clipboard. Is a display server running?");
        }
    };

    // Try to get image data
    match clipboard.get_image() {
        Ok(_) => Ok(true),
        Err(arboard::Error::ContentNotAvailable) => {
            Ok(try_get_image_bytes_from_platform_clipboard().is_some())
        }
        Err(e) => {
            tracing::debug!("Clipboard image check failed: {}", e);
            Err(e).context("Failed to check clipboard for image")
        }
    }
}

/// Get image from clipboard (with automatic compression for large images)
pub fn image_from_clipboard() -> Result<ClipboardImage> {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to create clipboard instance: {}", e);
            return Err(e).context("Failed to access clipboard. Is a display server running?");
        }
    };

    match clipboard.get_image() {
        Ok(image_data) => {
            tracing::info!(
                "Got raw clipboard image: {}x{} ({} bytes)",
                image_data.width,
                image_data.height,
                image_data.bytes.len()
            );

            let encoded = encode_arboard_image_as_png(
                image_data.width,
                image_data.height,
                image_data.bytes.to_vec(),
            )?;

            ClipboardImage::new_with_compression(encoded, true)
        }
        Err(primary_err) => {
            if let Some(bytes) = try_get_image_bytes_from_platform_clipboard() {
                tracing::info!("Read clipboard image via platform fallback");
                return ClipboardImage::new_with_compression(bytes, true);
            }

            Err(primary_err).context("No image in clipboard")
        }
    }
}

fn try_get_image_bytes_from_platform_clipboard() -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("pngpaste").arg("-").output() {
            if output.status.success() && !output.stdout.is_empty() {
                return Some(output.stdout);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let attempts: &[(&str, &[&str])] = &[
            ("wl-paste", &["--type", "image/png"]),
            ("wl-paste", &["--type", "image/jpeg"]),
            ("wl-paste", &["--type", "image/webp"]),
            (
                "xclip",
                &["-selection", "clipboard", "-t", "image/png", "-o"],
            ),
            (
                "xclip",
                &["-selection", "clipboard", "-t", "image/jpeg", "-o"],
            ),
        ];

        for (cmd, args) in attempts {
            if let Ok(output) = Command::new(cmd).args(*args).output() {
                if output.status.success() && !output.stdout.is_empty() {
                    return Some(output.stdout);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Use PowerShell to export clipboard image as PNG bytes.
        let script = "$img = Get-Clipboard -Format Image; if ($img -ne $null) { $ms = New-Object System.IO.MemoryStream; $img.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png); [Console]::OpenStandardOutput().Write($ms.ToArray(), 0, [int]$ms.Length) }";
        if let Ok(output) = Command::new("PowerShell")
            .args(["-NoProfile", "-Command", script])
            .output()
        {
            if output.status.success() && !output.stdout.is_empty() {
                return Some(output.stdout);
            }
        }
    }

    None
}

fn try_get_text_from_platform_clipboard() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("pbpaste").output() {
            if output.status.success() && !output.stdout.is_empty() {
                return Some(String::from_utf8_lossy(&output.stdout).into_owned());
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = Command::new("wl-paste").output() {
            if output.status.success() && !output.stdout.is_empty() {
                return Some(String::from_utf8_lossy(&output.stdout).into_owned());
            }
        }
        if let Ok(output) = Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output()
        {
            if output.status.success() && !output.stdout.is_empty() {
                return Some(String::from_utf8_lossy(&output.stdout).into_owned());
            }
        }
        if let Ok(output) = Command::new("xsel")
            .args(["--clipboard", "--output"])
            .output()
        {
            if output.status.success() && !output.stdout.is_empty() {
                return Some(String::from_utf8_lossy(&output.stdout).into_owned());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("PowerShell")
            .args(["-NoProfile", "-Command", "Get-Clipboard"])
            .output()
        {
            if output.status.success() && !output.stdout.is_empty() {
                return Some(String::from_utf8_lossy(&output.stdout).into_owned());
            }
        }
    }

    None
}

fn encode_arboard_image_as_png(width: usize, height: usize, bytes: Vec<u8>) -> Result<Vec<u8>> {
    let rgba = image::RgbaImage::from_raw(width as u32, height as u32, bytes)
        .ok_or_else(|| anyhow::anyhow!("Clipboard image buffer has invalid dimensions/stride"))?;

    let mut encoded = Vec::new();
    let mut cursor = Cursor::new(&mut encoded);
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut cursor, ImageFormat::Png)
        .context("Failed to encode clipboard image as PNG")?;
    Ok(encoded)
}

/// Get text from clipboard (fallback)
pub fn text_from_clipboard() -> Result<String> {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to create clipboard instance for text: {}", e);
            if let Some(text) = try_get_text_from_platform_clipboard() {
                tracing::info!("Got text from platform fallback: {} characters", text.len());
                return Ok(text);
            }
            return Err(e).context("Failed to access clipboard. Is a display server running?");
        }
    };

    match clipboard.get_text() {
        Ok(text) => {
            tracing::info!("Got text from clipboard: {} characters", text.len());
            Ok(text)
        }
        Err(e) => {
            if let Some(text) = try_get_text_from_platform_clipboard() {
                tracing::info!("Got text from platform fallback: {} characters", text.len());
                return Ok(text);
            }
            Err(e).context("No text in clipboard")
        }
    }
}

/// Copy text to clipboard using OSC 52 escape sequence (TUI-aware version)
///
/// This uses the terminal's clipboard capabilities via the OSC 52 escape sequence.
/// Works in most modern terminals (iTerm2, kitty, WezTerm, tmux, etc.)
///
/// This version temporarily suspends raw mode to write the sequence properly.
///
pub fn copy_text_to_clipboard_osc52(text: &str) -> Result<()> {
    use base64::Engine;

    // Encode text as base64 (required by OSC 52)
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);

    // OSC 52 escape sequence format: \x1b]52;c;<base64_data>\x07
    let osc52_sequence = format!("\x1b]52;c;{}\x07", encoded);

    // Suspend raw mode temporarily to write the sequence
    let was_raw_mode = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    if was_raw_mode {
        if let Err(e) = crossterm::terminal::disable_raw_mode() {
            tracing::warn!("Failed to disable raw mode for OSC 52 clipboard: {}", e);
        }
    }

    // Write the sequence
    print!("{}", osc52_sequence);
    use std::io::Write;
    if let Err(e) = std::io::stdout().flush() {
        tracing::warn!("Failed to flush stdout for OSC 52 clipboard: {}", e);
    }

    // Restore raw mode if it was enabled
    if was_raw_mode {
        if let Err(e) = crossterm::terminal::enable_raw_mode() {
            tracing::warn!("Failed to re-enable raw mode after OSC 52 clipboard: {}", e);
        }
    }

    tracing::info!("Copied {} characters to clipboard via OSC 52", text.len());
    Ok(())
}

/// Copy text to clipboard using system clipboard (arboard)
///
/// Falls back to OSC 52 if system clipboard is unavailable.
///
pub fn copy_text_to_clipboard(text: &str) -> Result<()> {
    // Try system clipboard first (more reliable)
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => {
            clipboard
                .set_text(text.to_string())
                .map_err(|e| anyhow::anyhow!("Failed to set clipboard text: {}", e))?;

            tracing::info!("Copied {} characters to system clipboard", text.len());
            Ok(())
        }
        Err(e) => {
            tracing::warn!(
                "System clipboard unavailable ({}), falling back to OSC 52",
                e
            );
            // Fallback to OSC 52
            copy_text_to_clipboard_osc52(text)
        }
    }
}

/// Copy text to clipboard with both methods for maximum compatibility
///
/// Tries system clipboard (arboard), platform-specific tools, and OSC 52
/// to ensure the text is copied regardless of terminal capabilities.
///
pub fn copy_text_to_clipboard_both(text: &str) -> Result<()> {
    let mut last_error: Option<anyhow::Error> = None;
    // Prefixed with underscore to verify usage while suppressing false positive warning
    let _ = &last_error;

    // Try system clipboard first (most reliable)
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Err(e) = clipboard.set_text(text.to_string()) {
            last_error = Some(anyhow::anyhow!("System clipboard failed: {}", e));
        } else {
            tracing::info!("Copied {} characters via system clipboard", text.len());
            return Ok(());
        }
    } else {
        last_error = Some(anyhow::anyhow!("System clipboard unavailable"));
    }

    // Try platform-specific clipboard tools as fallback
    if let Ok(()) = copy_text_via_platform_tool(text) {
        tracing::info!(
            "Copied {} characters via platform clipboard tool",
            text.len()
        );
        return Ok(());
    }

    // Try OSC 52 as last resort
    if let Err(e) = copy_text_to_clipboard_osc52(text) {
        tracing::warn!("OSC 52 copy failed: {}", e);
        // Return error if everything failed
        if let Some(err) = last_error {
            return Err(err);
        }
    } else {
        tracing::info!("Copied {} characters via OSC 52", text.len());
        return Ok(());
    }

    // Should not reach here, but just in case
    last_error
        .map(Err)
        .unwrap_or(Err(anyhow::anyhow!("All clipboard methods failed")))
}

/// Copy text using platform-specific clipboard tools
///
/// This tries pbcopy (macOS), wl-copy (Wayland), xclip (X11), and clip.exe (Windows).
fn copy_text_via_platform_tool(text: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        if let Ok(mut child) = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(text.as_bytes())?;
                stdin.flush()?;
            }
            drop(child.stdin.take()); // Close stdin so pbcopy knows we're done
            let status = child.wait()?;
            if !status.success() {
                return Err(anyhow::anyhow!(
                    "pbcopy failed with exit code: {:?}",
                    status.code()
                ));
            }
            return Ok(());
        }

        Err(anyhow::anyhow!("pbcopy not available"))
    }

    #[cfg(target_os = "linux")]
    {
        // Try wl-copy (Wayland) first
        if let Ok(mut child) = std::process::Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(text.as_bytes())?;
                stdin.flush()?;
            }
            drop(child.stdin.take());
            let status = child.wait()?;
            if !status.success() {
                return Err(anyhow::anyhow!(
                    "wl-copy failed with exit code: {:?}",
                    status.code()
                ));
            }
            return Ok(());
        }

        // Try xclip (X11)
        if let Ok(mut child) = std::process::Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(text.as_bytes())?;
                stdin.flush()?;
            }
            drop(child.stdin.take());
            let status = child.wait()?;
            if !status.success() {
                return Err(anyhow::anyhow!(
                    "xclip failed with exit code: {:?}",
                    status.code()
                ));
            }
            return Ok(());
        }

        // Try xsel
        if let Ok(mut child) = std::process::Command::new("xsel")
            .args(["--clipboard", "--input"])
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(text.as_bytes())?;
                stdin.flush()?;
            }
            drop(child.stdin.take());
            let status = child.wait()?;
            if !status.success() {
                return Err(anyhow::anyhow!(
                    "xsel failed with exit code: {:?}",
                    status.code()
                ));
            }
            return Ok(());
        }

        Err(anyhow::anyhow!(
            "No clipboard tool available (wl-copy, xclip, xsel)"
        ))
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(mut child) = std::process::Command::new("clip.exe")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                // clip.exe expects trailing newline
                if let Err(e) = stdin.write_all(format!("{}\n", text).as_bytes()) {
                    tracing::warn!("Failed to write to clip.exe stdin: {}", e);
                }
                if let Err(e) = stdin.flush() {
                    tracing::warn!("Failed to flush clip.exe stdin: {}", e);
                }
            }
            if let Err(e) = child.wait() {
                tracing::warn!("clip.exe process wait failed: {}", e);
            }
            return Ok(());
        }

        return Err(anyhow::anyhow!("clip.exe not available"));
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(anyhow::anyhow!("Platform clipboard not supported"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_detection_png() {
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let format = ImageFormatType::from_magic_bytes(&png_header);
        assert_eq!(format, ImageFormatType::Png);
    }

    #[test]
    fn test_format_detection_jpeg() {
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x00, 0x00, 0x00];
        let format = ImageFormatType::from_magic_bytes(&jpeg_header);
        assert_eq!(format, ImageFormatType::Jpeg);
    }

    #[test]
    fn test_format_detection_gif() {
        let gif_header = [0x47, 0x49, 0x46, 0x38, 0x37, 0x61, 0x00, 0x00];
        let format = ImageFormatType::from_magic_bytes(&gif_header);
        assert_eq!(format, ImageFormatType::Gif);
    }

    #[test]
    fn test_format_detection_unknown() {
        let unknown_data = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let format = ImageFormatType::from_magic_bytes(&unknown_data);
        assert_eq!(format, ImageFormatType::Unknown);
    }

    #[test]
    fn test_mime_types() {
        assert_eq!(ImageFormatType::Png.mime_type(), "image/png");
        assert_eq!(ImageFormatType::Jpeg.mime_type(), "image/jpeg");
        assert_eq!(ImageFormatType::Gif.mime_type(), "image/gif");
        assert_eq!(ImageFormatType::WebP.mime_type(), "image/webp");
    }

    #[test]
    fn test_extensions() {
        assert_eq!(ImageFormatType::Png.extension(), "png");
        assert_eq!(ImageFormatType::Jpeg.extension(), "jpg");
        assert_eq!(ImageFormatType::Gif.extension(), "gif");
        assert_eq!(ImageFormatType::WebP.extension(), "webp");
    }

    #[test]
    fn test_size_too_large() {
        let large_data = vec![0u8; MAX_IMAGE_SIZE + 1];
        let result = ClipboardImage::new(large_data);
        assert!(result.is_err());
    }
}
