//! N3b: exporting documents with several buffers (GLB BIN + external files or data URIs).
mod common;

use mesh_tools::compat::point3;
use mesh_tools::{BufferEncoding, ExtraBuffers, GltfBuilder, GltfError, Triangle};
use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mesh_tools_mbx_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Buffer 0: a unit box (8 vertices). Buffer 1: one triangle (3 positions + 3 u16 indices).
/// Buffer 2: a second triangle. Returns the builder; positions are hand-chosen for read-back.
fn three_buffers() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let box_mesh = b.create_box(1.0);
    let mut nodes = vec![b.add_node(None, Some(box_mesh), None, None, None)];
    for (buf, x) in [(1usize, 10.0f32), (2, 20.0)] {
        b.add_buffer(Some(format!("extra{buf}"))).unwrap();
        b.set_write_buffer(buf).unwrap();
        let pos = [point3::new(x, 0.0, 0.0), point3::new(x + 1.0, 0.0, 0.0), point3::new(x, 1.0, 0.0)];
        let m = b.create_simple_mesh(None, &pos, &[Triangle::new(0, 1, 2)], None, None, None);
        nodes.push(b.add_node(None, Some(m), None, None, None));
    }
    b.set_write_buffer(0).unwrap();
    b.add_scene(None, Some(nodes));
    b
}

fn positions(doc: &gltf::Document, bufs: &[gltf::buffer::Data], mesh: usize) -> Vec<[f32; 3]> {
    let prim = doc.meshes().nth(mesh).unwrap().primitives().next().unwrap();
    prim.reader(|b| Some(&bufs[b.index()].0[..])).read_positions().unwrap().collect()
}

fn check_import(doc: &gltf::Document, bufs: &[gltf::buffer::Data]) {
    assert_eq!(bufs.len(), 3);
    assert_eq!(doc.buffers().nth(1).unwrap().name(), Some("extra1"));
    assert_eq!(positions(doc, bufs, 1), [[10.0, 0.0, 0.0], [11.0, 0.0, 0.0], [10.0, 1.0, 0.0]]);
    assert_eq!(positions(doc, bufs, 2), [[20.0, 0.0, 0.0], [21.0, 0.0, 0.0], [20.0, 1.0, 0.0]]);
    assert_eq!(positions(doc, bufs, 0).len(), 8);
    // Each mesh's views live in its own buffer.
    let view_buffer = |m: usize| {
        let p = doc.meshes().nth(m).unwrap().primitives().next().unwrap();
        p.get(&gltf::Semantic::Positions).unwrap().view().unwrap().buffer().index()
    };
    assert_eq!((view_buffer(0), view_buffer(1), view_buffer(2)), (0, 1, 2));
}

#[test]
fn glb_with_external_extra_buffers_round_trips_through_gltf_import() {
    let b = three_buffers();
    let parts = b.to_glb_parts(&ExtraBuffers::External { prefix: "m_".into() }).unwrap();
    let names: Vec<_> = parts.extra_bins.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["m_1.bin", "m_2.bin"]);
    assert_eq!(parts.extra_bins[0].1, b.buffer_bytes(1).unwrap());
    let dir = tmp_dir("glb_ext");
    std::fs::write(dir.join("m.glb"), &parts.glb).unwrap();
    for (n, d) in &parts.extra_bins {
        std::fs::write(dir.join(n), d).unwrap();
    }
    let (doc, bufs, _) = gltf::import(dir.join("m.glb")).unwrap();
    check_import(&doc, &bufs);
    // Buffer 0 is the BIN chunk, extras carry uris.
    assert!(matches!(doc.buffers().next().unwrap().source(), gltf::buffer::Source::Bin));
}

#[test]
fn glb_with_data_uri_extra_buffers_is_self_contained() {
    let b = three_buffers();
    let parts = b.to_glb_parts(&ExtraBuffers::DataUri).unwrap();
    assert!(parts.extra_bins.is_empty());
    let (doc, bufs, _) = gltf::import_slice(&parts.glb).unwrap();
    check_import(&doc, &bufs);
}

#[test]
fn gltf_parts_external_and_data_uri() {
    let b = three_buffers();
    let enc = BufferEncoding::External { bin_file_name: "m.bin".into() };
    let parts = b.to_gltf_parts_with(&enc, &ExtraBuffers::External { prefix: "m_".into() }).unwrap();
    assert!(parts.json.contains("\"uri\":\"m.bin\"") && parts.json.contains("\"uri\":\"m_1.bin\"") && parts.json.contains("\"uri\":\"m_2.bin\""));
    assert_eq!(parts.bin, b.buffer_bytes(0).unwrap());
    assert_eq!(parts.extra_bins.len(), 2);
    let dir = tmp_dir("parts_ext");
    std::fs::write(dir.join("m.gltf"), &parts.json).unwrap();
    std::fs::write(dir.join("m.bin"), &parts.bin).unwrap();
    for (n, d) in &parts.extra_bins {
        std::fs::write(dir.join(n), d).unwrap();
    }
    let (doc, bufs, _) = gltf::import(dir.join("m.gltf")).unwrap();
    check_import(&doc, &bufs);

    let emb = b.to_gltf_parts_with(&BufferEncoding::DataUri, &ExtraBuffers::DataUri).unwrap();
    assert!(emb.bin.is_empty() && emb.extra_bins.is_empty());
    assert_eq!(emb.json.matches("data:application/octet-stream;base64,").count(), 3);
    let (doc, bufs, _) = gltf::import_slice(emb.json.as_bytes()).unwrap();
    check_import(&doc, &bufs);
    // The convenience wrapper embeds every buffer.
    assert_eq!(b.to_gltf_string_embedded().unwrap(), emb.json);
}

#[test]
fn export_gltf_writes_every_bin_and_the_gltf() {
    let b = three_buffers();
    let dir = tmp_dir("export");
    b.export_gltf(dir.join("m.gltf")).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    files.sort();
    assert_eq!(files, ["m.bin", "m.gltf", "m_1.bin", "m_2.bin"]);
    assert_eq!(std::fs::read(dir.join("m_2.bin")).unwrap(), b.buffer_bytes(2).unwrap());
    let (doc, bufs, _) = gltf::import(dir.join("m.gltf")).unwrap();
    check_import(&doc, &bufs);
    // Embedded export of the same document.
    b.export_gltf_embedded(dir.join("e.gltf")).unwrap();
    let (doc, bufs, _) = gltf::import(dir.join("e.gltf")).unwrap();
    check_import(&doc, &bufs);
}

#[test]
fn glb_without_a_resolution_for_extra_buffers_is_rejected() {
    let b = three_buffers();
    for r in [b.to_glb_bytes().map(drop), b.export_glb("/nonexistent-dir/x.glb")] {
        match r {
            Err(GltfError::InvalidData(m)) => assert!(m.contains("BUFFER_GLB_EXTRA_UNRESOLVED") && m.contains("buffer 1"), "{m}"),
            other => panic!("{other:?}"),
        }
    }
    // Plain to_gltf_parts cannot place the extras either.
    let e = b.to_gltf_parts(&BufferEncoding::DataUri).unwrap_err().to_string();
    assert!(e.contains("BUFFER_URI_MISSING") && e.contains("to_gltf_parts_with"), "{e}");
}

#[test]
fn file_name_collisions_are_rejected() {
    let b = three_buffers();
    let enc = BufferEncoding::External { bin_file_name: "m_1.bin".into() };
    let e = b.to_gltf_parts_with(&enc, &ExtraBuffers::External { prefix: "m_".into() }).unwrap_err().to_string();
    assert!(e.contains("BUFFER_FILE_NAME_COLLISION") && e.contains("m_1.bin"), "{e}");
    // Case-insensitive file systems.
    let enc = BufferEncoding::External { bin_file_name: "M_1.BIN".into() };
    assert!(b.to_gltf_parts_with(&enc, &ExtraBuffers::External { prefix: "m_".into() }).unwrap_err().to_string().contains("BUFFER_FILE_NAME_COLLISION"));
    // Collision with a hand-added uri on another buffer.
    let mut c = GltfBuilder::new();
    c.add_data_view(&[1; 4], None, None).unwrap();
    c.add_buffer(None).unwrap();
    c.add_data_view_in_buffer(1, &[1; 4], None, None).unwrap();
    let mut hand = mesh_tools::Buffer::default();
    hand.uri = Some("x_1.bin".into());
    hand.byte_length = 4;
    c.gltf.buffers.as_mut().unwrap().push(hand);
    let e = c.to_glb_parts(&ExtraBuffers::External { prefix: "x_".into() }).unwrap_err().to_string();
    assert!(e.contains("BUFFER_FILE_NAME_COLLISION"), "{e}");
    // A prefix that makes an unsafe name.
    let e = b.to_glb_parts(&ExtraBuffers::External { prefix: "../".into() }).unwrap_err().to_string();
    assert!(e.contains("BUFFER_URI_INVALID"), "{e}");
    // export_gltf: the .gltf itself must not be one of the generated names.
    let dir = tmp_dir("collide");
    let r = b.export_gltf(dir.join("m_1.bin"));
    assert!(r.is_err());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
}

#[test]
fn rename_failure_of_an_extra_bin_never_writes_the_gltf_or_leaves_tmp_files() {
    let b = three_buffers();
    let dir = tmp_dir("rename_fail");
    // A non-empty directory where m_2.bin should go makes that rename fail.
    std::fs::create_dir_all(dir.join("m_2.bin")).unwrap();
    std::fs::write(dir.join("m_2.bin").join("keep"), b"x").unwrap();
    let e = b.export_gltf(dir.join("m.gltf")).unwrap_err();
    assert!(matches!(e, GltfError::Io(_)), "{e:?}");
    assert!(!dir.join("m.gltf").exists(), "the .gltf must be written last");
    let mut left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    left.sort();
    assert!(!left.iter().any(|n| n.ends_with(".tmp")), "{left:?}");
}

#[test]
fn validation_failure_writes_nothing_and_does_not_modify_the_builder() {
    let mut b = three_buffers();
    let before = serde_json::to_string(&b.gltf).unwrap();
    let dir = tmp_dir("invalid");
    b.gltf.buffers.as_mut().unwrap()[2].byte_length += 4; // length mismatch
    let broken = serde_json::to_string(&b.gltf).unwrap();
    assert!(b.export_gltf(dir.join("m.gltf")).is_err());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    assert_eq!(serde_json::to_string(&b.gltf).unwrap(), broken);
    assert_ne!(before, broken);
}

#[test]
fn builder_document_never_gains_uris() {
    let b = three_buffers();
    b.to_glb_parts(&ExtraBuffers::DataUri).unwrap();
    b.to_gltf_parts_with(&BufferEncoding::DataUri, &ExtraBuffers::DataUri).unwrap();
    assert!(b.gltf.buffers.as_ref().unwrap().iter().all(|x| x.uri.is_none()));
}

#[test]
fn validate_accepts_extra_buffers_that_hold_bytes() {
    let b = three_buffers();
    let r = b.validate();
    assert!(r.errors.is_empty() && r.warnings.is_empty(), "{r:#?}");
}

#[test]
fn single_buffer_documents_ignore_the_extra_policy() {
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    let plain = b.to_gltf_parts(&BufferEncoding::External { bin_file_name: "a.bin".into() }).unwrap();
    let with = b.to_gltf_parts_with(&BufferEncoding::External { bin_file_name: "a.bin".into() }, &ExtraBuffers::DataUri).unwrap();
    assert_eq!(plain, with);
    assert!(with.extra_bins.is_empty());
    let g = b.to_glb_parts(&ExtraBuffers::DataUri).unwrap();
    assert_eq!(g.glb, b.to_glb_bytes().unwrap());
}

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, x| (h ^ *x as u64).wrapping_mul(0x100000001b3))
}

/// Hashes recorded from the previous commit (0.21.0): single-buffer output must not change.
#[test]
fn single_buffer_output_is_byte_identical_to_the_previous_commit() {
    let mut b = GltfBuilder::new();
    let m = b.create_box(1.0);
    let n = b.add_node(None, Some(m), None, None, None);
    b.add_scene(None, Some(vec![n]));
    let p = b.to_gltf_parts(&BufferEncoding::External { bin_file_name: "a b.bin".into() }).unwrap();
    assert_eq!((fnv(p.json.as_bytes()), fnv(&p.bin)), (0x9dd8d66b6c1419f6, 0xb69673e8948524d5));
    let dir = tmp_dir("golden");
    b.export_gltf(dir.join("m.gltf")).unwrap();
    assert_eq!(fnv(&std::fs::read(dir.join("m.gltf")).unwrap()), 0xf4c4d59b26d9b4b3);
    assert_eq!(fnv(&std::fs::read(dir.join("m.bin")).unwrap()), 0xb69673e8948524d5);
    assert_eq!(fnv(&b.to_glb_bytes().unwrap()), 0x4e9bf7fb746e6bf9);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
}

#[test]
fn stored_bytes_with_a_hand_set_uri_are_an_error_not_silently_dropped() {
    let mut c = GltfBuilder::new();
    c.add_data_view(&[1; 4], None, None).unwrap();
    c.add_buffer(None).unwrap();
    c.add_data_view_in_buffer(1, &[1; 4], None, None).unwrap();
    c.gltf.buffers.as_mut().unwrap()[1].uri = Some("foo.bin".into());
    let ext = ExtraBuffers::External { prefix: "x_".into() };
    for e in [
        c.to_glb_parts(&ext).unwrap_err().to_string(),
        c.to_glb_bytes().unwrap_err().to_string(),
        c.to_gltf_parts(&BufferEncoding::Glb).unwrap_err().to_string(),
        c.to_gltf_string_embedded().unwrap_err().to_string(),
    ] {
        assert!(e.contains("BUFFER_URI_ON_STORED_BUFFER"), "{e}");
    }
}
