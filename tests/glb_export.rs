mod common;

use mesh_tools::GltfBuilder;

#[test]
fn simple_box_round_trips_through_reference_loader() {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let node = b.add_node(Some("Box".into()), Some(mesh), None, None, None);
    b.add_scene(Some("Scene".into()), Some(vec![node]));

    let glb = common::parse_glb(&b);
    assert_eq!(glb.document.meshes().count(), 1);
    assert_eq!(glb.document.nodes().count(), 1);
    let blob = glb.blob.as_ref().expect("BIN chunk present");
    assert!(!blob.is_empty());
}

#[test]
fn glb_header_and_chunks_are_aligned_and_length_is_consistent() {
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    let bytes = b.to_glb_bytes().unwrap();

    assert_eq!(&bytes[0..4], b"glTF");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize, bytes.len());
    assert_eq!(bytes.len() % 4, 0);
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    assert_eq!(json_len % 4, 0);
}

#[test]
fn truncated_glb_is_rejected_by_reference_loader() {
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    let bytes = b.to_glb_bytes().unwrap();
    assert!(gltf::Gltf::from_slice(&bytes[..bytes.len() - 8]).is_err());
}

#[test]
fn export_glb_writes_same_bytes_as_to_glb_bytes() {
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    let path = std::env::temp_dir().join("mesh_tools_export_test.glb");
    b.export_glb(path.to_str().unwrap()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b.to_glb_bytes().unwrap());
    let _ = std::fs::remove_file(path);
}
