//! Normalized u8/u16 `WEIGHTS_n`: quantization oracles, wire format, mixed sets, failure paths.
mod common;

use gltf::accessor::DataType;
use gltf::mesh::util::ReadWeights;
use gltf::Semantic;
use mesh_tools::{quantize_u16, quantize_u8, GltfBuilder, GltfError, JointIndices, SkinSet, SkinWeights};

const TRI: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

fn mesh_with(b: &mut GltfBuilder, sets: &[SkinSet]) -> mesh_tools::Result<usize> {
    let prim = b.create_skinned_primitive(&TRI, Some(&[0, 1, 2]), None, None, sets, 4, None)?;
    Ok(b.add_mesh(None, vec![prim]))
}

fn counts(b: &GltfBuilder) -> (usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, Vec::len),
        b.gltf.buffer_views.as_ref().map_or(0, Vec::len),
        b.gltf.meshes.as_ref().map_or(0, Vec::len),
    )
}

// Hand-derived oracles.

#[test]
fn quantize_u8_oracles() {
    // 200/255 and 55/255 are exactly representable by construction.
    let q = quantize_u8(&[[200.0 / 255.0, 55.0 / 255.0, 0.0, 0.0]]).unwrap();
    assert_eq!(q, vec![[200, 55, 0, 0]]);
    // Thirds: 255/3 = 85 exactly.
    assert_eq!(quantize_u8(&[[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 0.0]]).unwrap(), vec![[85, 85, 85, 0]]);
    // 127.5 each: floors 127,127, one unit left, tie goes to the lower slot.
    assert_eq!(quantize_u8(&[[0.5, 0.5, 0.0, 0.0]]).unwrap(), vec![[128, 127, 0, 0]]);
    // Largest remainder, exact in binary: ideals 127.5, 63.75, 31.875, 31.875 floor to 252,
    // so the 3 spare units go to the largest fractions .875, .875 (lower slot first), .75.
    assert_eq!(quantize_u8(&[[0.5, 0.25, 0.125, 0.125]]).unwrap(), vec![[127, 64, 32, 32]]);
}

#[test]
fn quantize_u16_thirds_are_exact() {
    assert_eq!(quantize_u16(&[[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 0.0]]).unwrap(), vec![[21845, 21845, 21845, 0]]);
}

#[test]
fn quantized_sums_are_exact_for_many_inputs() {
    let mut state = 12345u32;
    let mut next = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (state >> 8) as f32 / (1u32 << 24) as f32
    };
    let mut weights = Vec::new();
    for _ in 0..500 {
        let raw = [next() + 0.01, next(), next() * 0.5, next()];
        let s: f32 = raw.iter().sum();
        weights.push(raw.map(|x| x / s));
    }
    for q in quantize_u8(&weights).unwrap() {
        assert_eq!(q.iter().map(|&x| u32::from(x)).sum::<u32>(), 255);
    }
    for q in quantize_u16(&weights).unwrap() {
        assert_eq!(q.iter().map(|&x| u32::from(x)).sum::<u32>(), 65535);
    }
}

#[test]
fn quantize_rejects_bad_input_naming_the_vertex() {
    for bad in [[-0.1, 1.1, 0.0, 0.0], [f32::NAN, 1.0, 0.0, 0.0], [0.0; 4], [0.5, 0.5, 0.5, 0.0]] {
        let e = quantize_u8(&[[1.0, 0.0, 0.0, 0.0], bad]).unwrap_err().to_string();
        assert!(e.contains("vertex 1"), "{e}");
        assert!(quantize_u16(&[bad]).is_err());
    }
}

// Wire format, read back through the independent reader.

#[test]
fn u8_weights_are_written_normalized_and_read_back() {
    let mut b = GltfBuilder::new();
    let weights = vec![[200, 55, 0, 0], [255, 0, 0, 0], [85, 85, 85, 0]];
    let set = SkinSet::new(JointIndices::U8(vec![[0, 1, 0, 0], [0; 4], [0, 1, 2, 0]]), weights.clone());
    let mesh = mesh_with(&mut b, &[set]).unwrap();
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));

    let glb = common::parse_glb(&b);
    let prim = glb.document.meshes().next().unwrap().primitives().next().unwrap();
    let acc = prim.get(&Semantic::Weights(0)).unwrap();
    assert_eq!(acc.data_type(), DataType::U8);
    assert!(acc.normalized());
    let reader = prim.reader(|_| glb.blob.as_deref());
    match reader.read_weights(0).unwrap() {
        ReadWeights::U8(it) => assert_eq!(it.collect::<Vec<_>>(), weights),
        _ => panic!("expected U8 weights"),
    }
    // Normalizing reader view: 200/255.
    let f: Vec<[f32; 4]> = reader.read_weights(0).unwrap().into_f32().collect();
    assert!((f[0][0] - 200.0 / 255.0).abs() < 1e-7);
    assert!(b.validate().is_valid(), "{:?}", b.validate());
}

#[test]
fn u16_thirds_and_mixed_sets() {
    let mut b = GltfBuilder::new();
    // Set 0 u16 thirds, set 1 f32 zero: vertex sums are 1 overall.
    let s0 = SkinSet::new(JointIndices::U8(vec![[0, 1, 2, 0]; 3]), vec![[21845u16, 21845, 21845, 0]; 3]);
    let s1 = SkinSet::new(JointIndices::U8(vec![[0; 4]; 3]), vec![[0.0f32; 4]; 3]);
    // Set 2 u8 with zero weights.
    let s2 = SkinSet::new(JointIndices::U16(vec![[0; 4]; 3]), vec![[0u8; 4]; 3]);
    let mesh = mesh_with(&mut b, &[s0, s1, s2]).unwrap();
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));

    let glb = common::parse_glb(&b);
    let prim = glb.document.meshes().next().unwrap().primitives().next().unwrap();
    assert_eq!(prim.get(&Semantic::Weights(0)).unwrap().data_type(), DataType::U16);
    assert_eq!(prim.get(&Semantic::Weights(1)).unwrap().data_type(), DataType::F32);
    assert_eq!(prim.get(&Semantic::Weights(2)).unwrap().data_type(), DataType::U8);
    let reader = prim.reader(|_| glb.blob.as_deref());
    match reader.read_weights(0).unwrap() {
        ReadWeights::U16(mut it) => assert!(it.all(|w| w == [21845, 21845, 21845, 0])),
        _ => panic!("expected U16 weights"),
    }
    assert!(b.validate().is_valid(), "{:?}", b.validate());
}

#[test]
fn integer_sets_split_across_u8_and_u16_sum_exactly() {
    // u8 128 = 32896/65535 plus u16 32639 = 65535 - 32896: exactly 1 in 1/65535 units.
    let mut b = GltfBuilder::new();
    let s0 = SkinSet::new(JointIndices::U8(vec![[0, 0, 0, 0]; 3]), vec![[128u8, 0, 0, 0]; 3]);
    let s1 = SkinSet::new(JointIndices::U8(vec![[1, 0, 0, 0]; 3]), vec![[32639u16, 0, 0, 0]; 3]);
    mesh_with(&mut b, &[s0, s1]).unwrap();
}

#[test]
fn integer_sum_off_by_one_is_rejected_atomically() {
    let mut b = GltfBuilder::new();
    let before = counts(&b);
    let bad = SkinSet::new(JointIndices::U8(vec![[0, 1, 0, 0]; 3]), vec![[200u8, 54, 0, 0]; 3]);
    let e = mesh_with(&mut b, &[bad]).unwrap_err();
    assert!(matches!(&e, GltfError::InvalidInput(m) if m.contains("vertex 0") && m.contains("255")), "{e}");
    assert_eq!(counts(&b), before);
}

#[test]
fn joints_rules_apply_to_integer_weights() {
    let mut b = GltfBuilder::new();
    // Duplicate non-zero joint on one vertex.
    let dup = SkinSet::new(JointIndices::U8(vec![[1, 1, 0, 0]; 3]), vec![[128u8, 127, 0, 0]; 3]);
    assert!(mesh_with(&mut b, &[dup]).is_err());
    // A zero-weight slot may repeat/point anywhere in range, but a non-zero one beyond joint_count fails.
    let oor = SkinSet::new(JointIndices::U8(vec![[9, 0, 0, 0]; 3]), vec![[255u8, 0, 0, 0]; 3]);
    assert!(mesh_with(&mut b, &[oor]).is_err());
    let ok = SkinSet::new(JointIndices::U8(vec![[1, 9, 9, 9]; 3]), vec![[255u8, 0, 0, 0]; 3]);
    mesh_with(&mut b, &[ok]).unwrap();
}

#[test]
fn weights_length_mismatch_is_rejected() {
    let mut b = GltfBuilder::new();
    let set = SkinSet::new(JointIndices::U8(vec![[0; 4]; 3]), vec![[255u8, 0, 0, 0]; 2]);
    assert!(mesh_with(&mut b, &[set]).is_err());
}

#[test]
fn f32_conversion_keeps_the_variant() {
    let s = SkinSet::new(JointIndices::U8(vec![[0; 4]]), vec![[1.0f32, 0.0, 0.0, 0.0]]);
    assert!(matches!(s.weights, SkinWeights::F32(_)));
    let s = SkinSet::new(JointIndices::U8(vec![[0; 4]]), vec![[255u8, 0, 0, 0]]);
    assert!(matches!(s.weights, SkinWeights::U8(_)));
}

#[test]
fn mesh_builder_with_skin_sets() {
    use mesh_tools::mesh::MeshBuilder;
    let mut b = GltfBuilder::new();
    let set = SkinSet::new(JointIndices::U8(vec![[0, 1, 0, 0]; 3]), vec![[200u8, 55, 0, 0]; 3]);
    let idx = MeshBuilder::new(None).with_positions(TRI.to_vec()).with_indices(vec![0, 1, 2])
        .with_skin_sets(vec![set])
        .with_joint_count(2)
        .build_into(&mut b)
        .unwrap();
    let prim = &b.gltf.meshes.as_ref().unwrap()[idx].primitives[0];
    assert!(prim.attributes.contains_key("WEIGHTS_0"));
}

#[test]
fn mesh_builder_rejects_skin_sets_combined_with_joints() {
    use mesh_tools::mesh::MeshBuilder;
    let mut b = GltfBuilder::new();
    let set = SkinSet::new(JointIndices::U8(vec![[0; 4]; 3]), vec![[255u8, 0, 0, 0]; 3]);
    let before = counts(&b);
    let r = MeshBuilder::new(None).with_positions(TRI.to_vec()).with_indices(vec![0, 1, 2])
        .with_skin_sets(vec![set])
        .with_joints(vec![[0; 4]; 3])
        .with_weights(vec![[1.0, 0.0, 0.0, 0.0]; 3])
        .build_into(&mut b);
    assert!(r.is_err());
    assert_eq!(counts(&b), before);
}

// Golden bytes for f32 skinned output live in tests/accessor_goldens.rs (skin_and_ibm,
// mesh_builder_skinned_morphed) and must not change.
