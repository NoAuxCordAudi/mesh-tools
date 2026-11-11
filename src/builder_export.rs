//! Buffer encodings: GLB BIN chunk, base64 data URI, or an external `.bin` file.
//!
//! The builder always holds its binary data in memory and never stores a `uri` on buffer 0
//! (validation reports one as `BUFFER_URI_ON_GLB_BUFFER`). The encoding is chosen at export time
//! and the `uri` is injected into a serialized copy only.

use std::path::{Path, PathBuf};

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

/// Where buffers other than buffer 0 go. Buffer 0 follows the [`BufferEncoding`] (or is the GLB
/// BIN chunk); the builder never stores a `uri` for any buffer it holds bytes for.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtraBuffers {
    /// Separate files named `{prefix}{index}.bin` (buffer 1 with prefix `"m_"` is `m_1.bin`),
    /// returned in `extra_bins` for the caller to write next to the document.
    External { prefix: String },
    /// Base64 `data:` URIs inside the JSON; `extra_bins` stays empty.
    DataUri,
}

/// The pieces of an exported document.
///
/// `#[non_exhaustive]`: build it only by exporting. Since 0.22.0 it has `extra_bins`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GltfParts {
    /// The glTF JSON text.
    pub json: String,
    /// Binary buffer bytes for `Glb` and `External`; empty for `DataUri` or when there is no data.
    pub bin: Vec<u8>,
    /// `(file name, bytes)` of buffers 1.. for [`ExtraBuffers::External`], in buffer order.
    /// Empty for single-buffer documents and for [`ExtraBuffers::DataUri`].
    pub extra_bins: Vec<(String, Vec<u8>)>,
}

/// A GLB file plus the external files its extra buffers refer to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct GlbParts {
    /// The GLB container (buffer 0 is the BIN chunk).
    pub glb: Vec<u8>,
    /// `(file name, bytes)` of buffers 1.. for [`ExtraBuffers::External`], in buffer order.
    pub extra_bins: Vec<(String, Vec<u8>)>,
}

/// Per-buffer `uri`s to inject into the output, and the files to write.
struct Resolved {
    uris: Vec<Option<String>>,
    extra_bins: Vec<(String, Vec<u8>)>,
}

impl GltfBuilder {
    /// Indices of buffers 1.. that hold bytes but have no `uri`: the exporter must place them.
    fn unresolved_extras(&self) -> Vec<usize> {
        let bufs = self.gltf.buffers.as_deref().unwrap_or(&[]);
        (1..bufs.len())
            .filter(|&i| bufs[i].uri.is_none() && self.buffer_data.get(i).is_some_and(|d| !d.is_empty()))
            .collect()
    }

    /// `BUFFER_URI_ON_STORED_BUFFER` if a buffer 1.. has both stored bytes and a hand-set `uri`:
    /// the exporter would drop the bytes silently.
    fn ensure_no_shadowed_bytes(&self) -> Result<()> {
        let bufs = self.gltf.buffers.as_deref().unwrap_or(&[]);
        match (1..bufs.len()).find(|&i| bufs[i].uri.is_some() && self.buffer_data.get(i).is_some_and(|d| !d.is_empty())) {
            Some(i) => Err(GltfError::InvalidData(format!(
                "BUFFER_URI_ON_STORED_BUFFER: buffer {i} holds bytes and also has a uri; the exporter would not write those bytes. \
                 Clear the uri to let ExtraBuffers place it"
            ))),
            None => Ok(()),
        }
    }

    /// `BUFFER_GLB_EXTRA_UNRESOLVED` if a GLB cannot hold every buffer.
    pub(crate) fn ensure_no_unresolved_extras(&self) -> Result<()> {
        self.ensure_no_shadowed_bytes()?;
        let extras = self.unresolved_extras();
        if extras.is_empty() {
            return Ok(());
        }
        let list: Vec<_> = extras.iter().map(|i| format!("buffer {i}")).collect();
        Err(GltfError::InvalidData(format!(
            "BUFFER_GLB_EXTRA_UNRESOLVED: {} cannot be the GLB BIN chunk; use to_glb_parts with ExtraBuffers",
            list.join(", ")
        )))
    }

    /// Choose a `uri` (and file) for every unresolved extra buffer, rejecting unsafe or colliding
    /// file names. `first` is the external file name of buffer 0 and `reserved` another name that
    /// must stay free (the `.gltf` itself).
    fn resolve_extras(&self, extra: &ExtraBuffers, first: Option<&str>, reserved: Option<&str>) -> Result<Resolved> {
        let n = self.gltf.buffers.as_ref().map_or(0, Vec::len);
        let mut uris = vec![None; n];
        let mut extra_bins = Vec::new();
        let todo = self.unresolved_extras();
        let mut taken: Vec<(String, String)> = Vec::new(); // (lowercase uri, what it names)
        let mut claim = |uri: String, what: String| -> Result<()> {
            let key = uri.to_lowercase();
            match taken.iter().find(|(k, _)| *k == key) {
                Some((_, other)) => Err(GltfError::InvalidData(format!(
                    "BUFFER_FILE_NAME_COLLISION: {what} and {other} would both be written to {uri:?} (compared case-insensitively)"
                ))),
                None => {
                    taken.push((key, what));
                    Ok(())
                }
            }
        };
        if let Some(r) = reserved {
            claim(uri::percent_encode(r), "the .gltf file".to_string())?;
        }
        if let Some(f) = first {
            claim(uri::percent_encode(f), "buffer 0".to_string())?;
        }
        for (i, buf) in self.gltf.buffers.as_deref().unwrap_or(&[]).iter().enumerate() {
            match buf.uri.as_deref() {
                Some(u) if i > 0 && !uri::is_data_uri(u) => claim(u.to_string(), format!("the uri of buffer {i}"))?,
                _ => {}
            }
        }
        for &i in &todo {
            let data = &self.buffer_data[i];
            match extra {
                ExtraBuffers::DataUri => uris[i] = Some(uri::encode_data_uri("application/octet-stream", data)),
                ExtraBuffers::External { prefix } => {
                    let name = format!("{prefix}{i}.bin");
                    uri::check_file_name(&name).map_err(|e| GltfError::InvalidData(format!("BUFFER_URI_INVALID: buffer {i}: {e}")))?;
                    let encoded = uri::percent_encode(&name);
                    claim(encoded.clone(), format!("buffer {i}"))?;
                    uris[i] = Some(encoded);
                    extra_bins.push((name, data.clone()));
                }
            }
        }
        Ok(Resolved { uris, extra_bins })
    }

    /// Validate and split into parts; `extra` is `None` for the single-buffer entry point.
    fn export_parts(
        &self, encoding: &BufferEncoding, extra: Option<&ExtraBuffers>, reserved: Option<&str>,
    ) -> Result<GltfParts> {
        if let BufferEncoding::External { bin_file_name } = encoding {
            uri::check_file_name(bin_file_name)
                .map_err(|e| GltfError::InvalidData(format!("BUFFER_URI_INVALID: {e}")))?;
        }
        self.ensure_valid()?;
        self.ensure_no_shadowed_bytes()?;
        let data = self.buffer_data.first().map_or(&[][..], Vec::as_slice);
        let (buffer_uri, bin) = match encoding {
            _ if data.is_empty() => (None, Vec::new()),
            BufferEncoding::Glb => (None, data.to_vec()),
            BufferEncoding::DataUri => (Some(uri::encode_data_uri("application/octet-stream", data)), Vec::new()),
            BufferEncoding::External { bin_file_name } => (Some(uri::percent_encode(bin_file_name)), data.to_vec()),
        };
        let first = match encoding {
            BufferEncoding::External { bin_file_name } if !data.is_empty() => Some(bin_file_name.as_str()),
            _ => None,
        };
        let Resolved { mut uris, extra_bins } = match extra {
            Some(e) => self.resolve_extras(e, first, reserved)?,
            None => {
                if let Some(i) = self.unresolved_extras().first() {
                    return Err(GltfError::InvalidData(format!(
                        "BUFFER_URI_MISSING: buffer {i} has no uri; use to_gltf_parts_with to choose where extra buffers go"
                    )));
                }
                Resolved { uris: vec![None], extra_bins: Vec::new() }
            }
        };
        if let Some(slot) = uris.first_mut() {
            *slot = buffer_uri;
        }
        Ok(GltfParts { json: self.serialize_json_with_uris(&uris)?, bin, extra_bins })
    }

    /// Validate the document and split it into JSON and binary data for `encoding`.
    ///
    /// Pure: nothing is written and the builder is not modified. A document with buffers other than
    /// 0 is rejected here (`BUFFER_URI_MISSING`); use [`to_gltf_parts_with`](Self::to_gltf_parts_with).
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
        self.export_parts(encoding, None, None)
    }

    /// Like [`to_gltf_parts`](Self::to_gltf_parts) for documents with several buffers: buffer 0
    /// follows `encoding`, buffers 1.. follow `extra`. A single-buffer document ignores `extra`.
    ///
    /// # Errors
    /// As `to_gltf_parts`, plus `BUFFER_FILE_NAME_COLLISION` if two buffers (or the `.bin` of
    /// buffer 0 and a hand-set `uri`) would use the same file name, compared case-insensitively,
    /// and `BUFFER_URI_INVALID` for an unsafe `prefix`.
    ///
    /// ```
    /// use mesh_tools::{BufferEncoding, ExtraBuffers, GltfBuilder};
    /// let mut b = GltfBuilder::new();
    /// b.create_box(1.0);
    /// b.add_buffer(None).unwrap(); // buffer 1
    /// b.add_data_view_in_buffer(1, &[1, 2, 3, 4], None, None).unwrap();
    /// let enc = BufferEncoding::External { bin_file_name: "m.bin".into() };
    /// let parts = b.to_gltf_parts_with(&enc, &ExtraBuffers::External { prefix: "m_".into() }).unwrap();
    /// assert!(parts.json.contains("\"uri\":\"m_1.bin\""));
    /// assert_eq!(parts.extra_bins, vec![("m_1.bin".to_string(), vec![1, 2, 3, 4])]);
    /// ```
    pub fn to_gltf_parts_with(&self, encoding: &BufferEncoding, extra: &ExtraBuffers) -> Result<GltfParts> {
        self.export_parts(encoding, Some(extra), None)
    }

    /// Validate and build a GLB (buffer 0 is the BIN chunk) whose buffers 1.. follow `extra`.
    ///
    /// # Errors
    /// As [`to_gltf_parts_with`](Self::to_gltf_parts_with).
    ///
    /// ```
    /// use mesh_tools::{ExtraBuffers, GltfBuilder};
    /// let mut b = GltfBuilder::new();
    /// b.create_box(1.0);
    /// b.add_buffer(None).unwrap();
    /// b.add_data_view_in_buffer(1, &[1, 2, 3, 4], None, None).unwrap();
    /// assert!(b.to_glb_bytes().unwrap_err().to_string().contains("BUFFER_GLB_EXTRA_UNRESOLVED"));
    /// let parts = b.to_glb_parts(&ExtraBuffers::External { prefix: "m_".into() }).unwrap();
    /// assert_eq!(&parts.glb[..4], b"glTF");
    /// assert_eq!(parts.extra_bins[0].0, "m_1.bin");
    /// ```
    pub fn to_glb_parts(&self, extra: &ExtraBuffers) -> Result<GlbParts> {
        let parts = self.export_parts(&BufferEncoding::Glb, Some(extra), None)?;
        let glb = crate::builder::glb_container(&parts.json, &parts.bin)?;
        Ok(GlbParts { glb, extra_bins: parts.extra_bins })
    }

    /// Validate and serialize to a single self-contained `.gltf` string (every buffer as a base64 data URI).
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
        Ok(self.to_gltf_parts_with(&BufferEncoding::DataUri, &ExtraBuffers::DataUri)?.json)
    }

    /// Validate and write `path` (`.gltf`) plus `<stem>.bin` next to it.
    ///
    /// Each file is written to a `.tmp` file and renamed into place, so a failure before the renames
    /// (invalid document, missing directory) leaves no files. The `.bin` is renamed first so the
    /// `.gltf` never points at a missing file. A document with no binary data writes only the `.gltf`.
    /// Buffers 1.. are written as `<stem>_<index>.bin`, all before the `.gltf`. If one rename fails,
    /// the `.gltf` is not written and the temp files are removed; `.bin` files renamed before the
    /// failure stay.
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
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or(stem);
        let enc = BufferEncoding::External { bin_file_name: bin_name };
        let parts = self.export_parts(&enc, Some(&ExtraBuffers::External { prefix: format!("{stem}_") }), Some(file_name))?;
        let mut files: Vec<(PathBuf, &[u8])> = Vec::new();
        if !parts.bin.is_empty() {
            files.push((bin_path, &parts.bin));
        }
        files.extend(parts.extra_bins.iter().map(|(n, d)| (path.with_file_name(n), d.as_slice())));
        files.push((path.to_path_buf(), parts.json.as_bytes()));
        write_atomic(&files)
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
        write_atomic(&[(path.as_ref().to_path_buf(), json.as_bytes())])
    }
}

/// Write every file to `<name>.tmp`, then rename them in order. Temp files are removed on failure.
fn write_atomic(files: &[(PathBuf, &[u8])]) -> Result<()> {
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
