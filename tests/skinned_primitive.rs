mod common;

use gltf::accessor::DataType;
use gltf::mesh::util::ReadJoints;
use gltf::Semantic;
use mesh_tools::{limit_influences, normalize_weights, GltfBuilder, GltfError, JointIndices, SkinSet};

/// A quad: 4 vertices, 2 triangles, with vertex 0..1 on joint 0 and 2..3 split over joints 0/1.
fn quad() -> (Vec<f32>, Vec<u32>, SkinSet) {
    let positions = vec![-1.0, 0.0, 0.5, 1.0, 0.0, -2.0, 1.0, 3.0, 0.0, -1.0, 3.0, 0.0];
    let indices = vec![0, 1, 2, 0, 2, 3];
    let set = SkinSet {
        joints: JointIndices::U8(vec![[0, 0, 0, 0], [0, 0, 0, 0], [0, 1, 0, 0], [1, 0, 0, 0]]),
        weights: vec![
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.5, 0.5, 0.0, 0.0],
            [0.75, 0.25, 0.0, 0.0],
        ],
    };
    (positions, indices, set)
}

fn counts(b: &GltfBuilder) -> (usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.gltf.meshes.as_ref().map_or(0, |a| a.len()),
    )
}

fn invalid_input(r: mesh_tools::Result<mesh_tools::Primitive>) -> String {
    match r {
        Err(GltfError::InvalidInput(m)) => m,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[test]
fn u8_joints_round_trip_with_correct_component_types() {
    let mut b = GltfBuilder::new();
    let (pos, idx, set) = quad();
    let prim = b.create_skinned_primitive(&pos, Some(&idx), None, None, &[set.clone()], 2, None).unwrap();
    let mesh = b.add_mesh(None, vec![prim]);
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));

    let glb = common::parse_glb(&b);
    let prim = glb.document.meshes().next().unwrap().primitives().next().unwrap();
    let reader = prim.reader(|_| glb.blob.as_deref());

    assert_eq!(prim.get(&Semantic::Joints(0)).unwrap().data_type(), DataType::U8);
    assert_eq!(prim.get(&Semantic::Weights(0)).unwrap().data_type(), DataType::F32);
    assert!(!prim.get(&Semantic::Weights(0)).unwrap().normalized());

    match reader.read_joints(0).unwrap() {
        ReadJoints::U8(it) => {
            let got: Vec<[u8; 4]> = it.collect();
            let JointIndices::U8(want) = &set.joints else { unreachable!() };
            assert_eq!(&got, want);
        }
        _ => panic!("expected U8 joints"),
    }
    let weights: Vec<[f32; 4]> = reader.read_weights(0).unwrap().into_f32().collect();
    assert_eq!(weights, set.weights);
    assert_eq!(reader.read_indices().unwrap().into_u32().collect::<Vec<_>>(), idx);
}

#[test]
fn u16_joints_and_multiple_sets_are_written() {
    let mut b = GltfBuilder::new();
    let (pos, idx, _) = quad();
    let set0 = SkinSet {
        joints: JointIndices::U16(vec![[300, 1, 2, 3]; 4]),
        weights: vec![[0.25, 0.25, 0.125, 0.125]; 4],
    };
    let set1 = SkinSet {
        joints: JointIndices::U16(vec![[4, 5, 6, 7]; 4]),
        weights: vec![[0.125, 0.0625, 0.0625, 0.0]; 4],
    };
    let prim = b.create_skinned_primitive(&pos, Some(&idx), None, None, &[set0, set1], 301, None).unwrap();
    b.add_mesh(None, vec![prim]);

    let glb = common::parse_glb(&b);
    let prim = glb.document.meshes().next().unwrap().primitives().next().unwrap();
    for n in 0..2 {
        assert!(prim.get(&Semantic::Joints(n)).is_some(), "JOINTS_{n}");
        assert!(prim.get(&Semantic::Weights(n)).is_some(), "WEIGHTS_{n}");
    }
    assert_eq!(prim.get(&Semantic::Joints(0)).unwrap().data_type(), DataType::U16);
    let reader = prim.reader(|_| glb.blob.as_deref());
    match reader.read_joints(0).unwrap() {
        ReadJoints::U16(mut it) => assert_eq!(it.next().unwrap(), [300, 1, 2, 3]),
        _ => panic!("expected U16 joints"),
    }
}

#[test]
fn position_accessor_has_bounds() {
    let mut b = GltfBuilder::new();
    let (pos, idx, set) = quad();
    let prim = b.create_skinned_primitive(&pos, Some(&idx), None, None, &[set], 2, None).unwrap();
    b.add_mesh(None, vec![prim]);
    let glb = common::parse_glb(&b);
    let prim = glb.document.meshes().next().unwrap().primitives().next().unwrap();
    let acc = prim.get(&Semantic::Positions).unwrap();
    let as_vec = |v: gltf::json::Value| -> Vec<f64> { v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() };
    assert_eq!(as_vec(acc.min().unwrap()), vec![-1.0, 0.0, -2.0]);
    assert_eq!(as_vec(acc.max().unwrap()), vec![1.0, 3.0, 0.5]);
}

#[test]
fn index_width_avoids_the_restart_value() {
    let (_, _, set) = quad();
    let big = 70_000;
    let mut positions = vec![0.0f32; big * 3];
    positions[0] = 1.0;
    let set_big = SkinSet {
        joints: JointIndices::U8(vec![[0; 4]; big]),
        weights: vec![[1.0, 0.0, 0.0, 0.0]; big],
    };
    let _ = set;

    let index_type = |max: u32| {
        let mut b = GltfBuilder::new();
        let prim = b
            .create_skinned_primitive(&positions, Some(&[0, 1, max]), None, None, &[set_big.clone()], 1, None)
            .unwrap();
        b.add_mesh(None, vec![prim]);
        let glb = common::parse_glb(&b);
        let p = glb.document.meshes().next().unwrap().primitives().next().unwrap();
        p.indices().unwrap().data_type()
    };
    assert_eq!(index_type(65_534), DataType::U16);
    assert_eq!(index_type(65_535), DataType::U32, "65535 is the restart value, must widen");
    assert_eq!(index_type(69_999), DataType::U32);
}

#[test]
fn non_indexed_geometry_is_supported() {
    let mut b = GltfBuilder::new();
    let positions = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let set = SkinSet { joints: JointIndices::U8(vec![[0; 4]; 3]), weights: vec![[1.0, 0.0, 0.0, 0.0]; 3] };
    let prim = b.create_skinned_primitive(&positions, None, None, None, &[set], 1, None).unwrap();
    assert!(prim.indices.is_none());
}

#[test]
fn rejects_invalid_input_with_actionable_messages_and_writes_nothing() {
    let (pos, idx, set) = quad();
    let mut b = GltfBuilder::new();
    let before = counts(&b);
    let mut run = |name: &str, f: &dyn Fn(&mut GltfBuilder) -> mesh_tools::Result<mesh_tools::Primitive>, needle: &str| {
        let msg = invalid_input(f(&mut b));
        assert!(msg.contains(needle), "{name}: message {msg:?} should contain {needle:?}");
    };

    run("partial vertex", &|b| b.create_skinned_primitive(&pos[..11], None, None, None, &[set.clone()], 2, None), "multiple of 3");
    run("index out of range", &|b| b.create_skinned_primitive(&pos, Some(&[0, 1, 4]), None, None, &[set.clone()], 2, None), "index 2 is 4");
    run("index count", &|b| b.create_skinned_primitive(&pos, Some(&idx[..5]), None, None, &[set.clone()], 2, None), "multiple of 3");
    run("normals len", &|b| b.create_skinned_primitive(&pos, Some(&idx), Some(&pos[..9]), None, &[set.clone()], 2, None), "normals");
    run("texcoords len", &|b| b.create_skinned_primitive(&pos, Some(&idx), None, Some(&[0.0; 4]), &[set.clone()], 2, None), "texcoord set 0");
    run("no sets", &|b| b.create_skinned_primitive(&pos, Some(&idx), None, None, &[], 2, None), "at least one skin set");
    run("zero joint_count", &|b| b.create_skinned_primitive(&pos, Some(&idx), None, None, &[set.clone()], 0, None), "joint_count");
    run("nan position", &|b| {
        let mut p = pos.clone();
        p[4] = f32::NAN;
        b.create_skinned_primitive(&p, Some(&idx), None, None, &[set.clone()], 2, None)
    }, "not finite");

    let with_weights = |w: Vec<[f32; 4]>| SkinSet { weights: w, ..set.clone() };
    let mut bad = set.weights.clone();
    bad[2] = [0.5, 0.4, 0.0, 0.0];
    run("sum != 1", &|b| b.create_skinned_primitive(&pos, Some(&idx), None, None, &[with_weights(bad.clone())], 2, None), "vertex 2: weights sum to");
    let mut neg = set.weights.clone();
    neg[1] = [1.5, -0.5, 0.0, 0.0];
    run("negative", &|b| b.create_skinned_primitive(&pos, Some(&idx), None, None, &[with_weights(neg.clone())], 2, None), "vertex 1");
    let mut nan = set.weights.clone();
    nan[0][0] = f32::NAN;
    run("nan weight", &|b| b.create_skinned_primitive(&pos, Some(&idx), None, None, &[with_weights(nan.clone())], 2, None), "vertex 0");
    run("short sets", &|b| b.create_skinned_primitive(&pos, Some(&idx), None, None, &[with_weights(set.weights[..3].to_vec())], 2, None), "skin set 0");

    // Joint slot out of range (the classic "passed a node index" mistake).
    run("joint out of range", &|b| b.create_skinned_primitive(&pos, Some(&idx), None, None, &[set.clone()], 1, None), "out of range");
    let dup = SkinSet {
        joints: JointIndices::U8(vec![[0, 0, 0, 0]; 4]),
        weights: vec![[0.5, 0.5, 0.0, 0.0]; 4],
    };
    run("duplicate joint", &|b| b.create_skinned_primitive(&pos, Some(&idx), None, None, &[dup.clone()], 2, None), "more than once");

    assert_eq!(counts(&b), before, "failed calls must not write accessors, views or meshes");
    assert!(b.to_glb_bytes().unwrap().len() < 1024, "and no binary data");
}

#[test]
fn zero_weight_slots_may_hold_any_joint_value() {
    // Unused slots commonly carry joint 0 with weight 0; an out-of-range value there is harmless.
    let mut b = GltfBuilder::new();
    let positions = vec![0.0; 9];
    let set = SkinSet { joints: JointIndices::U8(vec![[0, 200, 200, 200]; 3]), weights: vec![[1.0, 0.0, 0.0, 0.0]; 3] };
    assert!(b.create_skinned_primitive(&positions, None, None, None, &[set], 1, None).is_ok());
}

#[test]
fn create_skinned_mesh_wrapper_validates_too() {
    let mut b = GltfBuilder::new();
    let (pos, _, set) = quad();
    let JointIndices::U8(j8) = &set.joints else { unreachable!() };
    let joints: Vec<[u16; 4]> = j8.iter().map(|j| j.map(u16::from)).collect();
    let mesh = b.create_skinned_mesh(None, &pos, &[0, 1, 2, 0, 2, 3], None, None, &joints, &set.weights, None).unwrap();
    assert_eq!(mesh, 0);

    let mut bad = set.weights.clone();
    bad[0] = [2.0, 0.0, 0.0, 0.0];
    let before = counts(&b);
    assert!(b.create_skinned_mesh(None, &pos, &[0, 1, 2], None, None, &joints, &bad, None).is_err());
    assert_eq!(counts(&b), before);
}

#[test]
fn normalize_weights_fixes_sums_and_rejects_the_unfixable() {
    let mut w = vec![[1.0, 1.0, 0.0, 0.0], [0.2, 0.2, 0.2, 0.2]];
    normalize_weights(&mut w).unwrap();
    assert_eq!(w[0], [0.5, 0.5, 0.0, 0.0]);
    assert!((w[1].iter().sum::<f32>() - 1.0).abs() < 1e-6);

    let mut zero = vec![[1.0, 0.0, 0.0, 0.0], [0.0; 4]];
    let msg = match normalize_weights(&mut zero) { Err(GltfError::InvalidInput(m)) => m, o => panic!("{o:?}") };
    assert!(msg.contains("vertex 1"));
    assert_eq!(zero[0], [1.0, 0.0, 0.0, 0.0], "no partial modification on error");
    assert!(normalize_weights(&mut [[-1.0, 2.0, 0.0, 0.0]]).is_err());
    assert!(normalize_weights(&mut [[f32::NAN, 1.0, 0.0, 0.0]]).is_err());
}

#[test]
fn limit_influences_keeps_top_four_and_renormalizes() {
    let (j, w) = limit_influences(&[
        vec![(7, 0.1), (3, 0.4), (9, 0.2), (1, 0.2), (5, 0.1)],
        vec![(2, 1.0)],
    ])
    .unwrap();
    // Sorted descending, ties by lower joint; the lowest (7 or 5 at 0.1) is dropped.
    assert_eq!(j[0], [3, 1, 9, 5]);
    assert!((w[0].iter().sum::<f32>() - 1.0).abs() < 1e-6);
    assert!((w[0][0] - 0.4 / 0.9).abs() < 1e-6);
    assert_eq!(j[1], [2, 0, 0, 0]);
    assert_eq!(w[1], [1.0, 0.0, 0.0, 0.0]);

    assert!(limit_influences(&[vec![]]).is_err());
    assert!(limit_influences(&[vec![(1, 0.0)]]).is_err());
    assert!(limit_influences(&[vec![(1, -1.0)]]).is_err());
}

#[test]
fn output_of_limit_and_normalize_passes_primitive_validation() {
    let mut b = GltfBuilder::new();
    let positions = vec![0.0; 9];
    let (j, mut w) = limit_influences(&vec![vec![(0, 3.0), (1, 1.0), (2, 1.0), (3, 1.0), (4, 1.0)]; 3]).unwrap();
    normalize_weights(&mut w).unwrap();
    let set = SkinSet { joints: JointIndices::U16(j), weights: w };
    assert!(b.create_skinned_primitive(&positions, None, None, None, &[set], 5, None).is_ok());
}

#[test]
fn weight_sum_tolerance_is_1e_minus_5() {
    let positions = vec![0.0; 9];
    let joints = JointIndices::U8(vec![[0, 1, 0, 0]; 3]);
    let build = |sum_extra: f32| {
        let mut b = GltfBuilder::new();
        let set = SkinSet { joints: joints.clone(), weights: vec![[0.5, 0.5 + sum_extra, 0.0, 0.0]; 3] };
        b.create_skinned_primitive(&positions, None, None, None, &[set], 2, None)
    };
    assert!(build(5e-6).is_ok(), "sum 1 + 5e-6 is inside the tolerance");
    assert!(build(5e-5).is_err(), "sum 1 + 5e-5 is outside the tolerance");
}
