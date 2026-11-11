//! Sparse accessors: the substituted array read back through the independent `gltf` crate,
//! all three index widths, the zero-base variant, atomicity and the `ACCESSOR_SPARSE_*` codes.

mod common;

use gltf::accessor::{DataType, Iter};
use mesh_tools::{AccessorSpec, AccessorType as T, Bounds, GltfBuilder, GltfError, SparseSpec};

fn invalid<V: std::fmt::Debug>(r: mesh_tools::Result<V>) -> String {
    match r {
        Err(GltfError::InvalidInput(m)) => m,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

fn state(b: &GltfBuilder) -> (usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffers.as_ref().map_or(0, |a| a.first().map_or(0, |b| b.byte_length)),
    )
}

#[track_caller]
fn assert_error(b: &GltfBuilder, code: &str) {
    let r = b.validate();
    assert!(r.has_error(code), "expected error {code}, got: {:#?}", r.errors);
}

fn bin_chunk(bytes: &[u8]) -> Vec<u8> {
    let jl = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let at = 20 + jl;
    let bl = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    bytes[at + 8..at + 8 + bl].to_vec()
}

/// 5 x VEC3 base (0..15) with elements 1 and 3 replaced.
const BASE: [f32; 15] = [0., 1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12., 13., 14.];
const SUBST: [f32; 6] = [-1., -2., -3., 100., 200., 300.];

fn five_vec3() -> (GltfBuilder, usize) {
    let mut b = GltfBuilder::new();
    let spec = AccessorSpec::new().with_bounds(Bounds::Compute);
    let a = b.add_sparse_accessor(5, T::Vec3, Some(&BASE[..]), SparseSpec::new(vec![1, 3], SUBST.to_vec()), spec).unwrap();
    (b, a)
}

#[test]
fn base_with_substitution_reads_back_substituted() {
    let (b, a) = five_vec3();
    assert!(b.validate().is_valid(), "{:?}", b.validate().errors);
    let bytes = b.to_glb_bytes().unwrap();
    let glb = gltf::Gltf::from_slice(&bytes).unwrap();
    let blob = bin_chunk(&bytes);
    let acc = glb.document.accessors().nth(a).unwrap();
    assert_eq!(acc.count(), 5);
    let got: Vec<[f32; 3]> = Iter::new(acc.clone(), |_| Some(&blob[..])).unwrap().collect();
    assert_eq!(got, vec![[0., 1., 2.], [-1., -2., -3.], [6., 7., 8.], [100., 200., 300.], [12., 13., 14.]]);
    // Bounds describe the substituted array: x in -1..100, y in -2..200, z in -3..300.
    assert_eq!(acc.min().unwrap().as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>(), [-1., -2., -3.]);
    assert_eq!(acc.max().unwrap().as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>(), [100., 200., 300.]);
    let sp = acc.sparse().unwrap();
    assert_eq!(sp.count(), 2);
    assert!(matches!(sp.indices().index_type(), gltf::accessor::sparse::IndexType::U8));
}

/// Substituted result of a SCALAR sparse accessor of `count` elements via the gltf reader.
fn scalar_sparse(count: usize, indices: Vec<u32>, values: Vec<u16>) -> (gltf::accessor::sparse::IndexType, Vec<u16>) {
    let mut b = GltfBuilder::new();
    let a = b.add_sparse_accessor::<u16>(count, T::Scalar, None, SparseSpec::new(indices, values), AccessorSpec::new()).unwrap();
    assert!(b.validate().is_valid(), "{:?}", b.validate().errors);
    let bytes = b.to_glb_bytes().unwrap();
    let glb = gltf::Gltf::from_slice(&bytes).unwrap();
    let blob = bin_chunk(&bytes);
    let acc = glb.document.accessors().nth(a).unwrap();
    let ty = acc.sparse().unwrap().indices().index_type();
    (ty, Iter::<u16>::new(acc, |_| Some(&blob[..])).unwrap().collect())
}

#[test]
fn index_width_follows_largest_index() {
    use gltf::accessor::sparse::IndexType::*;
    fn name(t: gltf::accessor::sparse::IndexType) -> &'static str {
        match t { U8 => "U8", U16 => "U16", U32 => "U32" }
    }
    let (ty, got) = scalar_sparse(256, vec![0, 255], vec![7, 9]);
    assert_eq!(name(ty), "U8");
    assert_eq!((got[0], got[255], got[1]), (7, 9, 0));
    let (ty, got) = scalar_sparse(257, vec![256], vec![5]);
    assert_eq!(name(ty), "U16");
    assert_eq!((got[256], got[255]), (5, 0));
    let (ty, got) = scalar_sparse(65536, vec![1, 65535], vec![3, 4]);
    assert_eq!(name(ty), "U16");
    assert_eq!((got[1], got[65535]), (3, 4));
    let (ty, got) = scalar_sparse(65537, vec![0, 65536], vec![8, 6]);
    assert_eq!(name(ty), "U32");
    assert_eq!((got[0], got[65536], got[1]), (8, 6, 0));
}

#[test]
fn zero_base_has_no_buffer_view_and_reads_zeros() {
    let mut b = GltfBuilder::new();
    let spec = AccessorSpec::new().with_bounds(Bounds::Compute);
    let a = b.add_sparse_accessor::<i16>(4, T::Vec2, None, SparseSpec::new(vec![2], vec![-5, 9]), spec).unwrap();
    assert!(b.gltf.accessors.as_ref().unwrap()[a].buffer_view.is_none());
    assert!(b.validate().is_valid(), "{:?}", b.validate().errors);
    let bytes = b.to_glb_bytes().unwrap();
    let glb = gltf::Gltf::from_slice(&bytes).unwrap();
    let blob = bin_chunk(&bytes);
    let acc = glb.document.accessors().nth(a).unwrap();
    assert!(acc.view().is_none());
    assert_eq!(acc.data_type(), DataType::I16);
    let got: Vec<[i16; 2]> = Iter::new(acc.clone(), |_| Some(&blob[..])).unwrap().collect();
    assert_eq!(got, vec![[0, 0], [0, 0], [-5, 9], [0, 0]]);
    // The zero elements count for the bounds.
    assert_eq!(acc.min().unwrap().as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect::<Vec<_>>(), [-5, 0]);
    assert_eq!(acc.max().unwrap().as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect::<Vec<_>>(), [0, 9]);
}

#[test]
fn matrix_values_are_padded_per_column() {
    let mut b = GltfBuilder::new();
    let vals: Vec<i8> = (1..=9).collect();
    b.add_sparse_accessor::<i8>(2, T::Mat3, None, SparseSpec::new(vec![1], vals), AccessorSpec::new()).unwrap();
    let bytes = b.to_glb_bytes().unwrap();
    let bin = bin_chunk(&bytes);
    let doc = &b.gltf;
    let sp = doc.accessors.as_ref().unwrap()[0].sparse.as_ref().unwrap();
    let v = &doc.buffer_views.as_ref().unwrap()[sp.values.buffer_view];
    assert_eq!(v.byte_length, 12);
    assert_eq!(&bin[v.byte_offset..v.byte_offset + 12], &[1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0]);
}

#[test]
fn bounds_are_on_the_substituted_result() {
    let mut b = GltfBuilder::new();
    let base = [5.0f32, 1.0, 9.0];
    // Replacing the max (index 2) and the min (index 1) must change both bounds.
    let ok = AccessorSpec::new().with_bounds(Bounds::Explicit { min: vec![2.0], max: vec![5.0] });
    b.add_sparse_accessor(3, T::Scalar, Some(&base[..]), SparseSpec::new(vec![1, 2], vec![2.0, 3.0]), ok).unwrap();
    // Bounds of the base alone are wrong now.
    let before = state(&b);
    let stale = AccessorSpec::new().with_bounds(Bounds::Explicit { min: vec![1.0], max: vec![9.0] });
    let m = invalid(b.add_sparse_accessor(3, T::Scalar, Some(&base[..]), SparseSpec::new(vec![1, 2], vec![2.0, 3.0]), stale));
    assert!(m.contains("bounds"), "{m}");
    assert_eq!(state(&b), before);
}

#[test]
fn builder_errors_are_atomic() {
    let mut b = GltfBuilder::new();
    b.add_accessor_from_slice(&[1.0f32], T::Scalar, AccessorSpec::new()).unwrap();
    let before = state(&b);
    let base = [0.0f32; 4];
    let cases: Vec<(&str, mesh_tools::Result<usize>)> = vec![
        ("count 0", b.add_sparse_accessor::<f32>(0, T::Scalar, None, SparseSpec::new(vec![0], vec![1.0]), AccessorSpec::new())),
        ("no entries", b.add_sparse_accessor::<f32>(4, T::Scalar, None, SparseSpec::new(vec![], vec![]), AccessorSpec::new())),
        ("decreasing", b.add_sparse_accessor(4, T::Scalar, Some(&base[..]), SparseSpec::new(vec![2, 1], vec![1.0, 1.0]), AccessorSpec::new())),
        ("duplicate", b.add_sparse_accessor(4, T::Scalar, Some(&base[..]), SparseSpec::new(vec![1, 1], vec![1.0, 1.0]), AccessorSpec::new())),
        ("out of range", b.add_sparse_accessor(4, T::Scalar, Some(&base[..]), SparseSpec::new(vec![4], vec![1.0]), AccessorSpec::new())),
        ("value count", b.add_sparse_accessor(4, T::Vec2, None, SparseSpec::new(vec![1], vec![1.0f32]), AccessorSpec::new())),
        ("base length", b.add_sparse_accessor(5, T::Scalar, Some(&base[..]), SparseSpec::new(vec![1], vec![1.0]), AccessorSpec::new())),
        ("nan value", b.add_sparse_accessor(4, T::Scalar, None, SparseSpec::new(vec![1], vec![f32::NAN]), AccessorSpec::new())),
        ("nan base", b.add_sparse_accessor(1, T::Scalar, Some(&[f32::INFINITY][..]), SparseSpec::new(vec![0], vec![1.0]), AccessorSpec::new())),
        ("normalized f32", b.add_sparse_accessor(4, T::Scalar, None, SparseSpec::new(vec![1], vec![1.0f32]), AccessorSpec::new().with_normalized(true))),
    ];
    for (what, r) in cases {
        assert!(matches!(r, Err(GltfError::InvalidInput(_))), "{what}: {r:?}");
    }
    assert_eq!(state(&b), before);
}

// ---- validator negatives: start from a valid document and break one field ----

fn broken(edit: impl FnOnce(&mut mesh_tools::Accessor)) -> GltfBuilder {
    let (mut b, a) = five_vec3();
    edit(&mut b.gltf.accessors.as_mut().unwrap()[a]);
    b
}

#[test]
fn validator_sparse_count() {
    assert_error(&broken(|a| a.sparse.as_mut().unwrap().count = 0), "ACCESSOR_SPARSE_COUNT");
    assert_error(&broken(|a| a.sparse.as_mut().unwrap().count = 6), "ACCESSOR_SPARSE_COUNT");
}

#[test]
fn validator_sparse_indices_type() {
    assert_error(&broken(|a| a.sparse.as_mut().unwrap().indices.component_type = 5126), "ACCESSOR_SPARSE_INDICES_TYPE");
    assert_error(&broken(|a| a.sparse.as_mut().unwrap().indices.component_type = 5122), "ACCESSOR_SPARSE_INDICES_TYPE");
}

/// Hand-built zero-base VEC3 accessor of 5 elements whose u8 index bytes are `indices`.
fn hand_built(indices: &[u8]) -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let iv = b.add_data_view(indices, None, None).unwrap();
    let vv = b.add_data_view(&vec![0u8; 12 * indices.len()], None, None).unwrap();
    let mut a = mesh_tools::Accessor::default();
    a.component_type = 5126;
    a.count = 5;
    a.type_ = "VEC3".into();
    let mut sp = mesh_tools::Sparse::default();
    sp.count = indices.len();
    sp.indices.buffer_view = iv;
    sp.indices.component_type = 5121;
    sp.values.buffer_view = vv;
    a.sparse = Some(sp);
    b.gltf.accessors = Some(vec![a]);
    b
}

#[test]
fn validator_sparse_indices_order() {
    assert!(hand_built(&[1, 3]).validate().is_valid());
    assert_error(&hand_built(&[3, 3]), "ACCESSOR_SPARSE_INDICES_ORDER");
    assert_error(&hand_built(&[3, 1]), "ACCESSOR_SPARSE_INDICES_ORDER");
}

#[test]
fn validator_sparse_index_range() {
    assert_error(&hand_built(&[1, 5]), "ACCESSOR_SPARSE_INDEX_RANGE");
}

#[test]
fn validator_sparse_view_invalid() {
    assert_error(&broken(|a| a.sparse.as_mut().unwrap().indices.buffer_view = 99), "ACCESSOR_SPARSE_VIEW_INVALID");
    assert_error(&broken(|a| a.sparse.as_mut().unwrap().values.buffer_view = 99), "ACCESSOR_SPARSE_VIEW_INVALID");
    // A target or a byteStride on a sparse view is not allowed.
    for use_values in [false, true] {
        for stride in [false, true] {
            let mut b = five_vec3().0;
            let sp = b.gltf.accessors.as_ref().unwrap()[0].sparse.as_ref().unwrap();
            let vi = if use_values { sp.values.buffer_view } else { sp.indices.buffer_view };
            let v = &mut b.gltf.buffer_views.as_mut().unwrap()[vi];
            if stride { v.byte_stride = Some(12) } else { v.target = Some(34962) }
            assert_error(&b, "ACCESSOR_SPARSE_VIEW_INVALID");
        }
    }
}

#[test]
fn validator_sparse_range() {
    // Two indices need 2 bytes, two VEC3 floats need 24: ask for 3 entries of each.
    assert_error(&broken(|a| a.sparse.as_mut().unwrap().count = 3), "ACCESSOR_SPARSE_RANGE");
    assert_error(&broken(|a| a.sparse.as_mut().unwrap().values.byte_offset = Some(4)), "ACCESSOR_SPARSE_RANGE");
    assert_error(&broken(|a| a.sparse.as_mut().unwrap().indices.byte_offset = Some(usize::MAX)), "ACCESSOR_SPARSE_RANGE");
}

#[test]
fn validator_checks_bounds_against_the_substituted_data() {
    assert_error(&broken(|a| a.max = Some(vec![14.0, 14.0, 14.0])), "ACCESSOR_BOUNDS_MISMATCH");
    // Zero base: stored bounds that ignore the zeros are wrong.
    let mut b = GltfBuilder::new();
    let a = b.add_sparse_accessor::<u8>(3, T::Scalar, None, SparseSpec::new(vec![1], vec![4]), AccessorSpec::new()).unwrap();
    assert!(b.validate().is_valid());
    let acc = &mut b.gltf.accessors.as_mut().unwrap()[a];
    acc.min = Some(vec![4.0]);
    acc.max = Some(vec![4.0]);
    assert_error(&b, "ACCESSOR_BOUNDS_MISMATCH");
}

#[test]
fn hostile_count_does_not_allocate_or_overflow() {
    let mut b = GltfBuilder::new();
    let spec = AccessorSpec::new().with_bounds(Bounds::Compute);
    // Zero base with a huge count: bounds come from the one implicit zero element plus the values.
    let a = b.add_sparse_accessor::<u8>(1 << 40, T::Scalar, None, SparseSpec::new(vec![3], vec![7]), spec).unwrap();
    let acc = &b.gltf.accessors.as_ref().unwrap()[a];
    assert_eq!((acc.min.clone(), acc.max.clone()), (Some(vec![0.0]), Some(vec![7.0])));
    let before = state(&b);
    // count * components overflows: an error, not a panic.
    let base = [0u8; 4];
    let r = b.add_sparse_accessor(usize::MAX, T::Vec4, Some(&base[..]), SparseSpec::new(vec![0], vec![1u8; 4]), AccessorSpec::new());
    assert!(matches!(r, Err(GltfError::InvalidInput(_))), "{r:?}");
    assert_eq!(state(&b), before);
}

#[test]
fn zero_base_fully_replaced_has_no_zero_in_bounds() {
    let mut b = GltfBuilder::new();
    let spec = AccessorSpec::new().with_bounds(Bounds::Compute);
    let a = b.add_sparse_accessor::<u8>(2, T::Scalar, None, SparseSpec::new(vec![0, 1], vec![4, 6]), spec).unwrap();
    let acc = &b.gltf.accessors.as_ref().unwrap()[a];
    assert_eq!((acc.min.clone(), acc.max.clone()), (Some(vec![4.0]), Some(vec![6.0])));
}
