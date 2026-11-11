//! Skin validator codes (N6), set_node_skin, and IBM-less skins. Corrupt data is injected through
//! the public accessor/view writers so the stored bytes are exactly what each test says.
mod common;

use mesh_tools::{AccessorSpec, AccessorType, BufferViewTarget, GltfBuilder, GltfError, JointIndices, SkinSet, ValidationReport};

const POS: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// Valid document: 2-joint skin with IBMs, one skinned node (mesh 0), scene = [skinned node, joint 0].
fn valid() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let joints = b.create_bone_chain(&["A".into(), "B".into()], &[[0.0; 3], [0.0, 1.0, 0.0]]).unwrap();
    let skin = b.add_skin_from_joints(None, joints.clone(), Some(joints[0])).unwrap();
    let set = SkinSet::new(JointIndices::U8(vec![[0, 1, 0, 0]; 3]), vec![[200u8, 55, 0, 0]; 3]);
    let prim = b.create_skinned_primitive(&POS, Some(&[0, 1, 2]), None, None, &[set], 2, None).unwrap();
    let mesh = b.add_mesh(None, vec![prim]);
    let node = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.add_scene(None, Some(vec![node, joints[0]]));
    b
}

fn report(b: &GltfBuilder) -> ValidationReport {
    b.validate()
}

/// Replace attribute `name` of mesh 0 primitive 0 with a new accessor.
fn replace_attr(b: &mut GltfBuilder, name: &str, acc: usize) {
    b.gltf.meshes.as_mut().unwrap()[0].primitives[0].attributes.insert(name.to_string(), acc);
}

fn f32_weights(b: &mut GltfBuilder, w: &[[f32; 4]]) -> usize {
    let flat: Vec<f32> = w.iter().flatten().copied().collect();
    b.add_accessor_from_slice(&flat, AccessorType::Vec4, AccessorSpec::new().with_target(BufferViewTarget::ArrayBuffer)).unwrap()
}

#[test]
fn valid_document_is_clean() {
    let r = report(&valid());
    assert!(r.is_valid() && r.warnings.is_empty(), "{r:#?}");
}

#[test]
fn weights_negative_is_an_error() {
    let mut b = valid();
    let a = f32_weights(&mut b, &[[1.5, -0.5, 0.0, 0.0]; 3]);
    replace_attr(&mut b, "WEIGHTS_0", a);
    let r = report(&b);
    assert!(r.has_error("SKIN_WEIGHTS_NEGATIVE"), "{r:#?}");
    assert!(r.errors.iter().any(|e| e.contains("vertex 0 slot 1")), "{r:#?}");
}

#[test]
fn weights_sum_error_float_and_integer() {
    let mut b = valid();
    let a = f32_weights(&mut b, &[[0.5, 0.4, 0.0, 0.0]; 3]);
    replace_attr(&mut b, "WEIGHTS_0", a);
    assert!(report(&b).has_error("SKIN_WEIGHTS_SUM"));

    // u8 sum 250/255 is off by five steps; 254 (one step) is tolerated.
    for (w, bad) in [([200u8, 50, 0, 0], true), ([200, 54, 0, 0], false)] {
        let mut b = valid();
        let flat: Vec<u8> = vec![w; 3].into_iter().flatten().collect();
        let a = b.add_accessor_from_slice(&flat, AccessorType::Vec4, AccessorSpec::new().with_normalized(true)).unwrap();
        replace_attr(&mut b, "WEIGHTS_0", a);
        assert_eq!(report(&b).has_error("SKIN_WEIGHTS_SUM"), bad, "{w:?}");
    }
    // u16 is judged at u16 resolution: 65533 is two steps short.
    let mut b = valid();
    let flat: Vec<u16> = vec![[40000u16, 25533, 0, 0]; 3].into_iter().flatten().collect();
    let a = b.add_accessor_from_slice(&flat, AccessorType::Vec4, AccessorSpec::new().with_normalized(true)).unwrap();
    replace_attr(&mut b, "WEIGHTS_0", a);
    assert!(report(&b).has_error("SKIN_WEIGHTS_SUM"));
}

#[test]
fn weights_sum_is_taken_over_all_sets() {
    // Two sets whose weights are 0.5 each sum to 1 overall: valid.
    let mut b = GltfBuilder::new();
    let j = b.add_joint_node(None, None, None, None);
    let s0 = SkinSet::new(JointIndices::U8(vec![[0, 0, 0, 0]; 3]), vec![[0.5f32, 0.0, 0.0, 0.0]; 3]);
    let s1 = SkinSet::new(JointIndices::U8(vec![[1, 0, 0, 0]; 3]), vec![[128u8, 0, 0, 0]; 3]);
    // Mixed f32 + u8 is judged with the 1e-5 float tolerance: 0.5 + 128/255 is too far from 1.
    assert!(b.create_skinned_primitive(&POS, Some(&[0, 1, 2]), None, None, &[s0.clone(), s1], 2, None).is_err());
    let s1 = SkinSet::new(JointIndices::U8(vec![[1, 0, 0, 0]; 3]), vec![[0.5f32, 0.0, 0.0, 0.0]; 3]);
    let p = b.create_skinned_primitive(&POS, Some(&[0, 1, 2]), None, None, &[s0, s1], 2, None).unwrap();
    let mesh = b.add_mesh(None, vec![p]);
    let j1 = b.add_joint_node(None, None, None, None);
    b.add_child_to_node(j, j1).unwrap();
    let skin = b.add_skin(None, vec![j, j1], None, None).unwrap();
    let n = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.add_scene(None, Some(vec![n, j]));
    let r = b.validate();
    assert!(r.is_valid(), "{r:#?}");
}

#[test]
fn vertex_joint_duplicate_is_an_error_and_distinct_from_skin_joint_duplicate() {
    let mut b = valid();
    let a = b.add_accessor_from_slice(&[1u8, 1, 0, 0].repeat(3), AccessorType::Vec4, AccessorSpec::new()).unwrap();
    replace_attr(&mut b, "JOINTS_0", a);
    let r = report(&b);
    assert!(r.has_error("SKIN_VERTEX_JOINT_DUPLICATE"), "{r:#?}");
    assert!(!r.has_error("SKIN_JOINT_DUPLICATE"), "{r:#?}");
}

#[test]
fn zero_weight_joint_index_is_a_warning_not_an_error() {
    let mut b = valid();
    // Slot 2 carries joint 1 with weight 0.
    let a = b.add_accessor_from_slice(&[0u8, 1, 1, 0].repeat(3), AccessorType::Vec4, AccessorSpec::new()).unwrap();
    replace_attr(&mut b, "JOINTS_0", a);
    let r = report(&b);
    assert!(r.is_valid(), "{r:#?}");
    assert!(r.warnings.iter().any(|w| w.starts_with("SKIN_JOINT_ZERO_WEIGHT_INDEX") && w.contains("slot 2")), "{r:#?}");
}

#[test]
fn ibm_view_with_a_target_is_an_error() {
    let mut b = valid();
    let ident: Vec<f32> = [1.0f32, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.].repeat(2);
    let a = b
        .add_accessor_from_slice(&ident, AccessorType::Mat4, AccessorSpec::new().with_target(BufferViewTarget::ArrayBuffer))
        .unwrap();
    b.gltf.skins.as_mut().unwrap()[0].inverse_bind_matrices = Some(a);
    let r = report(&b);
    assert!(r.has_error("SKIN_IBM_VIEW_TARGET"), "{r:#?}");
}

#[test]
fn ibm_view_with_a_stride_is_reported_by_the_existing_code() {
    // The stride rule already exists as BUFFER_VIEW_STRIDE_ON_NON_VERTEX; no SKIN_IBM_VIEW_STRIDE duplicate.
    let mut b = valid();
    let view = b.gltf.accessors.as_ref().unwrap().iter().find(|a| a.type_ == "MAT4").unwrap().buffer_view.unwrap();
    b.gltf.buffer_views.as_mut().unwrap()[view].byte_stride = Some(64);
    let r = report(&b);
    assert!(r.has_error("BUFFER_VIEW_STRIDE_ON_NON_VERTEX"), "{r:#?}");
    assert!(!r.errors.iter().any(|e| e.starts_with("SKIN_IBM_VIEW_STRIDE")), "{r:#?}");
}

#[test]
fn skinned_node_not_root_warns() {
    let mut b = valid();
    let skinned = b.gltf.nodes.as_ref().unwrap().iter().position(|n| n.skin.is_some()).unwrap();
    let parent = b.add_node(None, None, None, None, None);
    b.add_child_to_node(parent, skinned).unwrap();
    // Move the scene root from the skinned node to its new parent.
    b.gltf.scenes.as_mut().unwrap()[0].nodes = Some(vec![parent, 0]);
    let r = report(&b);
    assert!(r.is_valid(), "{r:#?}");
    assert!(r.warnings.iter().any(|w| w.starts_with("SKINNED_NODE_NOT_ROOT")), "{r:#?}");
}

#[test]
fn skinned_mesh_without_skin_warns() {
    let mut b = valid();
    let mesh = 0;
    let n = b.add_node(None, Some(mesh), None, None, None);
    b.gltf.scenes.as_mut().unwrap()[0].nodes.as_mut().unwrap().push(n);
    let r = report(&b);
    assert!(r.is_valid(), "{r:#?}");
    assert!(r.warnings.iter().any(|w| w.starts_with("SKINNED_MESH_WITHOUT_SKIN") && w.contains(&format!("node {n}"))), "{r:#?}");
}

#[test]
fn skin_mesh_missing_attributes_stays_an_error() {
    let mut b = valid();
    let plain = b.create_box(1.0);
    let skin = 0;
    let n = b.add_skinned_node(None, plain, skin, None, None, None).unwrap();
    b.gltf.scenes.as_mut().unwrap()[0].nodes.as_mut().unwrap().push(n);
    assert!(report(&b).has_error("SKIN_MESH_MISSING_ATTRIBUTES"));
}

// NODE_SKIN_INVALID out-of-range and set_node_skin.

#[test]
fn node_skin_out_of_range_is_reported() {
    let mut b = valid();
    let node = b.gltf.nodes.as_ref().unwrap().iter().position(|n| n.skin.is_some()).unwrap();
    b.gltf.nodes.as_mut().unwrap()[node].skin = Some(1); // only skin 0 exists
    let r = report(&b);
    assert!(r.has_error("NODE_SKIN_INVALID"), "{r:#?}");
    assert!(r.errors.iter().any(|e| e.contains("references skin 1 (1 skins)")), "{r:#?}");
}

#[test]
fn set_node_skin_rules_and_atomicity() {
    let mut b = valid();
    let mesh = 0;
    let n = b.add_node(None, Some(mesh), None, None, None);
    let bare = b.add_node(None, None, None, None, None);
    let before = serde_json::to_string(&b.gltf).unwrap();
    for (node, skin) in [(99, 0), (n, 5), (bare, 0)] {
        assert!(matches!(b.set_node_skin(node, skin), Err(GltfError::InvalidInput(_))), "node {node} skin {skin}");
    }
    assert_eq!(serde_json::to_string(&b.gltf).unwrap(), before);
    b.set_node_skin(n, 0).unwrap();
    assert_eq!(b.gltf.nodes.as_ref().unwrap()[n].skin, Some(0));
    let after = serde_json::to_string(&b.gltf).unwrap();
    assert!(b.set_node_skin(n, 0).is_err(), "already skinned");
    assert_eq!(serde_json::to_string(&b.gltf).unwrap(), after);
}

#[test]
fn add_skinned_node_keeps_its_errors_and_leaves_the_document_unchanged() {
    let mut b = valid();
    let before = serde_json::to_string(&b.gltf).unwrap();
    assert!(matches!(b.add_skinned_node(None, 0, 7, None, None, None), Err(GltfError::InvalidIndex)));
    assert!(matches!(b.add_skinned_node(None, 7, 0, None, None, None), Err(GltfError::InvalidIndex)));
    assert_eq!(serde_json::to_string(&b.gltf).unwrap(), before);
}

// A skin without inverseBindMatrices (identity is implied) is valid and survives a round trip.

#[test]
fn ibm_less_skin_is_valid_and_round_trips() {
    let mut b = GltfBuilder::new();
    let joints = b.create_bone_chain(&["A".into(), "B".into()], &[[0.0; 3], [0.0, 1.0, 0.0]]).unwrap();
    let skin = b.add_skin(Some("NoIbm".into()), joints.clone(), None, Some(joints[0])).unwrap();
    let set = SkinSet::new(JointIndices::U8(vec![[0, 1, 0, 0]; 3]), vec![[128u8, 127, 0, 0]; 3]);
    let prim = b.create_skinned_primitive(&POS, Some(&[0, 1, 2]), None, None, &[set], 2, None).unwrap();
    let mesh = b.add_mesh(None, vec![prim]);
    let node = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.add_scene(None, Some(vec![node, joints[0]]));
    let r = b.validate();
    assert!(r.is_valid() && r.warnings.is_empty(), "{r:#?}");

    let glb = common::parse_glb(&b);
    let s = glb.document.skins().next().unwrap();
    assert!(s.inverse_bind_matrices().is_none());
    assert_eq!(s.joints().count(), 2);
    let reader = s.reader(|_| glb.blob.as_deref());
    assert!(reader.read_inverse_bind_matrices().is_none());
}

#[test]
fn huge_number_of_weight_sets_does_not_overflow_the_sum() {
    // 16385 u16 WEIGHTS sets of 4 x 65535 each exceed u32 when summed; must be an error, not a panic.
    let mut b = valid();
    let flat: Vec<u16> = vec![65535; 12];
    let w = b.add_accessor_from_slice(&flat, AccessorType::Vec4, AccessorSpec::new().with_target(BufferViewTarget::ArrayBuffer).with_normalized(true)).unwrap();
    let j = b.gltf.meshes.as_ref().unwrap()[0].primitives[0].attributes["JOINTS_0"];
    for n in 0..16385 {
        replace_attr(&mut b, &format!("JOINTS_{n}"), j);
        replace_attr(&mut b, &format!("WEIGHTS_{n}"), w);
    }
    assert!(report(&b).has_error("SKIN_WEIGHTS_SUM"));
}
