//! Sparse morph target deltas: decoded results must equal the dense encoding, read through the
//! independent `gltf` crate with hand-derived oracle values.

mod common;

use mesh_tools::{GltfBuilder, MorphDelta, MorphTarget, PrimitiveSpec};

// 5 vertices; sparse entries at 1 and 3.
const POS: [f32; 15] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0, 3.0, 0.0, 0.0, 4.0, 0.0, 0.0];
const NRM: [f32; 15] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
const TAN: [f32; 20] = [1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0];

const IDX: [u32; 2] = [1, 3];
const PV: [f32; 6] = [0.5, -2.0, 3.0, 1.0, 1.0, -1.0];
const NV: [f32; 6] = [0.0, 0.25, 0.0, 0.5, 0.0, 0.0];
const TV: [f32; 6] = [0.0, 0.0, 0.125, 0.0, 0.5, 0.0];

/// Expand sparse entries into the dense array (hand oracle, independent of the library).
fn counts(b: &GltfBuilder) -> (usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffers.as_ref().map_or(0, |a| a.first().map_or(0, |b| b.byte_length)),
    )
}

fn dense_of(values: &[f32; 6]) -> Vec<f32> {
    let mut d = vec![0.0; 15];
    d[3..6].copy_from_slice(&values[0..3]);
    d[9..12].copy_from_slice(&values[3..6]);
    d
}

fn primitive(b: &mut GltfBuilder) -> mesh_tools::Primitive {
    b.create_primitive(PrimitiveSpec {
        positions: POS.to_vec(),
        normals: Some(NRM.to_vec()),
        tangents: Some(TAN.to_vec()),
        indices: Some(mesh_tools::Indices::U8(vec![0, 1, 2, 2, 3, 4])),
        ..Default::default()
    })
    .unwrap()
}

fn build(target: MorphTarget) -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let mut p = primitive(&mut b);
    b.attach_morph_targets(&mut p, &[target]).unwrap();
    let m = b.add_morphable_mesh(None, vec![p], None, None).unwrap();
    let n = b.add_node(None, Some(m), None, None, None);
    b.add_scene(None, Some(vec![n]));
    b
}

type Decoded = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 3]>);

fn decode(b: &GltfBuilder) -> Decoded {
    assert!(b.validate().is_valid(), "{:?}", b.validate().errors);
    let glb = common::parse_glb(b);
    let prim = glb.document.meshes().next().unwrap().primitives().next().unwrap();
    let (p, n, t) = prim.reader(|_| glb.blob.as_deref()).read_morph_targets().next().unwrap();
    (p.unwrap().collect(), n.unwrap().collect(), t.unwrap().collect())
}

#[test]
fn sparse_decodes_to_zeros_with_values_at_the_indices() {
    let t = MorphTarget::new()
        .with_sparse_positions(IDX.to_vec(), PV.to_vec())
        .with_sparse_normals(IDX.to_vec(), NV.to_vec())
        .with_sparse_tangents(IDX.to_vec(), TV.to_vec());
    let (p, n, tg) = decode(&build(t));
    let z = [0.0; 3];
    assert_eq!(p, vec![z, [0.5, -2.0, 3.0], z, [1.0, 1.0, -1.0], z]);
    assert_eq!(n, vec![z, [0.0, 0.25, 0.0], z, [0.5, 0.0, 0.0], z]);
    assert_eq!(tg, vec![z, [0.0, 0.0, 0.125], z, [0.0, 0.5, 0.0], z]);
}

#[test]
fn sparse_equals_dense() {
    let sparse = MorphTarget::new()
        .with_sparse_positions(IDX.to_vec(), PV.to_vec())
        .with_sparse_normals(IDX.to_vec(), NV.to_vec())
        .with_sparse_tangents(IDX.to_vec(), TV.to_vec());
    let dense = MorphTarget::new()
        .with_positions(dense_of(&PV))
        .with_normals(dense_of(&NV))
        .with_tangents(dense_of(&TV));
    assert_eq!(decode(&build(sparse)), decode(&build(dense)));
}

#[test]
fn sparse_accessor_layout_and_position_bounds_of_the_dense_result() {
    let b = build(MorphTarget::new().with_sparse_positions(IDX.to_vec(), PV.to_vec()).with_sparse_normals(IDX.to_vec(), NV.to_vec()));
    let prim = &b.gltf.meshes.as_ref().unwrap()[0].primitives[0];
    let accs = b.gltf.accessors.as_ref().unwrap();
    let pa = &accs[prim.targets.as_ref().unwrap()[0]["POSITION"]];
    assert!(pa.buffer_view.is_none(), "zero base: no bufferView");
    let s = pa.sparse.as_ref().unwrap();
    assert_eq!((pa.count, s.count, s.indices.component_type), (5, 2, 5121));
    // Dense result has zeros at 0, 2, 4, so min/max include 0:
    // x in {0, 0.5, 1}, y in {0, -2, 1}, z in {0, 3, -1}.
    assert_eq!(pa.min, Some(vec![0.0, -2.0, -1.0]));
    assert_eq!(pa.max, Some(vec![1.0, 1.0, 3.0]));
    let na = &accs[prim.targets.as_ref().unwrap()[0]["NORMAL"]];
    assert!(na.min.is_none() && na.sparse.is_some());
}

#[test]
fn sparse_bounds_when_every_vertex_is_replaced() {
    let mut b = GltfBuilder::new();
    let mut p = primitive(&mut b);
    let vals: Vec<f32> = (1..=15).map(|v| v as f32).collect();
    let t = MorphTarget::new().with_sparse_positions(vec![0, 1, 2, 3, 4], vals);
    b.attach_morph_targets(&mut p, &[t]).unwrap();
    let a = &b.gltf.accessors.as_ref().unwrap()[p.targets.as_ref().unwrap()[0]["POSITION"]];
    // No implicit zero element: min is the first element, max the last.
    assert_eq!((a.min.clone(), a.max.clone()), (Some(vec![1.0, 2.0, 3.0]), Some(vec![13.0, 14.0, 15.0])));
}

#[test]
fn invalid_sparse_input_is_rejected_atomically() {
    let mut b = GltfBuilder::new();
    let mut p = primitive(&mut b);
    let before = counts(&b);
    let sp = |i: Vec<u32>, v: Vec<f32>| MorphTarget::new().with_sparse_positions(i, v);
    let cases: Vec<(&str, MorphTarget, &str)> = vec![
        ("empty", sp(vec![], vec![]), "at least one entry"),
        ("unsorted", sp(vec![3, 1], PV.to_vec()), "strictly increasing"),
        ("duplicate", sp(vec![1, 1], PV.to_vec()), "strictly increasing"),
        ("out of range", sp(vec![1, 5], PV.to_vec()), "index 5 is out of range for 5 vertices"),
        ("value count", sp(vec![1, 3], vec![0.0; 5]), "expected 6"),
        ("non finite", sp(vec![1, 3], vec![0.0, f32::INFINITY, 0.0, 0.0, 0.0, 0.0]), "value 1 is not finite"),
    ];
    for (name, t, needle) in cases {
        // A valid dense attribute before the bad one must not leak out either.
        let t = t.with_normals(vec![0.0; 15]);
        let msg = b.attach_morph_targets(&mut p, &[t]).unwrap_err().to_string();
        assert!(msg.contains(needle), "{name}: {msg}");
        assert_eq!(counts(&b), before, "{name}");
        assert!(p.targets.is_none());
    }
}

#[test]
fn from_absolute_computes_morphed_minus_base() {
    let base = [1.0, 2.0, 3.0, -1.0, 0.0, 0.5];
    let morphed = [1.5, 2.0, 1.0, -1.0, 0.25, 0.5];
    assert_eq!(MorphDelta::from_absolute(&base, &morphed).unwrap(), vec![0.5, 0.0, -2.0, 0.0, 0.25, 0.0]);
    let e = MorphDelta::from_absolute(&base, &morphed[..3]).unwrap_err().to_string();
    assert!(e.contains("base has 6 floats but morphed has 3"), "{e}");
    assert!(MorphDelta::from_absolute(&[1.0; 4], &[1.0; 4]).is_err(), "not a multiple of 3");
    assert!(MorphDelta::from_absolute(&[f32::NAN; 3], &[0.0; 3]).is_err());
    assert!(MorphDelta::from_absolute(&[-f32::MAX; 3], &[f32::MAX; 3]).is_err(), "overflowing delta");
}

#[test]
fn sparse_from_dense_keeps_nonzero_elements_exactly() {
    let dense = dense_of(&PV);
    let d = MorphDelta::sparse_from_dense(&dense, None).unwrap();
    assert_eq!(d, MorphDelta::Sparse { indices: vec![1, 3], values: PV.to_vec() });
    // The whole vector counts as one element: a single non-zero component keeps it.
    let d = MorphDelta::sparse_from_dense(&[0.0, 0.0, 0.0, 0.0, 1e-9, 0.0], None).unwrap();
    assert_eq!(d, MorphDelta::Sparse { indices: vec![1], values: vec![0.0, 1e-9, 0.0] });
}

#[test]
fn sparse_from_dense_epsilon_is_an_explicit_lossy_threshold() {
    let dense = [0.0, 0.0, 0.0, 0.01, -0.01, 0.0, 0.02, 0.0, 0.0, 0.0, 0.0, 0.5];
    // |component| <= epsilon is dropped: element 1 (max 0.01) goes with epsilon 0.01, element 2 (0.02) stays.
    let kept = |eps| MorphDelta::sparse_from_dense(&dense, eps);
    assert_eq!(
        kept(Some(0.01)).unwrap(),
        MorphDelta::Sparse { indices: vec![2, 3], values: vec![0.02, 0.0, 0.0, 0.0, 0.0, 0.5] }
    );
    assert_eq!(
        kept(Some(0.0099)).unwrap(),
        MorphDelta::Sparse { indices: vec![1, 2, 3], values: dense[3..].to_vec() }
    );
    // Without epsilon nothing but exact zeros is dropped.
    assert_eq!(kept(None).unwrap(), MorphDelta::Sparse { indices: vec![1, 2, 3], values: dense[3..].to_vec() });
}

#[test]
fn sparse_from_dense_errors() {
    let e = |r: mesh_tools::Result<MorphDelta>| r.unwrap_err().to_string();
    assert!(e(MorphDelta::sparse_from_dense(&[0.0; 6], None)).contains("all deltas are zero"));
    assert!(e(MorphDelta::sparse_from_dense(&[0.1; 6], Some(0.5))).contains("all deltas are zero"));
    assert!(e(MorphDelta::sparse_from_dense(&[0.0; 4], None)).contains("multiple of 3"));
    assert!(e(MorphDelta::sparse_from_dense(&[1.0; 3], Some(-1.0))).contains("epsilon"));
    assert!(e(MorphDelta::sparse_from_dense(&[1.0; 3], Some(f32::NAN))).contains("epsilon"));
    assert!(e(MorphDelta::sparse_from_dense(&[f32::NAN; 3], None)).contains("not finite"));
}

#[test]
fn target_from_sparse_from_dense_round_trips() {
    let delta = MorphDelta::sparse_from_dense(&dense_of(&PV), None).unwrap();
    let (p, ..) = decode(&build(MorphTarget::new().with_position_delta(delta).with_normals(vec![0.0; 15]).with_tangents(vec![0.0; 15])));
    let z = [0.0; 3];
    assert_eq!(p, vec![z, [0.5, -2.0, 3.0], z, [1.0, 1.0, -1.0], z]);
}
