//! # Error Handling
//!
//! This module defines the error types and result wrappers used throughout the library.
//! It centralizes error handling to provide consistent and meaningful error messages
//! for various failure scenarios that might occur during glTF creation and export.
//!
//! The `GltfError` enum covers errors from various sources including:
//! - I/O operations
//! - JSON serialization/deserialization
//! - Invalid data or parameters
//! - Texture processing issues
//!
//! The module also provides a convenient `Result` type alias for functions
//! that may return a `GltfError`.

use std::io;
use thiserror::Error;
use crate::texture;

/// Comprehensive error type for glTF export operations
#[derive(Error, Debug)]
pub enum GltfError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Invalid data: {0}")]
    InvalidData(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    /// A material failed a check; `code` is a stable `MATERIAL_*` validator code.
    /// `index` is the material's index in the document, or `None` before it has one.
    #[error("{code}: material{} {message}", index.map(|i| format!(" {i}")).unwrap_or_default())]
    InvalidMaterial { index: Option<usize>, code: &'static str, message: String },

    #[error("Invalid index")]
    InvalidIndex,
    
    #[error("Texture error: {0:?}")]
    Texture(#[from] texture::TextureError),
}

/// Result type for glTF export operations
pub type Result<T> = std::result::Result<T, GltfError>;
