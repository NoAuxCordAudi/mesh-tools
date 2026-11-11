mod common;

use gltf::scene::Transform;
use mesh_tools::{AnimationPath, GltfBuilder, GltfError, InterpolationType};

const IDENTITY: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
/// Uniform scale 2 and translation (1,2,3), column-major.
const SCALE2_T123: [f32; 16] = [2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 1.0, 2.0, 3.0, 1.0];

fn msg<T: std::fmt::Debug>(r: mesh_tools::Result<T>) -> String {
    match r {
        Err(GltfError::InvalidInput(m)) => m,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

fn read_transform(b: &GltfBuilder, node: usize) -> Transform {
    common::parse_glb(b).document.nodes().nth(node).unwrap().transform()
}

#[test]
fn matrix_reads_back_as_hand_derived_trs() {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let n = b.add_node_with_matrix(Some("m".into()), Some(mesh), SCALE2_T123).unwrap();
    b.add_scene(None, Some(vec![n]));
    let t = read_transform(&b, n);
    assert!(matches!(t, Transform::Matrix { .. }));
    let (tr, rot, sc) = t.decomposed();
    assert_eq!(tr, [1.0, 2.0, 3.0]);
    assert_eq!(rot, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(sc, [2.0, 2.0, 2.0]);
}

#[test]
fn matrix_from_trs_matches_hand_derived_values() {
    // 90 degrees about Z maps x to y and y to -x: columns (0,2,0) and (-3,0,0) after scale (2,3,4).
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let m = GltfBuilder::matrix_from_trs([5.0, 6.0, 7.0], [0.0, 0.0, s, s], [2.0, 3.0, 4.0]);
    let expect = [0.0, 2.0, 0.0, 0.0, -3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 5.0, 6.0, 7.0, 1.0];
    for (a, e) in m.iter().zip(expect) {
        assert!((a - e).abs() < 1e-5, "{m:?}");
    }
    // The independent reader decomposes it back.
    let mut b = GltfBuilder::new();
    let n = b.add_node_with_matrix(None, None, m).unwrap();
    let (t, r, sc) = read_transform(&b, n).decomposed();
    for (a, e) in t.iter().chain(&r).chain(&sc).zip([5.0, 6.0, 7.0, 0.0, 0.0, s, s, 2.0, 3.0, 4.0]) {
        assert!((a - e).abs() < 1e-5, "{t:?} {r:?} {sc:?}");
    }
}

#[test]
fn add_node_with_matrix_rejects_each_invalid_matrix_with_its_own_message() {
    let mut b = GltfBuilder::new();
    let mut nan = IDENTITY;
    nan[5] = f32::NAN;
    assert!(msg(b.add_node_with_matrix(None, None, nan)).starts_with("NODE_MATRIX_NOT_FINITE"));
    let mut bottom = IDENTITY;
    bottom[15] = 2.0;
    assert!(msg(b.add_node_with_matrix(None, None, bottom)).starts_with("NODE_MATRIX_NOT_AFFINE"));
    let mut persp = IDENTITY;
    persp[3] = 0.5;
    assert!(msg(b.add_node_with_matrix(None, None, persp)).starts_with("NODE_MATRIX_NOT_AFFINE"));
    let mut zero = [0.0; 16];
    zero[15] = 1.0;
    assert!(msg(b.add_node_with_matrix(None, None, zero)).starts_with("NODE_MATRIX_NOT_DECOMPOSABLE"));
    assert!(msg(b.add_node_with_matrix(None, Some(3), IDENTITY)).contains("mesh 3"));
    assert_eq!(b.node_count(), 0);
}

#[test]
fn negative_determinant_mirror_is_valid() {
    let mut b = GltfBuilder::new();
    let mut mirror = IDENTITY;
    mirror[0] = -1.0;
    let n = b.add_node_with_matrix(None, None, mirror).unwrap();
    b.add_scene(None, Some(vec![n]));
    let r = b.validate();
    assert!(r.is_valid() && r.warnings.is_empty(), "{r:#?}");
    common::parse_glb(&b);
}

#[test]
fn shear_is_only_a_warning() {
    let mut b = GltfBuilder::new();
    let mut shear = IDENTITY;
    shear[4] = 0.5; // x' = x + 0.5 y
    let n = b.add_node_with_matrix(None, None, shear).unwrap();
    b.add_scene(None, Some(vec![n]));
    let r = b.validate();
    assert!(r.is_valid() && r.has_warning("NODE_MATRIX_SHEAR"), "{r:#?}");
    assert!(b.to_glb_bytes().is_ok());
    // A pure rotation or non-uniform scale is not shear.
    let mut b = GltfBuilder::new();
    let m = GltfBuilder::matrix_from_trs([0.0; 3], GltfBuilder::quaternion_from_euler(0.3, 0.2, 0.1), [1.0, 2.0, 3.0]);
    b.add_node_with_matrix(None, None, m).unwrap();
    assert!(!b.validate().has_warning("NODE_MATRIX_SHEAR"));
}

#[test]
fn set_node_matrix_and_clear_node_matrix_round_trip() {
    let mut b = GltfBuilder::new();
    let n = b.add_node(None, None, None, None, None);
    b.set_node_matrix(n, SCALE2_T123).unwrap();
    assert_eq!(read_transform(&b, n).decomposed().0, [1.0, 2.0, 3.0]);
    b.clear_node_matrix(n).unwrap();
    assert!(matches!(read_transform(&b, n), Transform::Decomposed { .. }));
    b.clear_node_matrix(n).unwrap(); // already clear: harmless
    assert!(b.clear_node_matrix(4).is_err());
}

#[test]
fn set_node_matrix_rejects_trs_nodes_and_bad_input_without_clearing() {
    let mut b = GltfBuilder::new();
    let t = b.add_node(None, None, Some([1.0, 0.0, 0.0]), None, None);
    let r = b.add_node(None, None, None, Some([0.0, 0.0, 0.0, 1.0]), None);
    let s = b.add_node(None, None, None, None, Some([1.0; 3]));
    for n in [t, r, s] {
        assert!(msg(b.set_node_matrix(n, IDENTITY)).contains("TRS"), "node {n}");
        assert!(b.gltf.nodes.as_ref().unwrap()[n].matrix.is_none());
    }
    assert_eq!(b.gltf.nodes.as_ref().unwrap()[t].translation, Some([1.0, 0.0, 0.0]));
    let plain = b.add_node(None, None, None, None, None);
    assert!(msg(b.set_node_matrix(plain, [0.0; 16])).starts_with("NODE_MATRIX_NOT_AFFINE"));
    assert!(msg(b.set_node_matrix(99, IDENTITY)).contains("node 99"));
    assert!(b.gltf.nodes.as_ref().unwrap()[plain].matrix.is_none());
}

#[test]
fn set_node_transform_normalizes_and_rejects_matrix_nodes() {
    let mut b = GltfBuilder::new();
    let n = b.add_node(None, None, None, None, None);
    b.set_node_transform(n, [1.0, 2.0, 3.0], [0.0, 0.0, 2.0, 2.0], [1.0, 1.0, 1.0]).unwrap();
    let (t, r, s) = read_transform(&b, n).decomposed();
    let h = std::f32::consts::FRAC_1_SQRT_2;
    assert_eq!((t, s), ([1.0, 2.0, 3.0], [1.0; 3]));
    assert!((r[2] - h).abs() < 1e-6 && (r[3] - h).abs() < 1e-6, "{r:?}");
    assert!(b.validate().is_valid());

    let m = b.add_node_with_matrix(None, None, IDENTITY).unwrap();
    assert!(msg(b.set_node_transform(m, [0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3])).contains("clear_node_matrix"));
    assert!(b.gltf.nodes.as_ref().unwrap()[m].translation.is_none());

    let before = b.gltf.nodes.as_ref().unwrap()[n].translation;
    let id = [0.0, 0.0, 0.0, 1.0];
    assert!(msg(b.set_node_transform(n, [f32::NAN, 0.0, 0.0], id, [1.0; 3])).contains("translation"));
    assert!(msg(b.set_node_transform(n, [0.0; 3], id, [f32::INFINITY, 1.0, 1.0])).contains("scale"));
    assert!(msg(b.set_node_transform(n, [0.0; 3], [0.0; 4], [1.0; 3])).contains("rotation"));
    assert!(msg(b.set_node_transform(n, [0.0; 3], [f32::NAN, 0.0, 0.0, 1.0], [1.0; 3])).contains("rotation"));
    assert!(msg(b.set_node_transform(77, [0.0; 3], id, [1.0; 3])).contains("node 77"));
    assert_eq!(b.gltf.nodes.as_ref().unwrap()[n].translation, before);
}

#[test]
fn matrix_cannot_be_set_on_a_trs_animated_node_but_weights_are_fine() {
    let mut b = GltfBuilder::new();
    let n = b.add_node(None, None, None, None, None);
    let anim = b.add_animation(None);
    b.add_translation_track(anim, n, &[0.0], &[[0.0; 3]], InterpolationType::Linear).unwrap();
    let m = msg(b.set_node_matrix(n, IDENTITY));
    assert!(m.contains("animation 0") && m.contains("translation"), "{m}");
    assert!(b.gltf.nodes.as_ref().unwrap()[n].matrix.is_none());
    // And the other way round: a matrix node cannot be TRS animated.
    let k = b.add_node_with_matrix(None, None, IDENTITY).unwrap();
    assert!(b.add_rotation_track(anim, k, &[0.0], &[[0.0, 0.0, 0.0, 1.0]], InterpolationType::Linear).is_err());
    assert!(AnimationPath::Weights.to_string() == "weights");
}

#[test]
fn a_weights_channel_does_not_block_a_matrix() {
    use mesh_tools::{JointIndices, MorphTarget, SkinSet};
    let mut b = GltfBuilder::new();
    let set = SkinSet::new(JointIndices::U8(vec![[0; 4]; 3]), vec![[1.0, 0.0, 0.0, 0.0]; 3]);
    let pos = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let mut p = b.create_skinned_primitive(&pos, Some(&[0, 1, 2]), None, None, &[set], 1, None).unwrap();
    b.attach_morph_targets(&mut p, &[MorphTarget::new().with_positions(vec![0.1; 9])]).unwrap();
    let mesh = b.add_morphable_mesh(None, vec![p], None, None).unwrap();
    let n = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![n]));
    let anim = b.add_animation(None);
    b.add_weights_track(anim, n, &[0.0], &[vec![0.5]], InterpolationType::Linear).unwrap();
    b.set_node_matrix(n, SCALE2_T123).unwrap();
    assert!(b.validate().is_valid(), "{:#?}", b.validate().errors);
    common::parse_glb(&b);
}

#[test]
fn inverse_bind_matrix_of_a_matrix_joint_is_the_inverse_translation() {
    let mut b = GltfBuilder::new();
    let j = b.add_joint_node(None, None, None, None);
    let mut up = IDENTITY;
    up[13] = 1.0; // translate (0,1,0)
    b.set_node_matrix(j, up).unwrap();
    let ibm = b.compute_inverse_bind_matrices(&[j]).unwrap();
    let mut down = IDENTITY;
    down[13] = -1.0;
    assert_eq!(ibm[0], down);
}

#[test]
fn skin_with_matrix_joints_round_trips_through_the_reader() {
    let mut b = GltfBuilder::new();
    let root = b.add_joint_node(None, None, None, None);
    let child = b.add_joint_node(None, None, None, None);
    b.add_child_to_node(root, child).unwrap();
    b.set_node_matrix(root, SCALE2_T123).unwrap();
    let mut up = IDENTITY;
    up[13] = 1.0;
    b.set_node_matrix(child, up).unwrap();
    b.add_scene(None, Some(vec![root]));
    let ibm = b.compute_inverse_bind_matrices(&[root, child]).unwrap();
    // world(child) = M_root * up: scale 2, translation (1, 2 + 2, 3) = (1,4,3); inverse translation is -(1,4,3)/2.
    let c = ibm[1];
    assert!((c[0] - 0.5).abs() < 1e-6 && (c[12] + 0.5).abs() < 1e-6 && (c[13] + 2.0).abs() < 1e-6 && (c[14] + 1.5).abs() < 1e-6, "{c:?}");
    b.add_skin(None, vec![root, child], None, Some(root)).unwrap();
    assert!(b.validate().is_valid(), "{:#?}", b.validate().errors);
    common::parse_glb(&b);
}

#[test]
fn trs_validator_codes() {
    let id = [0.0, 0.0, 0.0, 1.0];
    let mut b = GltfBuilder::new();
    let n = b.add_node(None, None, Some([0.0; 3]), Some(id), Some([1.0; 3]));
    let set = |b: &mut GltfBuilder, f: &dyn Fn(&mut mesh_tools::Node)| f(&mut b.gltf.nodes.as_mut().unwrap()[n]);

    set(&mut b, &|x| x.translation = Some([f32::NAN, 0.0, 0.0]));
    assert!(b.validate().has_error("NODE_TRANSLATION_NOT_FINITE"));
    set(&mut b, &|x| x.translation = Some([0.0; 3]));
    set(&mut b, &|x| x.scale = Some([1.0, f32::INFINITY, 1.0]));
    assert!(b.validate().has_error("NODE_SCALE_NOT_FINITE"));
    set(&mut b, &|x| x.scale = Some([1.0; 3]));
    set(&mut b, &|x| x.rotation = Some([f32::NAN, 0.0, 0.0, 1.0]));
    assert!(b.validate().has_error("NODE_ROTATION_NOT_FINITE"));
    set(&mut b, &|x| x.rotation = Some([0.0, 0.0, 0.0, 2.0]));
    assert!(b.validate().has_error("NODE_ROTATION_NOT_UNIT"));
    set(&mut b, &|x| x.rotation = Some(id));
    assert!(b.validate().is_valid());
    set(&mut b, &|x| x.scale = Some([1.0, 0.0, 1.0]));
    let r = b.validate();
    assert!(r.has_warning("NODE_SCALE_ZERO") && r.is_valid(), "{r:#?}");
    set(&mut b, &|x| x.scale = Some([1.0; 3]));
    set(&mut b, &|x| x.children = Some(vec![]));
    assert!(b.validate().has_error("NODE_CHILDREN_EMPTY"));
}

#[test]
fn matrix_validator_codes_on_hand_edited_documents() {
    let mut b = GltfBuilder::new();
    let n = b.add_node(None, None, None, None, None);
    let mut check = |m: [f32; 16]| {
        b.gltf.nodes.as_mut().unwrap()[n].matrix = Some(m);
        b.validate()
    };
    let mut nan = IDENTITY;
    nan[0] = f32::NAN;
    assert!(check(nan).has_error("NODE_MATRIX_NOT_FINITE"));
    let mut bottom = IDENTITY;
    bottom[15] = 2.0;
    assert!(check(bottom).has_error("NODE_MATRIX_NOT_AFFINE"));
    let mut zero = [0.0; 16];
    zero[15] = 1.0;
    assert!(check(zero).has_error("NODE_MATRIX_NOT_DECOMPOSABLE"));
    // Two parallel columns: zero determinant.
    let mut flat = IDENTITY;
    flat[4] = 1.0;
    flat[5] = 0.0;
    assert!(check(flat).has_error("NODE_MATRIX_NOT_DECOMPOSABLE"));
    let mut shear = IDENTITY;
    shear[4] = 0.5;
    assert!(check(shear).has_warning("NODE_MATRIX_SHEAR"));
    assert!(check(SCALE2_T123).is_valid());
}

#[test]
fn set_node_transform_normalizes_tiny_and_huge_rotations_exactly() {
    let mut b = GltfBuilder::new();
    let n = b.add_node(None, None, None, None, None);
    // f32 squares of 1e-30 underflow to 0; this must not silently become the identity.
    b.set_node_transform(n, [0.0; 3], [0.0, 1e-30, 0.0, 0.0], [1.0; 3]).unwrap();
    assert_eq!(read_transform(&b, n).decomposed().1, [0.0, 1.0, 0.0, 0.0]);
    // Squares of 1e30 overflow f32.
    b.set_node_transform(n, [0.0; 3], [0.0, 0.0, 3e30, 3e30], [1.0; 3]).unwrap();
    let r = read_transform(&b, n).decomposed().1;
    let h = std::f32::consts::FRAC_1_SQRT_2;
    assert!((r[2] - h).abs() < 1e-6 && (r[3] - h).abs() < 1e-6, "{r:?}");
    assert!(b.validate().is_valid());
}
