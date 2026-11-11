mod common;

use gltf::accessor::{DataType, Dimensions};
use mesh_tools::{GltfBuilder, GltfError, JointIndices, MorphDelta, MorphTarget, Primitive, SkinSet};

/// Triangle with base normals, as a skinned primitive (one joint). Not yet added to a mesh.
fn base_primitive(b: &mut GltfBuilder) -> Primitive {
    let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let normals = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let set = SkinSet { joints: JointIndices::U8(vec![[0; 4]; 3]), weights: vec![[1.0, 0.0, 0.0, 0.0]; 3] };
    b.create_skinned_primitive(&positions, Some(&[0, 1, 2]), Some(&normals), None, &[set], 1, None).unwrap()
}

fn counts(b: &GltfBuilder) -> (usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffers.as_ref().map_or(0, |a| a.first().map_or(0, |b| b.byte_length)),
    )
}

fn err(r: mesh_tools::Result<()>) -> String {
    match r {
        Err(GltfError::InvalidInput(m)) => m,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

const D0: [f32; 9] = [0.0, 0.0, 1.0, 0.0, 0.5, 0.0, -0.25, 0.0, 0.0];
const D1: [f32; 9] = [1.0, 2.0, 3.0, -1.0, -2.0, -3.0, 0.5, 0.5, 0.5];
const N1: [f32; 9] = [0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0];

#[test]
fn deltas_round_trip_through_the_reference_loader_in_order() {
    let mut b = GltfBuilder::new();
    let mut prim = base_primitive(&mut b);
    b.attach_morph_targets(
        &mut prim,
        &[
            MorphTarget::new().with_positions(D0.to_vec()),
            MorphTarget::new().with_positions(D1.to_vec()).with_normals(N1.to_vec()),
        ],
    )
    .unwrap();
    b.add_mesh(None, vec![prim]);

    let glb = common::parse_glb(&b);
    let p = glb.document.meshes().next().unwrap().primitives().next().unwrap();
    assert_eq!(p.morph_targets().count(), 2);
    let reader = p.reader(|_| glb.blob.as_deref());
    let targets: Vec<_> = reader.read_morph_targets().collect();

    let t0_pos: Vec<f32> = targets[0].0.clone().unwrap().flatten().collect();
    assert_eq!(t0_pos, D0.to_vec());
    assert!(targets[0].1.is_none(), "target 0 has no normal deltas");
    let t1_pos: Vec<f32> = targets[1].0.clone().unwrap().flatten().collect();
    let t1_nrm: Vec<f32> = targets[1].1.clone().unwrap().flatten().collect();
    assert_eq!(t1_pos, D1.to_vec(), "targets keep the order they were given in");
    assert_eq!(t1_nrm, N1.to_vec());
}

#[test]
fn delta_accessors_are_float_vec3_array_buffer_with_hand_computed_bounds() {
    let mut b = GltfBuilder::new();
    let mut prim = base_primitive(&mut b);
    b.attach_morph_targets(&mut prim, &[MorphTarget::new().with_positions(D1.to_vec()).with_normals(N1.to_vec())]).unwrap();
    b.add_mesh(None, vec![prim]);

    let glb = common::parse_glb(&b);
    let p = glb.document.meshes().next().unwrap().primitives().next().unwrap();
    let t = p.morph_targets().next().unwrap();
    let pos = t.positions().unwrap();
    assert_eq!((pos.data_type(), pos.dimensions(), pos.count()), (DataType::F32, Dimensions::Vec3, 3));
    assert_eq!(pos.view().unwrap().target(), Some(gltf::buffer::Target::ArrayBuffer));
    let as_vec = |v: gltf::json::Value| -> Vec<f64> { v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() };
    // D1 triples: (1,2,3) (-1,-2,-3) (0.5,0.5,0.5)
    assert_eq!(as_vec(pos.min().unwrap()), vec![-1.0, -2.0, -3.0]);
    assert_eq!(as_vec(pos.max().unwrap()), vec![1.0, 2.0, 3.0]);
    let nrm = t.normals().unwrap();
    assert_eq!((nrm.data_type(), nrm.dimensions()), (DataType::F32, Dimensions::Vec3));
}

#[test]
fn an_all_zero_target_is_legal() {
    let mut b = GltfBuilder::new();
    let mut prim = base_primitive(&mut b);
    b.attach_morph_targets(&mut prim, &[MorphTarget::new().with_positions(vec![0.0; 9])]).unwrap();
    assert_eq!(prim.targets.as_ref().unwrap().len(), 1);
}

#[test]
fn a_skinned_and_morphed_primitive_exports_and_reloads() {
    let mut b = GltfBuilder::new();
    let mut prim = base_primitive(&mut b);
    b.attach_morph_targets(&mut prim, &[MorphTarget::new().with_positions(D0.to_vec())]).unwrap();
    b.add_mesh(None, vec![prim]);
    let glb = common::parse_glb(&b);
    let p = glb.document.meshes().next().unwrap().primitives().next().unwrap();
    assert!(p.get(&gltf::Semantic::Joints(0)).is_some() && p.morph_targets().count() == 1);
}

#[test]
fn a_box_from_the_generators_can_be_morphed_via_its_primitive() {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let mut prim = std::mem::take(&mut b.gltf.meshes.as_mut().unwrap()[mesh].primitives[0]);
    let n = b.gltf.accessors.as_ref().unwrap()[prim.attributes["POSITION"]].count;
    b.attach_morph_targets(&mut prim, &[MorphTarget::new().with_positions(vec![0.0; n * 3]).with_normals(vec![0.0; n * 3])]).unwrap();
    b.gltf.meshes.as_mut().unwrap()[mesh].primitives[0] = prim;
    common::parse_glb(&b);
}

#[test]
fn invalid_input_is_rejected_atomically_with_actionable_messages() {
    let mut b = GltfBuilder::new();
    let mut prim = base_primitive(&mut b);
    let before = counts(&b);

    let ok = || MorphTarget::new().with_positions(D0.to_vec());
    let cases: Vec<(&str, Vec<MorphTarget>, &str)> = vec![
        ("no targets", vec![], "at least one morph target"),
        ("target without deltas", vec![ok(), MorphTarget::new()], "morph target 1 has no deltas"),
        ("short positions", vec![MorphTarget::new().with_positions(vec![0.0; 6])], "expected 9"),
        ("long normals", vec![ok(), MorphTarget::new().with_normals(vec![0.0; 12])], "morph target 1: normal deltas have 12"),
        ("tangents without base", vec![MorphTarget::new().with_positions(D0.to_vec()).with_tangents(vec![0.0; 9])], "tangent deltas but the primitive has no base TANGENT"),
        ("nan", vec![MorphTarget::new().with_positions(vec![0.0, f32::NAN, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])], "value 1 is not finite"),
        ("inf", vec![ok(), MorphTarget::new().with_positions(vec![f32::INFINITY; 9])], "morph target 1"),
    ];
    for (name, targets, needle) in cases {
        let msg = err(b.attach_morph_targets(&mut prim, &targets));
        assert!(msg.contains(needle), "{name}: {msg:?} should contain {needle:?}");
        assert_eq!(counts(&b), before, "{name}: nothing may be written");
        assert!(prim.targets.is_none(), "{name}: primitive must be unchanged");
    }
}

#[test]
fn a_late_invalid_target_leaves_earlier_valid_targets_unwritten() {
    let mut b = GltfBuilder::new();
    let mut prim = base_primitive(&mut b);
    let before = counts(&b);
    let r = b.attach_morph_targets(&mut prim, &[MorphTarget::new().with_positions(D0.to_vec()), MorphTarget::new().with_positions(vec![0.0; 3])]);
    assert!(r.is_err());
    assert_eq!(counts(&b), before);
}

#[test]
fn normal_deltas_need_base_normals() {
    let mut b = GltfBuilder::new();
    let set = SkinSet { joints: JointIndices::U8(vec![[0; 4]; 3]), weights: vec![[1.0, 0.0, 0.0, 0.0]; 3] };
    let mut prim = b.create_skinned_primitive(&[0.0; 9], None, None, None, &[set], 1, None).unwrap();
    let before = counts(&b);
    let msg = err(b.attach_morph_targets(&mut prim, &[MorphTarget::new().with_normals(vec![0.0; 9])]));
    assert!(msg.contains("no base NORMAL"), "{msg}");
    assert_eq!(counts(&b), before);
}

#[test]
fn primitive_problems_are_reported() {
    let mut b = GltfBuilder::new();
    let t = [MorphTarget::new().with_positions(vec![0.0; 9])];

    let mut no_pos = Primitive::default();
    assert!(err(b.attach_morph_targets(&mut no_pos, &t)).contains("no POSITION"));

    let mut dangling = Primitive::default();
    dangling.attributes.insert("POSITION".into(), 99);
    assert!(err(b.attach_morph_targets(&mut dangling, &t)).contains("accessor 99 does not exist"));

    let mut prim = base_primitive(&mut b);
    b.attach_morph_targets(&mut prim, &t).unwrap();
    let before = counts(&b);
    assert!(err(b.attach_morph_targets(&mut prim, &t)).contains("already has morph targets"));
    assert_eq!(counts(&b), before);
}

#[test]
fn morph_target_builder_methods_set_exactly_their_field() {
    let m = MorphTarget::new();
    assert_eq!(m, MorphTarget::default());
    assert!(m.positions.is_none() && m.normals.is_none() && m.tangents.is_none());
    let m = MorphTarget::new().with_positions(vec![1.0]).with_normals(vec![2.0]).with_tangents(vec![3.0]);
    assert_eq!(
        (m.positions, m.normals, m.tangents),
        (Some(MorphDelta::Dense(vec![1.0])), Some(MorphDelta::Dense(vec![2.0])), Some(MorphDelta::Dense(vec![3.0])))
    );
}
