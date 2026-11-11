//! Unit tests that need raw buffer access (stride-aware decoding of JOINTS_n).

use crate::builder::GltfBuilder;

/// Valid 2-joint skinned document where JOINTS_0 is later rewritten as strided data.
fn skinned() -> (GltfBuilder, usize) {
    let mut b = GltfBuilder::new();
    let names = vec!["A".to_string(), "B".to_string()];
    let joints = b.create_bone_chain(&names, &[[0.0; 3], [1.0, 0.0, 0.0]]).unwrap();
    let positions = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let mesh = b
        .create_skinned_mesh(
            None, &positions, &[0, 1, 2], None, None,
            &[[0, 0, 0, 0], [1, 0, 0, 0], [0, 1, 0, 0]],
            &[[1.0, 0.0, 0.0, 0.0]; 3], None,
        )
        .unwrap();
    let skin = b.add_skin_from_joints(None, joints.clone(), Some(joints[0])).unwrap();
    let node = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.add_scene(None, Some(vec![node, joints[0]]));
    assert!(b.validate().is_valid());
    (b, mesh)
}

/// Point JOINTS_0 at freshly appended strided bytes (UNSIGNED_SHORT, stride 12, 4 pad bytes per vertex).
fn rewrite_joints_strided(b: &mut GltfBuilder, mesh: usize, rows: [[u16; 4]; 3]) {
    let ja = b.gltf.meshes.as_ref().unwrap()[mesh].primitives[0].attributes["JOINTS_0"];
    let mut data = Vec::new();
    for v in rows {
        for j in v {
            data.extend_from_slice(&j.to_le_bytes());
        }
        data.extend_from_slice(&[9, 9, 9, 9]);
    }
    let (off, len) = b.add_buffer_data(&data);
    let vi = b.gltf.accessors.as_ref().unwrap()[ja].buffer_view;
    assert_eq!(b.gltf.accessors.as_ref().unwrap()[ja].component_type, 5123);
    let v = &mut b.gltf.buffer_views.as_mut().unwrap()[vi];
    v.byte_offset = off;
    v.byte_length = len;
    v.byte_stride = Some(12);
    b.gltf.accessors.as_mut().unwrap()[ja].byte_offset = None;
}

#[test]
fn joints_range_check_fires_on_strided_data() {
    let (mut b, mesh) = skinned();
    // vertex 1 slot 0 = 7, but the skin has only 2 joints
    rewrite_joints_strided(&mut b, mesh, [[0, 0, 0, 0], [7, 0, 0, 0], [0, 1, 0, 0]]);
    let r = b.validate();
    assert!(r.has_error("SKIN_JOINT_OUT_OF_RANGE"), "{:#?}", r.errors);
    assert!(r.errors.iter().any(|e| e.contains("vertex 1 slot 0 is 7")), "{:#?}", r.errors);
}

#[test]
fn valid_strided_joints_pass_and_padding_bytes_are_ignored() {
    let (mut b, mesh) = skinned();
    // padding is 9 (>= joint count) but must not be read as a joint index
    rewrite_joints_strided(&mut b, mesh, [[0, 0, 0, 0], [1, 0, 0, 0], [0, 1, 0, 0]]);
    let r = b.validate();
    assert!(r.is_valid(), "{:#?}", r.errors);
}
