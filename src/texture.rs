//! # Texture Processing and Management
//!
//! This module provides utilities for working with textures in the glTF format.
//! It includes functionality for loading, converting, and encoding image data to be
//! used as textures in 3D models.
//!
//! The module supports:
//! - Loading textures from common image formats (PNG, JPEG, etc.)
//! - Converting between different color formats
//! - Encoding texture data for inclusion in glTF/GLB files
//! - Managing texture properties such as filtering, wrapping, and mipmaps
//!
//! ## Example
//!
//! ```rust
//! use mesh_tools::texture;
//! use image::DynamicImage;
//!
//! // Create a test pattern texture
//! let width = 512;
//! let height = 512;
//! let texture_image: DynamicImage = texture::create_uv_test_pattern(width, height).unwrap();
//! ```

use image::{DynamicImage, ImageBuffer, Rgba};
use std::io::Cursor;
use std::fmt;
use std::error::Error as StdError;

/// Error type for texture processing operations
///
/// Encapsulates errors that may occur during texture loading, conversion,
/// or encoding processes.
#[derive(Debug)]
pub enum TextureError {
    ImageError(image::ImageError),
    IoError(std::io::Error),
    /// A parameter was out of range (for example a zero checkerboard cell size)
    InvalidParameter(String),
}

impl fmt::Display for TextureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextureError::ImageError(err) => write!(f, "Image error: {}", err),
            TextureError::IoError(err) => write!(f, "IO error: {}", err),
            TextureError::InvalidParameter(msg) => write!(f, "Invalid parameter: {}", msg),
        }
    }
}

impl StdError for TextureError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            TextureError::ImageError(err) => Some(err),
            TextureError::IoError(err) => Some(err),
            TextureError::InvalidParameter(_) => None,
        }
    }
}

impl From<image::ImageError> for TextureError {
    fn from(err: image::ImageError) -> Self {
        TextureError::ImageError(err)
    }
}

impl From<std::io::Error> for TextureError {
    fn from(err: std::io::Error) -> Self {
        TextureError::IoError(err)
    }
}

/// Result type for texture operations
pub type Result<T> = std::result::Result<T, TextureError>;

/// Represents a texture format
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextureFormat {
    PNG,
    JPEG,
}

impl TextureFormat {
    /// Get the MIME type for this format
    pub fn mime_type(&self) -> &'static str {
        match self {
            TextureFormat::PNG => "image/png",
            TextureFormat::JPEG => "image/jpeg",
        }
    }
}

/// Generate a checkerboard pattern
///
/// # Errors
/// Returns [`TextureError::InvalidParameter`] if `cell_size` is 0.
pub fn generate_checkerboard(
    width: u32,
    height: u32,
    cell_size: u32,
    color1: Rgba<u8>,
    color2: Rgba<u8>,
) -> Result<DynamicImage> {
    if cell_size == 0 {
        return Err(TextureError::InvalidParameter("checkerboard cell_size must be at least 1".to_string()));
    }
    let mut img = ImageBuffer::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let cell_x = x / cell_size;
            let cell_y = y / cell_size;
            
            // If the sum of cell positions is even, use color1, otherwise color2
            let color = if (cell_x + cell_y) % 2 == 0 {
                color1
            } else {
                color2
            };
            
            img.put_pixel(x, y, color);
        }
    }

    Ok(DynamicImage::ImageRgba8(img))
}

/// Convert a dynamic image to PNG bytes
pub fn image_to_png_bytes(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut cursor = Cursor::new(&mut bytes);
    image.write_to(&mut cursor, image::ImageOutputFormat::Png)?;
    Ok(bytes)
}

/// Convert a dynamic image to JPEG bytes
pub fn image_to_jpeg_bytes(image: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut cursor = Cursor::new(&mut bytes);
    image.write_to(&mut cursor, image::ImageOutputFormat::Jpeg(quality))?;
    Ok(bytes)
}

/// Convert a dynamic image to bytes based on format
pub fn image_to_bytes(image: &DynamicImage, format: TextureFormat) -> Result<Vec<u8>> {
    match format {
        TextureFormat::PNG => image_to_png_bytes(image),
        TextureFormat::JPEG => image_to_jpeg_bytes(image, 90), // Default quality
    }
}

/// Create a simple UV test pattern (8x8 checkerboard)
///
/// # Errors
/// Returns [`TextureError::InvalidParameter`] if `width` is less than 8 (cells would be empty).
pub fn create_uv_test_pattern(width: u32, height: u32) -> Result<DynamicImage> {
    generate_checkerboard(
        width,
        height,
        width / 8, // 8x8 grid
        Rgba([255, 255, 255, 255]), // White
        Rgba([0, 0, 0, 255]),       // Black
    )
}

/// Create a colored checkerboard pattern
///
/// # Errors
/// Returns [`TextureError::InvalidParameter`] if `cell_size` is 0.
pub fn create_colored_checkerboard(
    width: u32, 
    height: u32,
    cell_size: u32,
    color1: [u8; 3],
    color2: [u8; 3],
) -> Result<DynamicImage> {
    generate_checkerboard(
        width,
        height,
        cell_size,
        Rgba([color1[0], color1[1], color1[2], 255]),
        Rgba([color2[0], color2[1], color2[2], 255]),
    )
}
