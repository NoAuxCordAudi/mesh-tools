# Buffers

The builder keeps its binary data in memory in one or more buffers. The **encoding** is chosen when
you export, never stored in the builder, so the same document can be written several ways. Buffer 0
is created by the first data write; the sections below start with that single buffer and then add more.

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
| `BUFFER_URI_MISSING` | error | buffer 1 or later has neither a `uri` nor stored bytes (only buffer 0 can be the BIN chunk) |
| `BUFFER_URI_INVALID` | error | empty, absolute, backslash, scheme, `..`, or a bad external file name |
| `BUFFER_DATA_URI_INVALID` | error | not base64, wrong media type, or shorter than `byteLength` |
| `BUFFER_UNUSED` | warning | no buffer view uses the buffer |
| `BUFFER_DATA_UNVERIFIED` | warning | a buffer without stored bytes is used by views; they are only checked against `byteLength` |
| `BUFFER_LENGTH_MISMATCH` | error | a buffer's `byteLength` differs from its stored bytes (per buffer) |
| `BUFFER_VIEW_RANGE` | error | a view does not fit in its own buffer |
| `BUFFER_VIEW_BUFFER_INVALID` | error | a view's `buffer` is not an existing buffer |
| `IMAGE_URI_INVALID` | error | external image uri is absolute, has a scheme or backslash, or escapes with `..` |
| `IMAGE_DATA_URI_INVALID` | error | image data URI malformed or disagreeing with `mimeType` |

## Several buffers in memory

`add_buffer` appends an empty buffer; every writer appends to the current write buffer
(`set_write_buffer`, initially 0) and `add_data_view_in_buffer` targets one buffer without changing
it. Offsets are 4-byte aligned per buffer.

```
use mesh_tools::GltfBuilder;
let mut b = GltfBuilder::new();
b.add_buffer(Some("geometry".into())).unwrap();
b.add_buffer(Some("extra".into())).unwrap();
let v = b.add_data_view_in_buffer(1, &[1, 2, 3, 4, 5], None, None).unwrap();
let w = b.add_data_view_in_buffer(1, &[9], None, None).unwrap();
let views = b.gltf.buffer_views.as_ref().unwrap();
assert_eq!((views[v].buffer, views[v].byte_offset), (1, 0));
assert_eq!((views[w].buffer, views[w].byte_offset), (1, 8)); // 5 bytes padded to 8
```

## Exporting several buffers

Buffer 0 follows the `BufferEncoding` as above. Buffers 1.. follow an `ExtraBuffers` policy:

| Policy | Result |
|---|---|
| `ExtraBuffers::External { prefix }` | files `{prefix}{index}.bin`, returned in `extra_bins` |
| `ExtraBuffers::DataUri` | base64 data URIs inside the JSON, `extra_bins` empty |

```
use mesh_tools::{BufferEncoding, ExtraBuffers, GltfBuilder};
let mut b = GltfBuilder::new();
b.create_box(1.0); // buffer 0
b.add_buffer(Some("extra".into())).unwrap();
b.add_data_view_in_buffer(1, &[1, 2, 3, 4], None, None).unwrap();
assert!(b.validate().is_valid()); // the exporter supplies buffer 1's uri

let enc = BufferEncoding::External { bin_file_name: "m.bin".into() };
let parts = b.to_gltf_parts_with(&enc, &ExtraBuffers::External { prefix: "m_".into() }).unwrap();
assert_eq!(parts.extra_bins[0].0, "m_1.bin");

// A GLB keeps buffer 0 as its BIN chunk; extra buffers need a policy.
assert!(b.to_glb_bytes().unwrap_err().to_string().contains("BUFFER_GLB_EXTRA_UNRESOLVED"));
let glb = b.to_glb_parts(&ExtraBuffers::DataUri).unwrap();
assert!(glb.extra_bins.is_empty());

// Colliding names are rejected (compared case-insensitively).
let clash = BufferEncoding::External { bin_file_name: "M_1.bin".into() };
let err = b.to_gltf_parts_with(&clash, &ExtraBuffers::External { prefix: "m_".into() }).unwrap_err();
assert!(err.to_string().contains("BUFFER_FILE_NAME_COLLISION"));
```

- `to_gltf_parts` (no policy) rejects a document with extra buffers (`BUFFER_URI_MISSING`).
- `to_gltf_string_embedded` / `export_gltf_embedded` embed every buffer.
- `export_gltf("dir/m.gltf")` writes `m.bin`, `m_1.bin`, `m_2.bin`, ... and then `m.gltf`. All files
  go through `.tmp` names; if any rename fails the `.gltf` is not written and the temp files are
  removed. `.bin` files renamed before the failure stay on disk.
- A single-buffer document ignores the policy, and its output is unchanged.
- Buffers that were hand-added with a `uri` and no stored bytes are passed through unchanged.

| Export error | When |
|---|---|
| `BUFFER_GLB_EXTRA_UNRESOLVED` | `to_glb_bytes` / `export_glb` with buffers other than 0 holding bytes |
| `BUFFER_FILE_NAME_COLLISION` | two buffers, a hand-set `uri` or the `.gltf` would share a file name |
| `BUFFER_URI_ON_STORED_BUFFER` | buffer 1.. holds stored bytes and has a hand-set `uri` (the bytes would not be written) |
| `BUFFER_URI_INVALID` | a `prefix` or file name that is empty, a path or unsafe |

## Limits

Buffers added by hand through the public `gltf` field must carry a valid relative or `data:` URI,
and views into them are only checked against `byteLength` (`BUFFER_DATA_UNVERIFIED`). The builder
cannot place their bytes because it does not hold them. Do not mix hand-added buffers with
`add_buffer`: it refuses when `gltf.buffers` and the stored bytes disagree.
