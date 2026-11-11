# Buffers

The builder keeps one binary buffer (buffer 0) in memory. The **encoding** is chosen when you
export, never stored in the builder, so the same document can be written three ways.

| Encoding | Call | Result |
|---|---|---|
| GLB BIN chunk | `export_glb`, `to_glb_bytes` | one `.glb` file |
| base64 data URI | `export_gltf_embedded`, `to_gltf_string_embedded` | one self-contained `.gltf` |
| external file | `export_gltf` | `.gltf` plus `<stem>.bin` |

```rust
use mesh_tools::{BufferEncoding, GltfBuilder};

let mut b = GltfBuilder::new();
b.create_box(1.0);

// Embedded: one JSON string, buffer as a data URI.
let json = b.to_gltf_string_embedded().unwrap();
assert!(json.contains("\"uri\":\"data:application/octet-stream;base64,"));

// Parts: JSON and bytes separately, for callers that do their own I/O.
let parts = b.to_gltf_parts(&BufferEncoding::External { bin_file_name: "my model.bin".into() }).unwrap();
assert!(parts.json.contains("\"uri\":\"my%20model.bin\"")); // percent-encoded
assert!(!parts.bin.is_empty());

// The builder document is never modified: buffer 0 still has no uri.
assert!(b.inspect_gltf().buffers.as_ref().unwrap()[0].uri.is_none());

// File names must be a bare name.
assert!(b.to_gltf_parts(&BufferEncoding::External { bin_file_name: "../x.bin".into() }).is_err());
```

## Writing files

`export_gltf(path)` writes `path` and `<stem>.bin` next to it. Files are written to `.tmp` names
and renamed, so an invalid document or a missing directory leaves nothing behind. The `.bin` is
renamed first. If the final `.gltf` rename itself fails, the already-renamed `.bin` stays. A document with no binary data writes only the `.gltf`.

## Images as data URIs

`add_image_data_uri` puts a PNG or JPEG in the image's `uri` as `data:image/png;base64,...`
instead of a buffer view, so it adds nothing to the binary buffer.

```rust
use mesh_tools::{texture::{create_uv_test_pattern, image_to_bytes, TextureFormat}, GltfBuilder};

let mut b = GltfBuilder::new();
let png = image_to_bytes(&create_uv_test_pattern(8, 8).unwrap(), TextureFormat::PNG).unwrap();
b.add_image_data_uri(Some("uv".into()), TextureFormat::PNG, &png).unwrap();
assert!(b.add_image_data_uri(None, TextureFormat::JPEG, &png).is_err()); // format mismatch
```

## Validator codes

| Code | Severity | Meaning |
|---|---|---|
| `BUFFER_URI_ON_GLB_BUFFER` | error | buffer 0 has a `uri`; the encoding is chosen at export |
| `BUFFER_URI_MISSING` | error | buffer 1 or later has no `uri` (only buffer 0 can be the BIN chunk) |
| `BUFFER_URI_INVALID` | error | empty, absolute, backslash, scheme, `..`, or a bad external file name |
| `BUFFER_DATA_URI_INVALID` | error | not base64, wrong media type, or shorter than `byteLength` |
| `BUFFER_UNUSED` | warning | no buffer view uses the buffer |
| `IMAGE_URI_INVALID` | error | external image uri is absolute, has a scheme or backslash, or escapes with `..` |
| `IMAGE_DATA_URI_INVALID` | error | image data URI malformed or disagreeing with `mimeType` |

## Limits

Only buffer 0 is written by the builder (single buffer). Buffers added by hand through the public
`gltf` field must carry a valid relative or `data:` URI.
