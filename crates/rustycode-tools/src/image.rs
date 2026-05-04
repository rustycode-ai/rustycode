//! Image Processing Pipeline
//!
//! Processes images for LLM consumption by resizing and compressing them
//! to fit within token budgets. Images are converted to JPEG format with
//! progressive compression when they exceed the budget.
//!
//! # Pipeline
//!
//! 1. Load image from memory buffer
//! 2. If any dimension > 1568px: resize proportionally (max dimension = 1568)
//! 3. Encode as JPEG quality 80
//! 4. If over token budget: re-encode at JPEG quality 40
//! 5. If still over: resize to max 400x400, re-encode at quality 20
//!
//! # Token Estimation
//!
//! Tokens are estimated as `(base64_len * 125) / 1000` to avoid floating-point
//! arithmetic while staying close to the actual ratio of ~0.125 tokens per
//! base64 character.

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::imageops::FilterType;

/// Maximum dimension for the initial resize step.
const MAX_DIMENSION_FULL: u32 = 1568;

/// Maximum dimensions for the aggressive resize step.
const MAX_DIMENSION_AGGRESSIVE: u32 = 400;

/// JPEG quality for the full compression level.
const JPEG_QUALITY_FULL: u8 = 80;

/// JPEG quality for the medium compression level.
const JPEG_QUALITY_MEDIUM: u8 = 40;

/// JPEG quality for the aggressive compression level.
const JPEG_QUALITY_AGGRESSIVE: u8 = 20;

/// Default token budget for processed images.
pub const DEFAULT_MAX_TOKENS: usize = 16_000;

/// Result of image processing.
#[derive(Debug, Clone)]
pub struct ProcessedImage {
    /// Base64-encoded image data.
    pub base64_data: String,
    /// MIME type of the output image (always "image/jpeg").
    pub media_type: String,
    /// Original image dimensions (width, height).
    pub original_dimensions: (u32, u32),
    /// Output image dimensions (width, height) after resizing.
    pub output_dimensions: (u32, u32),
    /// Original buffer size in bytes.
    pub original_size: usize,
    /// Output size in bytes (base64 string length).
    pub output_size: usize,
    /// Compression level applied.
    pub compression_level: CompressionLevel,
}

/// Compression level applied during image processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompressionLevel {
    /// Full quality: resize to max 1568px + JPEG quality 80.
    Full,
    /// Medium quality: JPEG quality 40 (no resize beyond initial).
    Medium,
    /// Aggressive quality: resize to max 400x400 + JPEG quality 20.
    Aggressive,
}

/// Encode a `DynamicImage` as JPEG with the given quality.
fn encode_jpeg(img: &image::DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    img.write_with_encoder(encoder)
        .with_context(|| "failed to encode image as JPEG")?;
    Ok(buf)
}

/// Resize an image so its maximum dimension does not exceed `max_dim`.
/// Returns the image unchanged if already within bounds.
fn resize_if_needed(img: &image::DynamicImage, max_dim: u32) -> image::DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_dim && h <= max_dim {
        return img.clone();
    }

    let scale = max_dim as f64 / w.max(h) as f64;
    let new_w = ((w as f64 * scale).round() as u32).max(1);
    let new_h = ((h as f64 * scale).round() as u32).max(1);

    let resized = image::imageops::resize(img, new_w, new_h, FilterType::Lanczos3);
    image::DynamicImage::ImageRgba8(resized)
}

/// Estimate the number of tokens from base64 length.
///
/// Uses integer arithmetic: `(base64_len * 125) / 1000` which avoids
/// floating-point while approximating `base64_len * 0.125`.
pub fn estimate_tokens(base64_len: usize) -> usize {
    (base64_len * 125) / 1000
}

/// Process an image buffer for LLM consumption.
///
/// The pipeline progressively compresses the image to fit within
/// `max_tokens` estimated tokens. Token estimation uses
/// `estimate_tokens(base64_len)`.
///
/// # Arguments
///
/// * `buffer` - Raw image bytes (PNG, JPEG, GIF, `WebP`, etc.)
/// * `max_tokens` - Target token budget (use `DEFAULT_MAX_TOKENS` for 16k)
///
/// # Errors
///
/// Returns an error if the buffer is not a valid image format.
pub fn process_image(buffer: &[u8], max_tokens: usize) -> Result<ProcessedImage> {
    let original_size = buffer.len();

    let img = image::load_from_memory(buffer)
        .with_context(|| "failed to load image from buffer")?;

    let original_dimensions = (img.width(), img.height());

    // Step 1: Resize if any dimension > 1568, encode at quality 80
    let resized = resize_if_needed(&img, MAX_DIMENSION_FULL);
    let jpeg_full = encode_jpeg(&resized, JPEG_QUALITY_FULL)?;
    let base64_full = STANDARD.encode(&jpeg_full);
    let output_dimensions = (resized.width(), resized.height());

    // Check if full quality fits the budget
    if estimate_tokens(base64_full.len()) <= max_tokens {
        let output_size = base64_full.len();
        return Ok(ProcessedImage {
            base64_data: base64_full,
            media_type: "image/jpeg".to_string(),
            original_dimensions,
            output_dimensions,
            original_size,
            output_size,
            compression_level: CompressionLevel::Full,
        });
    }

    // Step 2: Re-encode at quality 40
    let jpeg_medium = encode_jpeg(&resized, JPEG_QUALITY_MEDIUM)?;
    let base64_medium = STANDARD.encode(&jpeg_medium);

    if estimate_tokens(base64_medium.len()) <= max_tokens {
        let output_size = base64_medium.len();
        return Ok(ProcessedImage {
            base64_data: base64_medium,
            media_type: "image/jpeg".to_string(),
            original_dimensions,
            output_dimensions,
            original_size,
            output_size,
            compression_level: CompressionLevel::Medium,
        });
    }

    // Step 3: Resize to max 400x400, encode at quality 20
    let aggressive = resize_if_needed(&resized, MAX_DIMENSION_AGGRESSIVE);
    let jpeg_aggressive = encode_jpeg(&aggressive, JPEG_QUALITY_AGGRESSIVE)?;
    let base64_aggressive = STANDARD.encode(&jpeg_aggressive);
    let output_size = base64_aggressive.len();

    Ok(ProcessedImage {
        base64_data: base64_aggressive,
        media_type: "image/jpeg".to_string(),
        original_dimensions,
        output_dimensions: (aggressive.width(), aggressive.height()),
        original_size,
        output_size,
        compression_level: CompressionLevel::Aggressive,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Create a minimal valid JPEG image of the given dimensions.
    fn create_test_image(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(width, height, image::Rgb([128u8, 128, 128]));
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Jpeg)
            .unwrap();
        buf
    }

    #[test]
    fn test_process_small_image() {
        // 10x10 image should fit easily at full quality
        let buffer = create_test_image(10, 10);
        let result = process_image(&buffer, DEFAULT_MAX_TOKENS).unwrap();

        assert_eq!(result.compression_level, CompressionLevel::Full);
        assert_eq!(result.original_dimensions, (10, 10));
        assert_eq!(result.media_type, "image/jpeg");
        assert!(!result.base64_data.is_empty());
        assert!(estimate_tokens(result.base64_data.len()) <= DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn test_process_large_image_resizes() {
        // 3000x2000 image should be resized to max 1568
        let buffer = create_test_image(3000, 2000);
        let result = process_image(&buffer, DEFAULT_MAX_TOKENS).unwrap();

        assert!(result.output_dimensions.0 <= MAX_DIMENSION_FULL);
        assert!(result.output_dimensions.1 <= MAX_DIMENSION_FULL);
        assert_eq!(result.original_dimensions, (3000, 2000));
    }

    #[test]
    fn test_process_image_fallback_medium() {
        // Use a very tight budget so that quality 80 fails but quality 40 succeeds.
        // Create a larger image so the base64 is substantial.
        let buffer = create_test_image(800, 800);
        let result_full = process_image(&buffer, DEFAULT_MAX_TOKENS).unwrap();
        let tokens_full = estimate_tokens(result_full.base64_data.len());

        // Encode at medium to check if it would be smaller
        let img = image::load_from_memory(&buffer).unwrap();
        let resized = resize_if_needed(&img, MAX_DIMENSION_FULL);
        let jpeg_medium = encode_jpeg(&resized, JPEG_QUALITY_MEDIUM).unwrap();
        let base64_medium = STANDARD.encode(&jpeg_medium);
        let tokens_medium = estimate_tokens(base64_medium.len());

        // Only test medium fallback if quality difference is meaningful
        if tokens_medium < tokens_full {
            // Use a budget between medium and full tokens
            let budget = tokens_medium + (tokens_full - tokens_medium) / 2;
            let result = process_image(&buffer, budget).unwrap();

            // Should use at least medium (could be full if it fits)
            assert!(estimate_tokens(result.base64_data.len()) <= budget);
        }
    }

    #[test]
    fn test_process_image_fallback_aggressive() {
        // Use a very tiny budget to force aggressive compression
        let buffer = create_test_image(800, 800);

        // Budget so small it should force aggressive compression
        let result = process_image(&buffer, 50).unwrap();

        assert_eq!(result.compression_level, CompressionLevel::Aggressive);
        assert!(result.output_dimensions.0 <= MAX_DIMENSION_AGGRESSIVE);
        assert!(result.output_dimensions.1 <= MAX_DIMENSION_AGGRESSIVE);
    }

    #[test]
    fn test_process_invalid_image_data() {
        let result = process_image(b"not an image", DEFAULT_MAX_TOKENS);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to load image"));
    }

    #[test]
    fn test_token_estimation() {
        // Verify the estimation formula: (len * 125) / 1000
        assert_eq!(estimate_tokens(0), 0);
        assert_eq!(estimate_tokens(1000), 125);
        assert_eq!(estimate_tokens(8000), 1000);
        assert_eq!(estimate_tokens(10000), 1250);
        assert_eq!(estimate_tokens(128_000), 16_000);
    }

    #[test]
    fn test_resize_if_needed_no_resize() {
        let img = image::DynamicImage::ImageRgb8(
            image::RgbImage::from_pixel(100, 100, image::Rgb([0u8, 0, 0])),
        );
        let result = resize_if_needed(&img, 200);
        assert_eq!(result.width(), 100);
        assert_eq!(result.height(), 100);
    }

    #[test]
    fn test_resize_if_needed_resizes() {
        let img = image::DynamicImage::ImageRgb8(
            image::RgbImage::from_pixel(2000, 1000, image::Rgb([0u8, 0, 0])),
        );
        let result = resize_if_needed(&img, 1000);
        assert!(result.width() <= 1000);
        assert!(result.height() <= 1000);
        // Aspect ratio preserved: 2:1 → width should be ~1000, height ~500
        assert_eq!(result.width(), 1000);
        assert_eq!(result.height(), 500);
    }
}
