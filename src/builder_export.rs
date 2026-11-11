//! Buffer encodings: GLB BIN chunk, base64 data URI, or an external `.bin` file.
//!
//! The builder always holds its binary data in memory and never stores a `uri` on buffer 0
//! (validation reports one as `BUFFER_URI_ON_GLB_BUFFER`). The encoding is chosen at export time
//! and the `uri` is injected into a serialized copy only.

use std::path::Path;

use crate::builder::GltfBuilder;
use crate::error::{GltfError, Result};
use crate::uri;

/// Where the binary buffer goes when exporting a `.gltf`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferEncoding {
    /// No `uri`: the data is the GLB BIN chunk. `GltfParts::bin` holds the bytes.
    Glb,
    /// Base64 `data:` URI inside the JSON. `GltfParts::bin` is empty.
    DataUri,
    /// Relative `uri` to a separate file. `bin_file_name` is a bare file name (no path
    /// separators); the caller writes `GltfParts::bin` under that name next to the `.gltf`.
    External { bin_file_name: String },
}

/// The pieces of an exported document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GltfParts {
    /// The glTF JSON text.
    pub json: String,
    /// Binary buffer bytes for `Glb` and `External`; empty for `DataUri` or when there is no data.
    pub bin: Vec<u8>,
}

impl GltfBuilder {
    /// Validate the document and split it into JSON and binary data for `encoding`.
    ///
    /// Pure: nothing is written and the builder is not modified.
    ///
    /// # Errors
    /// `GltfError::InvalidData` if validation fails, or with `BUFFER_URI_INVALID` if an external
    /// file name is empty, a path, or has unsafe characters.
    ///
    /// ```
    /// use mesh_tools::{BufferEncoding, GltfBuilder};
    /// let mut b = GltfBuilder::new();
    /// b.create_box(1.0);
    /// let parts = b.to_gltf_parts(&BufferEncoding::External { bin_file_name: "box.bin".into() }).unwrap();
    /// assert!(parts.json.contains("\"uri\":\"box.bin\""));
    /// assert!(!parts.bin.is_empty());
    /// let err = b.to_gltf_parts(&BufferEncoding::External { bin_file_name: "../box.bin".into() });
    /// assert!(err.unwrap_err().to_string().contains("BUFFER_URI_INVALID"));
    /// ```
    pub fn to_gltf_parts(&self, encoding: &BufferEncoding) -> Result<GltfParts> {
        if let BufferEncoding::External { bin_file_name } = encoding {
            uri::check_file_name(bin_file_name)
                .map_err(|e| GltfError::InvalidData(format!("BUFFER_URI_INVALID: {e}")))?;
        }
        self.ensure_valid()?;
        let has_data = !self.buffer_data.is_empty();
        let (buffer_uri, bin) = match encoding {
            _ if !has_data => (None, Vec::new()),
            BufferEncoding::Glb => (None, self.buffer_data.clone()),
            BufferEncoding::DataUri => (Some(uri::encode_data_uri("application/octet-stream", &self.buffer_data)), Vec::new()),
            BufferEncoding::External { bin_file_name } => (Some(uri::percent_encode(bin_file_name)), self.buffer_data.clone()),
        };
        Ok(GltfParts { json: self.serialize_json(buffer_uri.as_deref())?, bin })
    }

    /// Validate and serialize to a single self-contained `.gltf` string (buffer as a base64 data URI).
    ///
    /// # Errors
    /// `GltfError::InvalidData` if validation fails.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// b.create_box(1.0);
    /// assert!(b.to_gltf_string_embedded().unwrap().contains("data:application/octet-stream;base64,"));
    /// ```
    pub fn to_gltf_string_embedded(&self) -> Result<String> {
        Ok(self.to_gltf_parts(&BufferEncoding::DataUri)?.json)
    }

    /// Validate and write `path` (`.gltf`) plus `<stem>.bin` next to it.
    ///
    /// Each file is written to a `.tmp` file and renamed into place, so a failure before the renames
    /// (invalid document, missing directory) leaves no files. The `.bin` is renamed first so the
    /// `.gltf` never points at a missing file. A document with no binary data writes only the `.gltf`.
    ///
    /// # Errors
    /// `GltfError::InvalidData` for validation failures, `GltfError::InvalidInput` if `path` has no
    /// UTF-8 file name or `<stem>.bin` would be `path` itself, `GltfError::Io` for file errors.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// b.create_box(1.0);
    /// assert!(b.export_gltf("/nonexistent-dir/box.gltf").is_err());
    /// ```
    pub fn export_gltf<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            GltfError::InvalidInput(format!("{} has no UTF-8 file name", path.display()))
        })?;
        let bin_name = format!("{stem}.bin");
        let bin_path = path.with_file_name(&bin_name);
        if bin_path == path {
            return Err(GltfError::InvalidInput(format!(
                "{} would overwrite itself with its .bin; use a .gltf extension", path.display()
            )));
        }
        let parts = self.to_gltf_parts(&BufferEncoding::External { bin_file_name: bin_name })?;
        if parts.bin.is_empty() {
            write_atomic(&[(path, parts.json.as_bytes())])
        } else {
            write_atomic(&[(&bin_path, &parts.bin), (path, parts.json.as_bytes())])
        }
    }

    /// Validate and write a single self-contained `.gltf` (buffer as a data URI), atomically via a
    /// `.tmp` file and rename.
    ///
    /// # Errors
    /// `GltfError::InvalidData` for validation failures, `GltfError::Io` for file errors.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// b.create_box(1.0);
    /// assert!(b.export_gltf_embedded("/nonexistent-dir/box.gltf").is_err());
    /// ```
    pub fn export_gltf_embedded<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = self.to_gltf_string_embedded()?;
        write_atomic(&[(path.as_ref(), json.as_bytes())])
    }
}

/// Write every file to `<name>.tmp`, then rename them in order. Temp files are removed on failure.
fn write_atomic(files: &[(&Path, &[u8])]) -> Result<()> {
    let tmp = |p: &Path| {
        let mut n = p.as_os_str().to_owned();
        n.push(".tmp");
        std::path::PathBuf::from(n)
    };
    let cleanup = |upto: usize| files[..upto].iter().for_each(|(p, _)| { let _ = std::fs::remove_file(tmp(p)); });
    for (i, (p, data)) in files.iter().enumerate() {
        if let Err(e) = std::fs::write(tmp(p), data) {
            cleanup(i + 1);
            return Err(e.into());
        }
    }
    for (p, _) in files {
        if let Err(e) = std::fs::rename(tmp(p), p) {
            cleanup(files.len());
            return Err(e.into());
        }
    }
    Ok(())
}
