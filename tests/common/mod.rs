#![allow(dead_code)] // each test binary uses a different subset of these helpers

//! Shared helpers for integration tests.

use mesh_tools::GltfBuilder;

/// Export the builder to GLB bytes and parse them with the independent `gltf` crate.
/// Panics (failing the test) if the container is malformed.
pub fn parse_glb(builder: &GltfBuilder) -> gltf::Gltf {
    let bytes = builder.to_glb_bytes().expect("serialize GLB");
    gltf::Gltf::from_slice(&bytes).expect("exported GLB must parse")
}

/// Builder with one box mesh, one skin over a single joint node, and no scene.
/// Returns `(builder, mesh_index, skin_index, joint_index)`.
pub fn builder_with_mesh_and_skin() -> (GltfBuilder, usize, usize, usize) {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let joint = b.add_joint_node(Some("Joint".into()), None, None, None);
    let skin = b.add_skin(Some("Skin".into()), vec![joint], None, Some(joint)).unwrap();
    (b, mesh, skin, joint)
}
