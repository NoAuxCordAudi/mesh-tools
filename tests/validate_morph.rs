mod common;

use mesh_tools::{GltfBuilder, GltfError, JointIndices, MorphTarget, SkinSet};

const POS: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
const NRM: [f32; 9] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];

fn skinned_primitive(b: &mut GltfBuilder) -> mesh_tools::Primitive {
    let set = SkinSet { joints: JointIndices::U8(vec![[0; 4]; 3]), weights: vec![[1.0, 0.0, 0.0, 0.0]; 3] };
    b.create_skinned_primitive(&POS, Some(&[0, 1, 2]), Some(&NRM), None, &[set], 1, None).unwrap()
}

fn two_targets() -> Vec<MorphTarget> {
    vec![
        MorphTarget::new().with_positions(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]),
        MorphTarget::new().with_positions(vec![1.0; 9]).with_normals(vec![0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]),
    ]
}

/// A valid skinned + morphed document: mesh 0 (2 targets, weights, names), skinned node 1 with
/// its own weights, a plain node 2 (no mesh), joint node 0, one scene.
fn valid() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let joint = b.add_joint_node(Some("J".into()), None, None, None);
    let mut p = skinned_primitive(&mut b);
    b.attach_morph_targets(&mut p, &two_targets()).unwrap();
    let mesh = b.add_morphable_mesh(None, vec![p], Some(vec![0.0, 0.5]), Some(vec!["a".into(), "b".into()])).unwrap();
    let skin = b.add_skin_from_joints(None, vec![joint], Some(joint)).unwrap();
    let node = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.set_node_weights(node, vec![1.0, 0.0]).unwrap();
    let plain = b.add_node(None, None, None, None, None);
    b.add_scene(None, Some(vec![node, joint, plain]));
    b
}

/// Apply `edit` to a valid document and return its validation report.
fn edited(edit: impl FnOnce(&mut GltfBuilder)) -> mesh_tools::ValidationReport {
    let mut b = valid();
    edit(&mut b);
    b.validate()
}

#[track_caller]
fn assert_error(code: &str, edit: impl FnOnce(&mut GltfBuilder)) {
    let r = edited(edit);
    assert!(r.has_error(code), "expected {code}; errors: {:#?}", r.errors);
}

#[track_caller]
fn assert_warning(code: &str, edit: impl FnOnce(&mut GltfBuilder)) {
    let r = edited(edit);
    assert!(r.has_warning(code), "expected warning {code}; warnings: {:#?}", r.warnings);
    assert!(r.is_valid(), "warnings must not be errors: {:#?}", r.errors);
}

fn prim(b: &mut GltfBuilder) -> &mut mesh_tools::Primitive {
    &mut b.gltf.meshes.as_mut().unwrap()[0].primitives[0]
}

#[test]
fn the_baseline_document_is_valid_with_no_warnings_and_loads() {
    let b = valid();
    let r = b.validate();
    assert!(r.errors.is_empty(), "{:#?}", r.errors);
    assert!(r.warnings.is_empty(), "{:#?}", r.warnings);
    common::parse_glb(&b);
}

#[test]
fn documents_without_morph_data_are_unaffected() {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));
    let r = b.validate();
    assert!(r.errors.is_empty() && r.warnings.is_empty(), "{r:#?}");
}

#[test]
fn target_structure_errors() {
    assert_error("MORPH_TARGETS_EMPTY", |b| prim(b).targets = Some(vec![]));
    assert_error("MORPH_TARGET_EMPTY", |b| prim(b).targets.as_mut().unwrap()[0].clear());
    assert_error("MORPH_TARGETS_INCONSISTENT", |b| {
        let mut extra = skinned_primitive(b);
        b.attach_morph_targets(&mut extra, &two_targets()[..1]).unwrap(); // 1 target vs the mesh's 2
        b.gltf.meshes.as_mut().unwrap()[0].primitives.push(extra);
    });
}

#[test]
fn target_attribute_errors() {
    assert_error("MORPH_TARGET_ACCESSOR_INVALID", |b| {
        prim(b).targets.as_mut().unwrap()[0].insert("POSITION".into(), 999);
    });
    assert_error("MORPH_TARGET_ATTRIBUTE_INVALID", |b| {
        prim(b).targets.as_mut().unwrap()[0].insert("COLOR_0".into(), 0);
    });
    assert_error("MORPH_TARGET_ATTRIBUTE_NOT_IN_BASE", |b| {
        prim(b).attributes.remove("NORMAL"); // target 1 still has NORMAL deltas
    });
    // Application-specific `_` attributes are allowed names but must still exist in the base.
    assert_error("MORPH_TARGET_ATTRIBUTE_NOT_IN_BASE", |b| {
        prim(b).targets.as_mut().unwrap()[0].insert("_CUSTOM".into(), 0);
    });
}

#[test]
fn target_accessor_type_and_count_errors() {
    assert_error("MORPH_TARGET_TYPE", |b| {
        let indices = prim(b).indices.unwrap(); // a SCALAR unsigned accessor
        prim(b).targets.as_mut().unwrap()[0].insert("POSITION".into(), indices);
    });
    assert_error("MORPH_TARGET_COUNT_MISMATCH", |b| {
        let acc = prim(b).targets.as_ref().unwrap()[0]["POSITION"];
        b.gltf.accessors.as_mut().unwrap()[acc].count = 2;
    });
}

#[test]
fn mesh_weights_and_names_errors() {
    assert_error("MESH_WEIGHTS_EMPTY", |b| b.gltf.meshes.as_mut().unwrap()[0].weights = Some(vec![]));
    assert_error("MESH_WEIGHTS_COUNT", |b| b.gltf.meshes.as_mut().unwrap()[0].weights = Some(vec![0.0, 0.0, 0.0]));
    assert_error("MESH_WEIGHTS_NOT_FINITE", |b| b.gltf.meshes.as_mut().unwrap()[0].weights = Some(vec![f32::NAN, 0.0]));
    assert_error("MESH_TARGET_NAMES_COUNT", |b| {
        b.gltf.meshes.as_mut().unwrap()[0].extras = Some(serde_json::json!({ "targetNames": ["a", "b", "c"] }));
    });
    assert_error("MESH_TARGET_NAMES_INVALID", |b| {
        b.gltf.meshes.as_mut().unwrap()[0].extras = Some(serde_json::json!({ "targetNames": "a" }));
    });
    assert_error("MESH_TARGET_NAMES_INVALID", |b| {
        b.gltf.meshes.as_mut().unwrap()[0].extras = Some(serde_json::json!({ "targetNames": ["a", 7] }));
    });
}

#[test]
fn weights_on_a_mesh_without_targets_are_an_error_not_silently_ignored() {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    b.gltf.meshes.as_mut().unwrap()[mesh].weights = Some(vec![0.5]);
    assert!(b.validate().has_error("MESH_WEIGHTS_COUNT"));
}

#[test]
fn node_weights_errors() {
    assert_error("NODE_WEIGHTS_WITHOUT_MESH", |b| b.gltf.nodes.as_mut().unwrap()[2].weights = Some(vec![0.0, 0.0]));
    assert_error("NODE_WEIGHTS_COUNT", |b| b.gltf.nodes.as_mut().unwrap()[1].weights = Some(vec![0.0]));
    assert_error("NODE_WEIGHTS_NOT_FINITE", |b| b.gltf.nodes.as_mut().unwrap()[1].weights = Some(vec![0.0, f32::INFINITY]));
    // Node weights on a node whose mesh has no targets.
    assert_error("NODE_WEIGHTS_COUNT", |b| {
        let plain_mesh = b.create_box(1.0);
        let n = b.add_node(None, Some(plain_mesh), None, None, None);
        b.gltf.nodes.as_mut().unwrap()[n].weights = Some(vec![0.0]);
    });
}

#[test]
fn warnings_do_not_block_export() {
    assert_warning("MORPH_TARGET_POSITION_NO_BOUNDS", |b| {
        let acc = prim(b).targets.as_ref().unwrap()[0]["POSITION"];
        b.gltf.accessors.as_mut().unwrap()[acc].min = None;
    });
    assert_warning("MESH_TARGET_NAMES_DUPLICATE", |b| {
        b.gltf.meshes.as_mut().unwrap()[0].extras = Some(serde_json::json!({ "targetNames": ["a", "a"] }));
    });
    // Many targets with several attributes each exceed the heuristic attribute budget.
    let mut b = GltfBuilder::new();
    let mut p = skinned_primitive(&mut b);
    let targets: Vec<MorphTarget> = (0..7)
        .map(|_| MorphTarget::new().with_positions(vec![0.0; 9]).with_normals(vec![0.0; 9]))
        .collect();
    b.attach_morph_targets(&mut p, &targets).unwrap();
    b.add_morphable_mesh(None, vec![p], None, None).unwrap();
    let r = b.validate();
    assert!(r.has_warning("MORPH_ATTRIBUTE_BUDGET"), "{r:#?}");
    assert!(r.is_valid());
    assert!(b.to_glb_bytes().is_ok(), "a warning must not block export");
}

#[test]
fn export_refuses_a_non_finite_weight_instead_of_writing_null() {
    let mut b = valid();
    b.gltf.meshes.as_mut().unwrap()[0].weights = Some(vec![f32::NAN, 0.0]);
    let Err(GltfError::InvalidData(msg)) = b.to_glb_bytes() else { panic!("expected InvalidData") };
    assert!(msg.contains("MESH_WEIGHTS_NOT_FINITE"), "{msg}");
}
