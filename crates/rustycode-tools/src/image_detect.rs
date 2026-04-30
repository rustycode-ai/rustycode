//! Image File Detection
//!
//! Detects and validates image files by examining magic bytes (file headers),
//! not just file extensions. This prevents spoofed extensions from being
//! treated as images.
//!
//! Inspired by goose's `is_image_file` and `load_image_file` in `providers/utils.rs`.
//!
//! # Supported Formats
//!
//! - PNG (magic: `89 50 4E 47`)
//! - JPEG (magic: `FF D8 FF`)
//! - GIF (magic: `47 49 46 38`)
//! - `WebP` (magic: `52 49 46 46...57 45 42 50`)
//! - BMP (magic: `42 4D`)
//! - ICO (magic: `00 00 01 00`)
//!
//! # Example
//!
//! ```
//! use rustycode_tools::image_detect::{detect_image_type, ImageType};
//!
//! // PNG magic bytes
//! let png_bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
//! assert_eq!(detect_image_type(&png_bytes), Some(ImageType::Png));
//!
//! // Not an image
//! let text_bytes = b"Hello, world!";
//! assert_eq!(detect_image_type(text_bytes), None);
//! ```

use std::io::Read;
use std::path::{Path, PathBuf};

/// Supported image types with their MIME types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum ImageType {
    Png,
    Jpeg,
    Gif,
    WebP,
    Bmp,
    Ico,
}

impl ImageType {
    /// Get the MIME type for this image format.
    pub const fn mime_type(&self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
            Self::Bmp => "image/bmp",
            Self::Ico => "image/x-icon",
        }
    }

    /// Get the typical file extension (without dot).
    pub const fn extension(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Gif => "gif",
            Self::WebP => "webp",
            Self::Bmp => "bmp",
            Self::Ico => "ico",
        }
    }

    /// Minimum number of bytes needed to detect this image type.
    pub const fn min_header_bytes(&self) -> usize {
        match self {
            Self::Png => 4,
            Self::Jpeg => 3,
            Self::Gif => 4,
            Self::WebP => 12,
            Self::Bmp => 2,
            Self::Ico => 4,
        }
    }
}

impl std::fmt::Display for ImageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.extension())
    }
}

/// Detect image type from magic bytes (file header).
///
/// Examines the first few bytes to identify the image format.
/// Returns `None` if the bytes don't match any known image format.
///
/// # Example
///
/// ```
/// use rustycode_tools::image_detect::{detect_image_type, ImageType};
///
/// let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
/// assert_eq!(detect_image_type(&png), Some(ImageType::Png));
///
/// let jpeg = [0xFF, 0xD8, 0xFF, 0xE0];
/// assert_eq!(detect_image_type(&jpeg), Some(ImageType::Jpeg));
///
/// let text = b"Hello, world!";
/// assert_eq!(detect_image_type(text), None);
/// ```
pub fn detect_image_type(bytes: &[u8]) -> Option<ImageType> {
    if bytes.len() < 2 {
        return None;
    }

    // BMP: "BM" (42 4D)
    if bytes.len() >= 2 && bytes[0] == 0x42 && bytes[1] == 0x4D {
        return Some(ImageType::Bmp);
    }

    // JPEG: FF D8 FF
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Some(ImageType::Jpeg);
    }

    // PNG: 89 50 4E 47 (‰PNG)
    if bytes.len() >= 4
        && bytes[0] == 0x89
        && bytes[1] == 0x50
        && bytes[2] == 0x4E
        && bytes[3] == 0x47
    {
        return Some(ImageType::Png);
    }

    // GIF: "GIF8" (47 49 46 38)
    if bytes.len() >= 4
        && bytes[0] == 0x47
        && bytes[1] == 0x49
        && bytes[2] == 0x46
        && bytes[3] == 0x38
    {
        return Some(ImageType::Gif);
    }

    // ICO: 00 00 01 00
    if bytes.len() >= 4
        && bytes[0] == 0x00
        && bytes[1] == 0x00
        && bytes[2] == 0x01
        && bytes[3] == 0x00
    {
        return Some(ImageType::Ico);
    }

    // WebP: RIFF....WEBP
    // "RIFF" = 52 49 46 46, "WEBP" at offset 8 = 57 45 42 50
    if bytes.len() >= 12 {
        let is_riff = bytes[0] == 0x52 && bytes[1] == 0x49 && bytes[2] == 0x46 && bytes[3] == 0x46;
        let is_webp =
            bytes[8] == 0x57 && bytes[9] == 0x45 && bytes[10] == 0x42 && bytes[11] == 0x50;
        if is_riff && is_webp {
            return Some(ImageType::WebP);
        }
    }

    None
}

/// Check if a file is an image by examining its magic bytes.
///
/// Reads the first 12 bytes of the file and checks against known image signatures.
/// Returns `false` if the file cannot be read or doesn't match any image format.
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::image_detect::is_image_file;
///
/// if is_image_file("/path/to/photo.png") {
///     println!("It's a real image!");
/// }
/// ```
pub fn is_image_file(path: &Path) -> bool {
    if let Ok(mut file) = std::fs::File::open(path) {
        let mut buffer = [0u8; 12];
        if file.read(&mut buffer).is_ok() {
            return detect_image_type(&buffer).is_some();
        }
    }
    false
}

/// Detect the image type of a file by reading its magic bytes.
///
/// Returns `None` if the file cannot be read or doesn't match any known format.
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::image_detect::detect_file_image_type;
///
/// if let Some(img_type) = detect_file_image_type("/path/to/photo.webp") {
///     println!("Image type: {} ({})", img_type, img_type.mime_type());
/// }
/// ```
pub fn detect_file_image_type(path: &Path) -> Option<ImageType> {
    if let Ok(mut file) = std::fs::File::open(path) {
        let mut buffer = [0u8; 12];
        if let Ok(n) = file.read(&mut buffer) {
            return detect_image_type(&buffer[..n]);
        }
    }
    None
}

/// Guess image type from file extension (without reading the file).
///
/// Less reliable than magic byte detection but useful when file access
/// isn't available. Returns `None` for unknown extensions.
///
/// # Example
///
/// ```
/// use rustycode_tools::image_detect::image_type_from_extension;
/// use std::path::Path;
///
/// assert_eq!(image_type_from_extension(Path::new("photo.png")), Some(rustycode_tools::image_detect::ImageType::Png));
/// assert_eq!(image_type_from_extension(Path::new("photo.JPG")), Some(rustycode_tools::image_detect::ImageType::Jpeg));
/// assert_eq!(image_type_from_extension(Path::new("document.pdf")), None);
/// ```
pub fn image_type_from_extension(path: &Path) -> Option<ImageType> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "png" => Some(ImageType::Png),
        "jpg" | "jpeg" => Some(ImageType::Jpeg),
        "gif" => Some(ImageType::Gif),
        "webp" => Some(ImageType::WebP),
        "bmp" => Some(ImageType::Bmp),
        "ico" => Some(ImageType::Ico),
        _ => None,
    }
}

/// Read image file and return its bytes along with detected type.
///
/// Validates that the file is actually an image (by magic bytes) before reading.
/// Returns an error if:
/// - The file doesn't exist
/// - The file isn't a recognized image format
/// - The file can't be read
///
/// # Example
///
/// ```ignore
/// use rustycode_tools::image_detect::read_image_file;
///
/// let (bytes, img_type) = read_image_file("/path/to/photo.png")?;
/// println!("Read {} bytes of {} image", bytes.len(), img_type);
/// ```
pub fn read_image_file(path: &Path) -> Result<(Vec<u8>, ImageType), ImageError> {
    if !path.exists() {
        return Err(ImageError::FileNotFound(path.to_owned()));
    }

    let mut file = std::fs::File::open(path).map_err(|e| ImageError::IoError(e.to_string()))?;

    let mut header = [0u8; 12];
    let header_len = file
        .read(&mut header)
        .map_err(|e| ImageError::IoError(e.to_string()))?;

    let img_type = detect_image_type(&header[..header_len])
        .ok_or_else(|| ImageError::NotAnImage(path.to_owned()))?;

    // Read the rest of the file
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&header[..header_len]);
    file.read_to_end(&mut bytes)
        .map_err(|e| ImageError::IoError(e.to_string()))?;

    Ok((bytes, img_type))
}

/// Errors that can occur when reading image files.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ImageError {
    /// The file doesn't exist at the given path
    FileNotFound(PathBuf),
    /// The file exists but its magic bytes don't match any image format
    NotAnImage(PathBuf),
    /// An I/O error occurred while reading
    IoError(String),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileNotFound(path) => write!(f, "File not found: {}", path.display()),
            Self::NotAnImage(path) => write!(f, "Not a valid image file: {}", path.display()),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl std::error::Error for ImageError {}

/// Detect image paths in text by looking for file paths with image extensions.
///
/// Scans text for words that look like absolute paths ending in image extensions,
/// then optionally verifies them by checking magic bytes.
///
/// Inspired by goose's `detect_image_path` in `providers/utils.rs`.
///
/// # Example
///
/// ```text
/// // See tests for usage examples
/// ```
pub fn detect_image_paths_in_text(text: &str, verify_magic_bytes: bool) -> Vec<String> {
    let extensions = [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".ico"];
    let mut found = Vec::new();

    for word in text.split_whitespace() {
        // Clean up trailing punctuation
        let cleaned = word.trim_end_matches(&[')', ']', ',', '.', ';', ':', '!', '?'][..]);

        if extensions
            .iter()
            .any(|ext| cleaned.to_lowercase().ends_with(ext))
        {
            let path = Path::new(cleaned);
            if path.is_absolute() && path.is_file() {
                if verify_magic_bytes {
                    if is_image_file(path) {
                        found.push(cleaned.to_string());
                    }
                } else {
                    found.push(cleaned.to_string());
                }
            }
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_png() {
        let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(detect_image_type(&bytes), Some(ImageType::Png));
    }

    #[test]
    fn test_detect_jpeg() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(detect_image_type(&bytes), Some(ImageType::Jpeg));
    }

    #[test]
    fn test_detect_gif() {
        let bytes = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61]; // GIF89a
        assert_eq!(detect_image_type(&bytes), Some(ImageType::Gif));

        let bytes2 = [0x47, 0x49, 0x46, 0x38, 0x37, 0x61]; // GIF87a
        assert_eq!(detect_image_type(&bytes2), Some(ImageType::Gif));
    }

    #[test]
    fn test_detect_webp() {
        // RIFF header + file size (4 bytes) + WEBP
        let bytes = [
            0x52, 0x49, 0x46, 0x46, // RIFF
            0x00, 0x00, 0x00, 0x00, // file size
            0x57, 0x45, 0x42, 0x50, // WEBP
        ];
        assert_eq!(detect_image_type(&bytes), Some(ImageType::WebP));
    }

    #[test]
    fn test_detect_bmp() {
        let bytes = [0x42, 0x4D, 0x00, 0x00]; // BM
        assert_eq!(detect_image_type(&bytes), Some(ImageType::Bmp));
    }

    #[test]
    fn test_detect_ico() {
        let bytes = [0x00, 0x00, 0x01, 0x00];
        assert_eq!(detect_image_type(&bytes), Some(ImageType::Ico));
    }

    #[test]
    fn test_detect_not_image() {
        assert_eq!(detect_image_type(b"Hello, world!"), None);
        assert_eq!(detect_image_type(b"<html>"), None);
        assert_eq!(detect_image_type(b"%PDF-1.4"), None);
        assert_eq!(detect_image_type(&[]), None);
        assert_eq!(detect_image_type(&[0x00]), None);
    }

    #[test]
    fn test_image_type_mime() {
        assert_eq!(ImageType::Png.mime_type(), "image/png");
        assert_eq!(ImageType::Jpeg.mime_type(), "image/jpeg");
        assert_eq!(ImageType::Gif.mime_type(), "image/gif");
        assert_eq!(ImageType::WebP.mime_type(), "image/webp");
        assert_eq!(ImageType::Bmp.mime_type(), "image/bmp");
        assert_eq!(ImageType::Ico.mime_type(), "image/x-icon");
    }

    #[test]
    fn test_image_type_extension() {
        assert_eq!(ImageType::Png.extension(), "png");
        assert_eq!(ImageType::Jpeg.extension(), "jpg");
        assert_eq!(ImageType::Gif.extension(), "gif");
        assert_eq!(ImageType::WebP.extension(), "webp");
    }

    #[test]
    fn test_image_type_from_extension() {
        assert_eq!(
            image_type_from_extension(Path::new("photo.png")),
            Some(ImageType::Png)
        );
        assert_eq!(
            image_type_from_extension(Path::new("photo.JPG")),
            Some(ImageType::Jpeg)
        );
        assert_eq!(
            image_type_from_extension(Path::new("photo.jpeg")),
            Some(ImageType::Jpeg)
        );
        assert_eq!(
            image_type_from_extension(Path::new("anim.gif")),
            Some(ImageType::Gif)
        );
        assert_eq!(image_type_from_extension(Path::new("doc.pdf")), None);
        assert_eq!(image_type_from_extension(Path::new("no_extension")), None);
    }

    #[test]
    fn test_is_image_file_real_png() {
        let temp_dir = tempfile::tempdir().unwrap();
        let png_path = temp_dir.path().join("test.png");
        let png_data = [
            0x89, 0x50, 0x4E, 0x47, // PNG magic
            0x0D, 0x0A, 0x1A, 0x0A, // PNG header
            0x00, 0x00, 0x00, 0x0D, // Rest of fake PNG
        ];
        std::fs::write(&png_path, png_data).unwrap();
        assert!(is_image_file(&png_path));
    }

    #[test]
    fn test_is_image_file_fake_png() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fake_path = temp_dir.path().join("fake.png");
        std::fs::write(&fake_path, b"not a real png").unwrap();
        assert!(!is_image_file(&fake_path));
    }

    #[test]
    fn test_is_image_file_nonexistent() {
        assert!(!is_image_file(Path::new("/nonexistent/path/image.png")));
    }

    #[test]
    fn test_detect_file_image_type() {
        let temp_dir = tempfile::tempdir().unwrap();
        let jpeg_path = temp_dir.path().join("test.jpg");
        let jpeg_data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        std::fs::write(&jpeg_path, jpeg_data).unwrap();
        assert_eq!(detect_file_image_type(&jpeg_path), Some(ImageType::Jpeg));
    }

    #[test]
    fn test_read_image_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let png_path = temp_dir.path().join("test.png");
        let png_data = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG header
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        ];
        std::fs::write(&png_path, png_data).unwrap();

        let (bytes, img_type) = read_image_file(&png_path).unwrap();
        assert_eq!(img_type, ImageType::Png);
        assert_eq!(bytes.len(), 16);
    }

    #[test]
    fn test_read_image_file_not_image() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fake_path = temp_dir.path().join("fake.png");
        std::fs::write(&fake_path, b"This is not an image").unwrap();

        let result = read_image_file(&fake_path);
        assert!(matches!(result, Err(ImageError::NotAnImage(_))));
    }

    #[test]
    fn test_read_image_file_not_found() {
        let result = read_image_file(Path::new("/nonexistent/image.png"));
        assert!(matches!(result, Err(ImageError::FileNotFound(_))));
    }

    #[test]
    fn test_detect_image_paths_in_text() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create a real PNG file
        let png_path = temp_dir.path().join("photo.png");
        let png_data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        std::fs::write(&png_path, png_data).unwrap();
        let png_path_str = png_path.to_str().unwrap();

        // Create a fake image (wrong magic bytes but .png extension)
        let fake_path = temp_dir.path().join("fake.png");
        std::fs::write(&fake_path, b"not an image").unwrap();
        let fake_path_str = fake_path.to_str().unwrap();

        // Test without verification — finds all extension matches
        let text = format!("Check {} and {}", png_path_str, fake_path_str);
        let paths = detect_image_paths_in_text(&text, false);
        assert_eq!(paths.len(), 2);

        // Test with verification — only real images
        let paths = detect_image_paths_in_text(&text, true);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], png_path_str);
    }

    #[test]
    fn test_detect_image_paths_no_absolute() {
        let text = "See relative/path/image.png for details";
        let paths = detect_image_paths_in_text(text, false);
        assert!(paths.is_empty()); // Not absolute paths
    }

    #[test]
    fn test_detect_image_paths_nonexistent() {
        let text = "Look at /nonexistent/image.png";
        let paths = detect_image_paths_in_text(text, false);
        assert!(paths.is_empty()); // File doesn't exist
    }

    #[test]
    fn test_image_error_display() {
        let err = ImageError::FileNotFound(PathBuf::from("/tmp/missing.png"));
        assert!(err.to_string().contains("File not found"));

        let err = ImageError::NotAnImage(PathBuf::from("/tmp/fake.png"));
        assert!(err.to_string().contains("Not a valid image"));

        let err = ImageError::IoError("permission denied".to_string());
        assert!(err.to_string().contains("I/O error"));
    }
}
