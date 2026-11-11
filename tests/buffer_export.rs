//! Buffer encodings (GLB BIN, base64 data URI, external .bin) read back through the
//! independent `gltf` crate against the hand-derived unit cube.

use mesh_tools::{texture::{create_uv_test_pattern, TextureFormat}, BufferEncoding, GltfBuilder};
use std::path::PathBuf;

/// `create_box(1.0)` corners in vertex order (see the comments in `builder_primitives.rs`).
const CUBE_POSITIONS: [[f32; 3]; 8] = [
    [-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5],
    [-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5],
];
/// Two triangles per face: front, back, top, bottom, right, left.
const CUBE_INDICES: [u32; 36] = [
    0, 1, 2, 0, 2, 3, 5, 4, 7, 5, 7, 6, 3, 2, 6, 3, 6, 7, 4, 5, 1, 4, 1, 0, 1, 5, 6, 1, 6, 2, 4, 0, 3, 4, 3, 7,
];

fn cube() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));
    b
}

fn assert_cube(doc: &gltf::Document, buffers: &[gltf::buffer::Data]) {
    let mesh = doc.meshes().next().unwrap();
    let prim = mesh.primitives().next().unwrap();
    let reader = prim.reader(|b| Some(&buffers[b.index()]));
    let pos: Vec<[f32; 3]> = reader.read_positions().unwrap().collect();
    assert_eq!(pos, CUBE_POSITIONS);
    let idx: Vec<u32> = reader.read_indices().unwrap().into_u32().collect();
    assert_eq!(idx, CUBE_INDICES);
}

/// Unique scratch directory (no tempdir dependency).
fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mesh_tools_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn entries(d: &PathBuf) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into()).collect();
    v.sort();
    v
}

/// Independent base64 decoder (not the crate the library uses).
fn b64(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A', b'a'..=b'z' => c - b'a' + 26, b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62, b'/' => 63, _ => panic!("bad base64 char {c}"),
    } as u32;
    let s = s.trim_end_matches('=').as_bytes();
    let mut out = Vec::new();
    for ch in s.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in ch.iter().enumerate() { n |= val(c) << (18 - 6 * i); }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..ch.len() * 6 / 8]);
    }
    out
}

fn json(s: &str) -> serde_json::Value { serde_json::from_str(s).unwrap() }

#[test]
fn glb_encoding_parts_and_import() {
    let b = cube();
    let parts = b.to_gltf_parts(&BufferEncoding::Glb).unwrap();
    assert!(json(&parts.json)["buffers"][0].get("uri").is_none());
    assert_eq!(json(&parts.json)["buffers"][0]["byteLength"].as_u64().unwrap() as usize, parts.bin.len());
    // 8*12 positions + 36*2 indices (u16) padded to 4 bytes
    assert!(parts.bin.len() >= 96 + 72);
    let (doc, buffers, _) = gltf::import_slice(b.to_glb_bytes().unwrap()).unwrap();
    assert_cube(&doc, &buffers);
}

#[test]
fn data_uri_encoding_imports_and_decodes_independently() {
    let b = cube();
    let s = b.to_gltf_string_embedded().unwrap();
    let v = json(&s);
    let uri = v["buffers"][0]["uri"].as_str().unwrap();
    let payload = uri.strip_prefix("data:application/octet-stream;base64,").expect("data uri prefix");
    let glb = b.to_gltf_parts(&BufferEncoding::Glb).unwrap();
    assert_eq!(b64(payload), glb.bin);
    let parts = b.to_gltf_parts(&BufferEncoding::DataUri).unwrap();
    assert!(parts.bin.is_empty());
    assert_eq!(parts.json, s);
    let (doc, buffers, _) = gltf::import_slice(s.as_bytes()).unwrap();
    assert_cube(&doc, &buffers);
}

#[test]
fn external_encoding_with_space_in_stem() {
    let b = cube();
    let d = scratch("ext");
    let path = d.join("my model.gltf");
    b.export_gltf(&path).unwrap();
    assert_eq!(entries(&d), ["my model.bin", "my model.gltf"]);
    let v = json(&std::fs::read_to_string(&path).unwrap());
    assert_eq!(v["buffers"][0]["uri"], "my%20model.bin");
    let (doc, buffers, _) = gltf::import(&path).unwrap();
    assert_cube(&doc, &buffers);
    assert_eq!(std::fs::read(d.join("my model.bin")).unwrap(), b.to_gltf_parts(&BufferEncoding::Glb).unwrap().bin);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn external_parts_use_given_name() {
    let b = cube();
    let p = b.to_gltf_parts(&BufferEncoding::External { bin_file_name: "é #.bin".into() }).unwrap();
    assert_eq!(json(&p.json)["buffers"][0]["uri"], "%C3%A9%20%23.bin");
    assert!(!p.bin.is_empty());
}

#[test]
fn embedded_export_is_a_single_file() {
    let b = cube();
    let d = scratch("emb");
    let path = d.join("cube.gltf");
    b.export_gltf_embedded(&path).unwrap();
    assert_eq!(entries(&d), ["cube.gltf"]);
    let (doc, buffers, _) = gltf::import(&path).unwrap();
    assert_cube(&doc, &buffers);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn export_leaves_builder_document_unchanged() {
    let b = cube();
    let before = b.to_glb_bytes().unwrap();
    b.to_gltf_string_embedded().unwrap();
    b.to_gltf_parts(&BufferEncoding::External { bin_file_name: "a.bin".into() }).unwrap();
    assert!(b.inspect_gltf().buffers.as_ref().unwrap()[0].uri.is_none());
    assert_eq!(b.to_glb_bytes().unwrap(), before);
}

#[test]
fn invalid_external_names_are_rejected() {
    let b = cube();
    for name in ["", "../x.bin", "..", ".", "a/b.bin", "a\\b.bin", "/abs.bin", "a\0.bin", "c:x.bin", "a?.bin"] {
        let e = b.to_gltf_parts(&BufferEncoding::External { bin_file_name: name.into() }).unwrap_err();
        assert!(e.to_string().contains("BUFFER_URI_INVALID"), "{name:?}: {e}");
    }
}

#[test]
fn failed_exports_leave_no_files() {
    let d = scratch("atomic");
    // invalid document
    let mut bad = cube();
    bad.gltf.nodes.as_mut().unwrap()[0].mesh = Some(99);
    assert!(bad.export_gltf(d.join("a.gltf")).is_err());
    assert!(bad.export_gltf_embedded(d.join("b.gltf")).is_err());
    assert!(entries(&d).is_empty());
    // missing directory
    let good = cube();
    assert!(good.export_gltf(d.join("missing/a.gltf")).is_err());
    assert!(good.export_gltf_embedded(d.join("missing/a.gltf")).is_err());
    assert!(entries(&d).is_empty());
    // the .gltf and its .bin would be the same file
    assert!(good.export_gltf(d.join("a.bin")).is_err());
    assert!(entries(&d).is_empty());
    // a path with no file name
    assert!(good.export_gltf("/").is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn empty_document_has_no_buffer_in_any_encoding() {
    let b = GltfBuilder::new();
    for enc in [BufferEncoding::Glb, BufferEncoding::DataUri, BufferEncoding::External { bin_file_name: "e.bin".into() }] {
        let p = b.to_gltf_parts(&enc).unwrap();
        assert!(json(&p.json).get("buffers").is_none());
        assert!(p.bin.is_empty());
    }
    let d = scratch("empty");
    b.export_gltf(d.join("e.gltf")).unwrap();
    assert_eq!(entries(&d), ["e.gltf"]);
    let _ = std::fs::remove_dir_all(&d);
}

// ---- validator codes ----

fn code(b: &GltfBuilder, c: &str) -> bool { b.validate().has_error(c) }

#[test]
fn glb_buffer_must_not_carry_a_uri() {
    let mut b = cube();
    b.gltf.buffers.as_mut().unwrap()[0].uri = Some("x.bin".into());
    assert!(code(&b, "BUFFER_URI_ON_GLB_BUFFER"));
    assert!(b.to_gltf_string_embedded().is_err());
}

#[test]
fn extra_buffer_needs_a_valid_uri() {
    let mut b = cube();
    let mut extra = mesh_tools::Buffer::default();
    extra.byte_length = 4;
    b.gltf.buffers.as_mut().unwrap().push(extra);
    assert!(code(&b, "BUFFER_URI_MISSING"));
    for bad in ["", "../x.bin", "/etc/x", "a\\b.bin", "%2e%2e/x.bin", "http://h/x.bin"] {
        b.gltf.buffers.as_mut().unwrap()[1].uri = Some(bad.into());
        assert!(code(&b, "BUFFER_URI_INVALID"), "{bad:?}");
    }
    b.gltf.buffers.as_mut().unwrap()[1].uri = Some("sub/x%20y.bin".into());
    assert!(b.validate().is_valid(), "{:?}", b.validate());
    // unused extra buffer is only a warning
    assert!(b.validate().has_warning("BUFFER_UNUSED"));
}

#[test]
fn buffer_data_uri_is_checked() {
    let mut b = cube();
    let mut extra = mesh_tools::Buffer::default();
    extra.byte_length = 4;
    b.gltf.buffers.as_mut().unwrap().push(extra);
    for bad in [
        "data:application/octet-stream;base64,@@@@",     // not base64
        "data:application/octet-stream,AAAA",            // not base64-encoded
        "data:text/plain;base64,AAAAAA==",               // wrong media type
        "data:application/octet-stream;base64,AAA=",     // 2 bytes < byteLength 4
    ] {
        b.gltf.buffers.as_mut().unwrap()[1].uri = Some(bad.into());
        assert!(code(&b, "BUFFER_DATA_URI_INVALID"), "{bad:?}");
    }
    b.gltf.buffers.as_mut().unwrap()[1].uri = Some("data:application/octet-stream;base64,AAAAAA==".into());
    assert!(b.validate().is_valid(), "{:?}", b.validate());
}

// ---- images as data URIs ----

fn png() -> Vec<u8> {
    mesh_tools::texture::image_to_bytes(&create_uv_test_pattern(8, 4).unwrap(), TextureFormat::PNG).unwrap()
}

#[test]
fn image_data_uri_loads_in_every_encoding() {
    let mut b = cube();
    let bytes = png();
    let img = b.add_image_data_uri(Some("uv".into()), TextureFormat::PNG, &bytes).unwrap();
    let uri = b.inspect_gltf().images.as_ref().unwrap()[img].uri.clone().unwrap();
    let payload = uri.strip_prefix("data:image/png;base64,").unwrap();
    assert_eq!(b64(payload), bytes);
    assert_eq!(b.inspect_gltf().images.as_ref().unwrap()[img].mime_type.as_deref(), Some("image/png"));
    // gltf::import_slice has no base directory and refuses every image uri, so import from files.
    let d = scratch("imguri");
    std::fs::write(d.join("a.glb"), b.to_glb_bytes().unwrap()).unwrap();
    b.export_gltf_embedded(d.join("b.gltf")).unwrap();
    for f in ["a.glb", "b.gltf"] {
        let (doc, buffers, images) = gltf::import(d.join(f)).unwrap();
        assert_cube(&doc, &buffers);
        assert_eq!((images[0].width, images[0].height), (8, 4), "{f}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bufferview_image_loads_from_data_uri_buffer() {
    let mut b = cube();
    b.add_image_bytes(None, TextureFormat::PNG, &png()).unwrap();
    let (_, _, images) = gltf::import_slice(b.to_gltf_string_embedded().unwrap().as_bytes()).unwrap();
    assert_eq!((images[0].width, images[0].height), (8, 4));
    let d = scratch("imgext");
    b.export_gltf(d.join("i.gltf")).unwrap();
    let (_, _, images) = gltf::import(d.join("i.gltf")).unwrap();
    assert_eq!((images[0].width, images[0].height), (8, 4));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn image_data_uri_errors_are_atomic() {
    let mut b = cube();
    assert!(b.add_image_data_uri(None, TextureFormat::PNG, b"nope").is_err());
    assert!(b.add_image_data_uri(None, TextureFormat::JPEG, &png()).is_err());
    assert!(b.inspect_gltf().images.as_ref().map_or(true, |v| v.is_empty()));
    // add_image_uri still refuses data: URIs and points at the right API
    let e = b.add_image_uri(None, "data:image/png;base64,AAAA", None).unwrap_err();
    assert!(e.to_string().contains("add_image_data_uri"));
}

#[test]
fn broken_image_data_uri_is_reported() {
    let mut b = cube();
    b.add_image_data_uri(None, TextureFormat::PNG, &png()).unwrap();
    let set = |b: &mut GltfBuilder, u: &str| b.gltf.images.as_mut().unwrap()[0].uri = Some(u.into());
    for bad in ["data:image/png;base64,@@@@", "data:image/png,AAAA", "data:image/jpeg;base64,AAAA"] {
        set(&mut b, bad);
        assert!(code(&b, "IMAGE_DATA_URI_INVALID"), "{bad:?}");
    }
    // valid base64 but not a PNG
    set(&mut b, "data:image/png;base64,AAAAAAAA");
    assert!(code(&b, "IMAGE_DATA_MISMATCH"));
}

#[test]
fn external_image_uris_must_be_relative() {
    let mut b = cube();
    for bad in ["/etc/a.png", "../a.png", "a\\b.png", "http://h/a.png", "file:///a.png", "%2e%2e/a.png", "a%2F..%5Cb.png", "a%00.png"] {
        assert!(b.add_image_uri(None, bad, None).is_err(), "{bad:?}");
    }
    assert!(b.inspect_gltf().images.as_ref().map_or(true, |v| v.is_empty()));
    b.add_image_uri(None, "tex/a%20b.png", None).unwrap();
    assert!(b.validate().is_valid());
    for bad in ["../a.png", "/a.png", "http://h/a.png"] {
        b.gltf.images.as_mut().unwrap()[0].uri = Some(bad.into());
        assert!(code(&b, "IMAGE_URI_INVALID"), "{bad:?}");
    }
}
