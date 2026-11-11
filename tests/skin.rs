mod common;

use mesh_tools::{GltfBuilder, GltfError};

const EPS: f32 = 1e-5;

/// Read back the inverse bind matrices of skin 0 through the independent `gltf` reader.
/// Each matrix is flattened column-major, exactly as stored.
fn read_ibms(b: &GltfBuilder) -> Vec<[f32; 16]> {
    let glb = common::parse_glb(b);
    let skin = glb.document.skins().next().expect("a skin");
    let reader = skin.reader(|_| glb.blob.as_deref());
    reader
        .read_inverse_bind_matrices()
        .expect("skin has inverse bind matrices")
        .map(|m| {
            let mut out = [0.0; 16];
            for c in 0..4 {
                out[c * 4..c * 4 + 4].copy_from_slice(&m[c]);
            }
            out
        })
        .collect()
}

/// Apply a column-major affine matrix to a point.
fn transform(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0; 3];
    for r in 0..3 {
        out[r] = m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r];
    }
    out
}

fn assert_close(a: &[f32], b: &[f32]) {
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() < EPS, "{a:?} != {b:?}");
    }
}

fn node(b: &mut GltfBuilder, t: [f32; 3]) -> usize {
    b.add_joint_node(None, Some(t), None, None)
}

#[test]
fn ibm_of_translated_joint_has_negated_translation_in_elements_12_to_14() {
    let mut b = GltfBuilder::new();
    let j = node(&mut b, [2.0, 0.0, 0.0]);
    let ibm = b.compute_inverse_bind_matrices(&[j]).unwrap();
    assert_eq!(
        ibm[0],
        [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -2.0, 0.0, 0.0, 1.0]
    );
}

#[test]
fn ibm_of_two_level_chain_uses_cumulative_translation() {
    let mut b = GltfBuilder::new();
    let a = node(&mut b, [1.0, 0.0, 0.0]);
    let c = node(&mut b, [0.0, 3.0, 0.0]);
    b.add_child_to_node(a, c).unwrap();
    let ibm = b.compute_inverse_bind_matrices(&[a, c]).unwrap();
    assert_close(&ibm[0][12..15], &[-1.0, 0.0, 0.0]);
    assert_close(&ibm[1][12..15], &[-1.0, -3.0, 0.0]);
}

#[test]
fn ibm_with_rotated_scaled_parent_maps_bind_pose_world_points_back_to_joint_space() {
    let mut b = GltfBuilder::new();
    let s = std::f32::consts::FRAC_1_SQRT_2;
    // Parent: translate (1,0,0), rotate 90 deg about Z, scale 2.
    let parent = b.add_joint_node(None, Some([1.0, 0.0, 0.0]), Some([0.0, 0.0, s, s]), Some([2.0, 2.0, 2.0]));
    let child = node(&mut b, [1.0, 0.0, 0.0]);
    b.add_child_to_node(parent, child).unwrap();

    let ibm = b.compute_inverse_bind_matrices(&[parent, child]).unwrap();

    // Hand-derived world positions. Child origin: T + R*S*(1,0,0) = (1,0,0) + (0,2,0).
    assert_close(&transform(&ibm[1], [1.0, 2.0, 0.0]), &[0.0, 0.0, 0.0]);
    // Child-local point (1,0,0) -> parent space (2,0,0) -> *2 -> (4,0,0) -> rot90 -> (0,4,0) -> +T.
    assert_close(&transform(&ibm[1], [1.0, 4.0, 0.0]), &[1.0, 0.0, 0.0]);
    // Parent origin sits at (1,0,0) in world.
    assert_close(&transform(&ibm[0], [1.0, 0.0, 0.0]), &[0.0, 0.0, 0.0]);
}

#[test]
fn ibm_uses_node_matrix_when_present() {
    let mut b = GltfBuilder::new();
    let j = node(&mut b, [0.0; 3]);
    b.gltf.nodes.as_mut().unwrap()[j].matrix =
        Some([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, 0.0, 0.0, 1.0]);
    let ibm = b.compute_inverse_bind_matrices(&[j]).unwrap();
    assert_close(&ibm[0][12..15], &[-5.0, 0.0, 0.0]);
}

#[test]
fn ibm_of_zero_scale_joint_is_an_error() {
    let mut b = GltfBuilder::new();
    let j = b.add_joint_node(None, None, None, Some([0.0, 1.0, 1.0]));
    assert!(matches!(b.compute_inverse_bind_matrices(&[j]), Err(GltfError::InvalidInput(_))));
}

#[test]
fn ibm_of_missing_joint_is_an_error() {
    let b = GltfBuilder::new();
    assert!(b.compute_inverse_bind_matrices(&[0]).is_err());
}

#[test]
fn ibm_accessor_has_no_buffer_view_target_and_round_trips() {
    let mut b = GltfBuilder::new();
    let j = node(&mut b, [2.0, 0.0, 0.0]);
    b.add_skin_from_joints(None, vec![j], Some(j)).unwrap();

    let glb = common::parse_glb(&b);
    let skin = glb.document.skins().next().unwrap();
    let view = skin.inverse_bind_matrices().unwrap().view().unwrap();
    assert!(view.target().is_none(), "IBM buffer view must not declare a target");

    let ibm = read_ibms(&b);
    assert_eq!(ibm.len(), 1);
    assert_close(&ibm[0][12..15], &[-2.0, 0.0, 0.0]);
}

#[test]
fn mint_matrix_columns_are_written_as_columns() {
    let mut b = GltfBuilder::new();
    let j = node(&mut b, [0.0; 3]);
    // Translation (-2,0,0) belongs in the last *column* (.w), components x/y/z.
    let m = mint::ColumnMatrix4 {
        x: mint::Vector4 { x: 1.0, y: 0.0, z: 0.0, w: 0.0 },
        y: mint::Vector4 { x: 0.0, y: 1.0, z: 0.0, w: 0.0 },
        z: mint::Vector4 { x: 0.0, y: 0.0, z: 1.0, w: 0.0 },
        w: mint::Vector4 { x: -2.0, y: 0.0, z: 0.0, w: 1.0 },
    };
    let acc = b.create_inverse_bind_matrices_accessor(&[m]).unwrap();
    b.add_skin(None, vec![j], Some(acc), None).unwrap();
    assert_close(&read_ibms(&b)[0][12..15], &[-2.0, 0.0, 0.0]);
}

#[test]
fn ibm_accessor_rejects_empty_and_non_finite() {
    let mut b = GltfBuilder::new();
    assert!(b.create_inverse_bind_matrices_accessor_from_f32(&[]).is_err());
    let mut m = [0.0; 16];
    m[0] = f32::NAN;
    assert!(b.create_inverse_bind_matrices_accessor_from_f32(&[m]).is_err());
}

fn skin_count(b: &GltfBuilder) -> usize {
    b.gltf.skins.as_ref().map_or(0, |s| s.len())
}

#[test]
fn add_skin_rejects_invalid_joints_and_adds_nothing() {
    let mut b = GltfBuilder::new();
    let j = node(&mut b, [0.0; 3]);
    assert!(b.add_skin(None, vec![], None, None).is_err(), "empty");
    assert!(b.add_skin(None, vec![j, j], None, None).is_err(), "duplicate");
    assert!(b.add_skin(None, vec![j + 5], None, None).is_err(), "out of range");
    assert!(b.add_skin(None, vec![j], None, Some(j + 5)).is_err(), "bad skeleton");
    assert_eq!(skin_count(&b), 0);
}

#[test]
fn add_skin_requires_common_root() {
    let mut b = GltfBuilder::new();
    let a = node(&mut b, [0.0; 3]);
    let c = node(&mut b, [0.0; 3]); // separate root
    assert!(b.add_skin(None, vec![a, c], None, None).is_err());
    assert_eq!(skin_count(&b), 0);
}

#[test]
fn add_skin_skeleton_must_be_ancestor_or_self_of_all_joints() {
    let mut b = GltfBuilder::new();
    let root = node(&mut b, [0.0; 3]);
    let a = node(&mut b, [0.0; 3]);
    let c = node(&mut b, [0.0; 3]);
    b.add_child_to_node(root, a).unwrap();
    b.add_child_to_node(root, c).unwrap();

    assert!(b.add_skin(None, vec![a, c], None, Some(a)).is_err(), "sibling is not an ancestor");
    assert!(b.add_skin(None, vec![a, c], None, Some(root)).is_ok(), "common parent is fine");
    assert!(b.add_skin(None, vec![a], None, Some(a)).is_ok(), "joint itself is fine");
}

#[test]
fn add_skin_validates_ibm_accessor() {
    let mut b = GltfBuilder::new();
    let mesh_unused = b.create_box(1.0); // creates VEC3 accessors, index 0 is POSITION
    let _ = mesh_unused;
    let j1 = node(&mut b, [0.0; 3]);
    let j2 = node(&mut b, [0.0; 3]);
    b.add_child_to_node(j1, j2).unwrap();

    assert!(b.add_skin(None, vec![j1], Some(999), None).is_err(), "missing accessor");
    assert!(b.add_skin(None, vec![j1], Some(0), None).is_err(), "VEC3 accessor is not MAT4");

    let one = b.create_inverse_bind_matrices_accessor_from_f32(&[[0.0; 16]]).unwrap();
    assert!(b.add_skin(None, vec![j1, j2], Some(one), None).is_err(), "too few matrices");
    assert!(b.add_skin(None, vec![j1], Some(one), None).is_ok());
}

#[test]
fn add_skin_from_joints_on_bone_chain() {
    let mut b = GltfBuilder::new();
    let names: Vec<String> = ["Shoulder", "Elbow", "Wrist"].iter().map(|s| s.to_string()).collect();
    let pos = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
    let joints = b.create_bone_chain(&names, &pos).unwrap();
    b.add_skin_from_joints(Some("Arm".into()), joints.clone(), Some(joints[0])).unwrap();

    let ibm = read_ibms(&b);
    assert_eq!(ibm.len(), 3);
    assert_close(&ibm[0][12..15], &[0.0, 0.0, 0.0]);
    assert_close(&ibm[1][12..15], &[-2.0, 0.0, 0.0]);
    assert_close(&ibm[2][12..15], &[-4.0, 0.0, 0.0]);
}

#[test]
fn add_skin_from_joints_is_atomic_on_error() {
    let mut b = GltfBuilder::new();
    let j = b.add_joint_node(None, None, None, Some([0.0, 1.0, 1.0])); // singular
    let acc_before = b.gltf.accessors.as_ref().map_or(0, |a| a.len());
    assert!(b.add_skin_from_joints(None, vec![j], None).is_err());
    assert_eq!(b.gltf.accessors.as_ref().map_or(0, |a| a.len()), acc_before);
    assert_eq!(skin_count(&b), 0);
}

#[test]
fn ibm_handles_a_node_matrix_with_shear() {
    // Column-major matrix: x' = x + 0.5*y (shear in XY), translation (3, 0, 0).
    let m = [1.0, 0.0, 0.0, 0.0, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 3.0, 0.0, 0.0, 1.0];
    let mut b = GltfBuilder::new();
    let j = node(&mut b, [0.0; 3]);
    b.gltf.nodes.as_mut().unwrap()[j].matrix = Some(m);
    let ibm = b.compute_inverse_bind_matrices(&[j]).unwrap().remove(0);
    // world * ibm must be the identity: apply ibm to a world point, expect the local point.
    let local = [1.0, 2.0, 0.0];
    let world = transform(&m, local);
    assert_close(&transform(&ibm, world), &local);
}
