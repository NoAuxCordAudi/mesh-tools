//! Typed accessor writer: every (component, accessor type) pair against the independent `gltf`
//! reader and hand-derived byte oracles, bounds, normalized data, zero accessors, atomicity and
//! the `ACCESSOR_*` validator codes.

mod common;

use gltf::accessor::{DataType, Dimensions, Iter};
use mesh_tools::{
    AccessorSpec, AccessorType as T, Bounds, BufferViewTarget, Component, ComponentType as C, GltfBuilder, GltfError,
};

fn bin_chunk(bytes: &[u8]) -> Vec<u8> {
    let jl = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let at = 20 + jl;
    let bl = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    bytes[at + 8..at + 8 + bl].to_vec()
}

fn state(b: &GltfBuilder) -> (usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffers.as_ref().map_or(0, |a| a.first().map_or(0, |b| b.byte_length)),
    )
}

fn invalid<T: std::fmt::Debug>(r: mesh_tools::Result<T>) -> String {
    match r {
        Err(GltfError::InvalidInput(m)) => m,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[track_caller]
fn assert_error(b: &GltfBuilder, code: &str) {
    let r = b.validate();
    assert!(r.has_error(code), "expected error {code}, got: {:#?}", r.errors);
}

fn gltf_type(c: C) -> DataType {
    match c {
        C::I8 => DataType::I8,
        C::U8 => DataType::U8,
        C::I16 => DataType::I16,
        C::U16 => DataType::U16,
        C::U32 => DataType::U32,
        C::F32 => DataType::F32,
    }
}

fn gltf_dims(t: T) -> (Dimensions, usize) {
    match t {
        T::Scalar => (Dimensions::Scalar, 1),
        T::Vec2 => (Dimensions::Vec2, 2),
        T::Vec3 => (Dimensions::Vec3, 3),
        T::Vec4 => (Dimensions::Vec4, 4),
        T::Mat2 => (Dimensions::Mat2, 4),
        T::Mat3 => (Dimensions::Mat3, 9),
        T::Mat4 => (Dimensions::Mat4, 16),
    }
}

/// (columns, rows) of an accessor type; vectors are one column.
fn shape(t: T) -> (usize, usize) {
    match t {
        T::Scalar => (1, 1),
        T::Vec2 => (1, 2),
        T::Vec3 => (1, 3),
        T::Vec4 => (1, 4),
        T::Mat2 => (2, 2),
        T::Mat3 => (3, 3),
        T::Mat4 => (4, 4),
    }
}

/// Hand-written oracle: little-endian bytes with every matrix column padded to a 4-byte boundary.
fn expected_bytes<V: Copy>(data: &[V], t: T, to_le: &dyn Fn(V) -> Vec<u8>) -> Vec<u8> {
    let (cols, rows) = shape(t);
    let mut out = Vec::new();
    let mut it = data.iter();
    while it.len() > 0 {
        for _ in 0..cols {
            let mut col = Vec::new();
            for _ in 0..rows {
                col.extend(to_le(*it.next().unwrap()));
            }
            while cols > 1 && col.len() % 4 != 0 {
                col.push(0);
            }
            out.extend(col);
        }
    }
    out
}

fn check_pair<V: Component + Copy>(c: C, t: T, make: impl Fn(usize) -> V, to_le: impl Fn(V) -> Vec<u8>, to_f64: impl Fn(V) -> f64) {
    let (dims, comps) = gltf_dims(t);
    let count = 3;
    let data: Vec<V> = (0..count * comps).map(&make).collect();
    let mut b = GltfBuilder::new();
    let spec = AccessorSpec::new().with_bounds(Bounds::Compute);
    let a = b.add_accessor_from_slice(&data, t, spec).unwrap();
    let bytes = b.to_glb_bytes().unwrap();
    let glb = gltf::Gltf::from_slice(&bytes).unwrap();
    let acc = glb.document.accessors().nth(a).unwrap();
    let label = format!("{c:?} {t:?}");
    assert_eq!(acc.data_type(), gltf_type(c), "{label}");
    assert_eq!(acc.dimensions(), dims, "{label}");
    assert_eq!(acc.count(), count, "{label}");
    assert!(!acc.normalized(), "{label}");
    let view = acc.view().unwrap();
    let want = expected_bytes(&data, t, &to_le);
    assert_eq!(view.length(), want.len(), "{label}");
    let bin = bin_chunk(&bytes);
    assert_eq!(&bin[view.offset()..view.offset() + view.length()], &want[..], "{label}");
    // bounds, one per component, from the raw values
    let min = acc.min().unwrap();
    let max = acc.max().unwrap();
    for k in 0..comps {
        let col: Vec<f64> = (0..count).map(|e| to_f64(data[e * comps + k])).collect();
        let lo = col.iter().cloned().fold(f64::INFINITY, f64::min);
        let hi = col.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(min.as_array().unwrap()[k].as_f64().unwrap(), lo, "{label} min[{k}]");
        assert_eq!(max.as_array().unwrap()[k].as_f64().unwrap(), hi, "{label} max[{k}]");
    }
    assert_eq!(min.as_array().unwrap().len(), comps, "{label}");
    assert!(b.validate().is_valid(), "{label}: {:?}", b.validate().errors);
}

macro_rules! pair_tests {
    ($($name:ident: $ct:expr, $ty:ty, $make:expr;)*) => {$(
        #[test]
        fn $name() {
            for t in [T::Scalar, T::Vec2, T::Vec3, T::Vec4, T::Mat2, T::Mat3, T::Mat4] {
                check_pair::<$ty>($ct, t, $make, |v| v.to_le_bytes().to_vec(), |v| v as f64);
            }
        }
    )*};
}

pair_tests! {
    pair_i8: C::I8, i8, |i| (i as i8) * 2 - 40;
    pair_u8: C::U8, u8, |i| (i as u8) * 5 + 1;
    pair_i16: C::I16, i16, |i| (i as i16) * 500 - 9000;
    pair_u16: C::U16, u16, |i| (i as u16) * 1300 + 7;
    pair_u32: C::U32, u32, |i| 16_777_217 + (i as u32) * 99_999;
    pair_f32: C::F32, f32, |i| (i as f32) * 0.5 - 7.25;
}

#[test]
fn matrix_padding_is_asserted_byte_by_byte() {
    let mut b = GltfBuilder::new();
    let cases: [(Vec<u8>, T, Vec<u8>); 2] = [
        (vec![1, 2, 3, 4], T::Mat2, vec![1, 2, 0, 0, 3, 4, 0, 0]),
        (vec![1, 2, 3, 4, 5, 6, 7, 8, 9], T::Mat3, vec![1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0]),
    ];
    for (data, t, want) in cases {
        let a = b.add_accessor_from_slice(&data, t, AccessorSpec::new()).unwrap();
        let v = b.gltf.accessors.as_ref().unwrap()[a].buffer_view.unwrap();
        let view = &b.gltf.buffer_views.as_ref().unwrap()[v];
        assert_eq!(view.byte_length, want.len());
        assert_eq!(&b.to_glb_bytes().map(|g| bin_chunk(&g)).unwrap()[view.byte_offset..view.byte_offset + want.len()], &want[..]);
    }
    // MAT3 of i16: 6 data bytes + 2 padding per column; MAT2 of i16 needs none.
    let m3: Vec<i16> = (1..=9).collect();
    let a = b.add_accessor_from_slice(&m3, T::Mat3, AccessorSpec::new()).unwrap();
    let v = b.gltf.accessors.as_ref().unwrap()[a].buffer_view.unwrap();
    assert_eq!(b.gltf.buffer_views.as_ref().unwrap()[v].byte_length, 24);
    let m2: Vec<i16> = (1..=4).collect();
    let a = b.add_accessor_from_slice(&m2, T::Mat2, AccessorSpec::new()).unwrap();
    let v = b.gltf.accessors.as_ref().unwrap()[a].buffer_view.unwrap();
    assert_eq!(b.gltf.buffer_views.as_ref().unwrap()[v].byte_length, 8);
    assert!(b.validate().is_valid(), "{:?}", b.validate().errors);
}

#[test]
fn normalized_i8_vec3_reads_back_as_unit_range() {
    let mut b = GltfBuilder::new();
    let a = b
        .add_accessor_from_slice(&[-127i8, 0, 127], T::Vec3, AccessorSpec::new().with_normalized(true).with_bounds(Bounds::Compute))
        .unwrap();
    let glb = common::parse_glb(&b);
    let acc = glb.document.accessors().nth(a).unwrap();
    assert!(acc.normalized());
    assert_eq!(acc.data_type(), DataType::I8);
    let blob = glb.blob.as_deref();
    let raw: Vec<[i8; 3]> = Iter::new(acc, |_| blob).unwrap().collect();
    assert_eq!(raw, vec![[-127, 0, 127]]);
    // glTF normalization for signed bytes is max(c / 127, -1): -127 -> -1, 0 -> 0, 127 -> 1.
    let f: Vec<f32> = raw[0].iter().map(|&c| (c as f32 / 127.0).max(-1.0)).collect();
    assert_eq!(f, vec![-1.0, 0.0, 1.0]);
    // bounds stay in the raw integer domain
    let acc = glb.document.accessors().nth(a).unwrap();
    assert_eq!(acc.min().unwrap(), serde_json::json!([-127, 0, 127]));
}

#[test]
fn normalized_u8_and_u16_round_trip_and_are_rejected_for_float_and_u32() {
    let mut b = GltfBuilder::new();
    let spec = || AccessorSpec::new().with_normalized(true);
    let a8 = b.add_accessor_from_slice(&[0u8, 255, 51, 102], T::Vec2, spec()).unwrap();
    let a16 = b.add_accessor_from_slice(&[0u16, 65535, 32768], T::Scalar, spec()).unwrap();
    let ai16 = b.add_accessor_from_slice(&[-32768i16, 32767], T::Vec2, spec()).unwrap();
    let glb = common::parse_glb(&b);
    let blob = glb.blob.as_deref();
    let accs: Vec<_> = glb.document.accessors().collect();
    assert!(accs[a8].normalized() && accs[a16].normalized() && accs[ai16].normalized());
    let got: Vec<[u8; 2]> = Iter::new(accs[a8].clone(), |_| blob).unwrap().collect();
    assert_eq!(got, vec![[0, 255], [51, 102]]);
    let got: Vec<u16> = Iter::new(accs[a16].clone(), |_| blob).unwrap().collect();
    assert_eq!(got, vec![0, 65535, 32768]);
    assert_eq!(accs[a8].count(), 2);

    let before = state(&b);
    assert!(invalid(b.add_accessor_from_slice(&[1.0f32], T::Scalar, spec())).contains("normalized"));
    assert!(invalid(b.add_accessor_from_slice(&[1u32], T::Scalar, spec())).contains("normalized"));
    assert_eq!(state(&b), before);
}

#[test]
fn integer_bounds_are_exact_above_two_pow_24() {
    let mut b = GltfBuilder::new();
    let a = b
        .add_accessor_from_slice(&[16_777_217u32, 4_294_967_295, 16_777_219], T::Scalar, AccessorSpec::new().with_bounds(Bounds::Compute))
        .unwrap();
    let glb = common::parse_glb(&b);
    let acc = glb.document.accessors().nth(a).unwrap();
    assert_eq!(acc.min().unwrap(), serde_json::json!([16_777_217u64]));
    assert_eq!(acc.max().unwrap(), serde_json::json!([4_294_967_295u64]));
}

#[test]
fn f32_bounds_are_the_exact_widened_values() {
    let mut b = GltfBuilder::new();
    let a = b.add_accessor_from_slice(&[0.1f32, -0.3], T::Scalar, AccessorSpec::new().with_bounds(Bounds::Compute)).unwrap();
    let glb = common::parse_glb(&b);
    let acc = glb.document.accessors().nth(a).unwrap();
    assert_eq!(acc.max().unwrap(), serde_json::json!([0.1f32 as f64]));
    assert_eq!(acc.min().unwrap(), serde_json::json!([-0.3f32 as f64]));
}

#[test]
fn explicit_bounds_must_match_and_are_checked() {
    let mut b = GltfBuilder::new();
    let ok = Bounds::Explicit { min: vec![1.0, 2.0], max: vec![3.0, 4.0] };
    b.add_accessor_from_slice(&[1.0f32, 2.0, 3.0, 4.0], T::Vec2, AccessorSpec::new().with_bounds(ok)).unwrap();
    let before = state(&b);
    let data = [1.0f32, 2.0, 3.0, 4.0];
    let bad = |min: Vec<f64>, max: Vec<f64>| AccessorSpec::new().with_bounds(Bounds::Explicit { min, max });
    assert!(invalid(b.add_accessor_from_slice(&data, T::Vec2, bad(vec![1.0], vec![3.0, 4.0]))).contains("length"));
    assert!(invalid(b.add_accessor_from_slice(&data, T::Vec2, bad(vec![4.0, 2.0], vec![3.0, 4.0]))).contains("min"));
    assert!(invalid(b.add_accessor_from_slice(&data, T::Vec2, bad(vec![0.0, 2.0], vec![3.0, 4.0]))).contains("match"));
    assert_eq!(state(&b), before);
}

#[test]
fn rejects_non_finite_empty_and_ragged_data_without_changing_the_document() {
    let mut b = GltfBuilder::new();
    b.add_accessor_from_slice(&[1.0f32, 2.0, 3.0], T::Vec3, AccessorSpec::new()).unwrap();
    let before = state(&b);
    assert!(invalid(b.add_accessor_from_slice(&[1.0f32, f32::NAN, 3.0], T::Vec3, AccessorSpec::new())).contains("finite"));
    assert!(invalid(b.add_accessor_from_slice(&[f32::INFINITY], T::Scalar, AccessorSpec::new())).contains("finite"));
    assert!(invalid(b.add_accessor_from_slice::<f32>(&[], T::Vec3, AccessorSpec::new())).contains("empty"));
    assert!(invalid(b.add_accessor_from_slice(&[1.0f32, 2.0], T::Vec3, AccessorSpec::new())).contains("multiple"));
    assert_eq!(state(&b), before);
}

#[test]
fn target_and_name_are_written() {
    let mut b = GltfBuilder::new();
    let spec = AccessorSpec::new().with_name("idx").with_target(BufferViewTarget::ElementArrayBuffer);
    let a = b.add_accessor_from_slice(&[0u16, 1, 2], T::Scalar, spec).unwrap();
    let glb = common::parse_glb(&b);
    let acc = glb.document.accessors().nth(a).unwrap();
    assert_eq!(acc.name(), Some("idx"));
    assert_eq!(acc.view().unwrap().target(), Some(gltf::buffer::Target::ElementArrayBuffer));
}

#[test]
fn zero_accessor_has_no_buffer_view_and_writes_no_bytes() {
    let mut b = GltfBuilder::new();
    let a = b.add_zero_accessor(5, T::Vec3, C::F32, false).unwrap();
    assert_eq!(state(&b), (1, 0, 0));
    let n = b.add_zero_accessor(2, T::Vec2, C::U8, true).unwrap();
    let bytes = b.to_glb_bytes().unwrap();
    // The reader (1.4) still demands a bufferView when validating, so parse without validation.
    let glb = gltf::Gltf::from_slice_without_validation(&bytes).unwrap();
    let acc = glb.document.accessors().nth(a).unwrap();
    assert!(acc.view().is_none());
    assert_eq!((acc.count(), acc.dimensions(), acc.data_type()), (5, Dimensions::Vec3, DataType::F32));
    assert!(glb.document.accessors().nth(n).unwrap().normalized());
    let before = state(&b);
    assert!(invalid(b.add_zero_accessor(0, T::Vec3, C::F32, false)).contains("count"));
    assert!(invalid(b.add_zero_accessor(1, T::Vec3, C::F32, true)).contains("normalized"));
    assert_eq!(state(&b), before);
}

#[test]
fn accessors_in_a_shared_view_with_offsets() {
    let mut b = GltfBuilder::new();
    let floats = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
    let bytes: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
    let view = b.add_data_view(&bytes, Some(BufferViewTarget::ArrayBuffer), None).unwrap();
    let a0 = b.add_accessor_in_view(view, 0, 1, T::Vec3, C::F32, false, Bounds::Compute).unwrap();
    let a1 = b.add_accessor_in_view(view, 12, 1, T::Vec3, C::F32, false, Bounds::None).unwrap();
    let glb = common::parse_glb(&b);
    let blob = glb.blob.as_deref();
    let accs: Vec<_> = glb.document.accessors().collect();
    let v1: Vec<[f32; 3]> = Iter::new(accs[a1].clone(), |_| blob).unwrap().collect();
    assert_eq!(v1, vec![[4.0, 5.0, 6.0]]);
    assert_eq!(accs[a0].min().unwrap(), serde_json::json!([1.0, 2.0, 3.0]));
    assert_eq!(accs[a1].offset(), 12);

    let before = state(&b);
    assert!(invalid(b.add_accessor_in_view(view, 0, 3, T::Vec3, C::F32, false, Bounds::None)).contains("range"));
    assert!(invalid(b.add_accessor_in_view(view, 2, 1, T::Vec3, C::F32, false, Bounds::None)).contains("align"));
    assert!(invalid(b.add_accessor_in_view(99, 0, 1, T::Vec3, C::F32, false, Bounds::None)).contains("view"));
    assert!(invalid(b.add_accessor_in_view(view, 0, 0, T::Vec3, C::F32, false, Bounds::None)).contains("count"));
    let wrong = Bounds::Explicit { min: vec![0.0; 3], max: vec![1.0; 3] };
    assert!(invalid(b.add_accessor_in_view(view, 0, 1, T::Vec3, C::F32, false, wrong)).contains("match"));
    assert_eq!(state(&b), before);
}

#[test]
fn data_view_validates_stride_and_target() {
    let mut b = GltfBuilder::new();
    let before = state(&b);
    assert!(invalid(b.add_data_view(&[], None, None)).contains("empty"));
    assert!(invalid(b.add_data_view(&[0; 16], None, Some(3))).contains("stride"));
    assert!(invalid(b.add_data_view(&[0; 16], None, Some(256))).contains("stride"));
    assert!(invalid(b.add_data_view(&[0; 16], Some(BufferViewTarget::ElementArrayBuffer), Some(4))).contains("stride"));
    assert_eq!(state(&b), before);
    b.add_data_view(&[0; 16], Some(BufferViewTarget::ArrayBuffer), Some(8)).unwrap();
    assert_eq!(b.gltf.buffer_views.as_ref().unwrap()[0].byte_stride, Some(8));
}

// ---- validator codes ----

fn one_vec3() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    b.add_accessor_from_slice(&[1.0f32, 2.0, 3.0], T::Vec3, AccessorSpec::new().with_bounds(Bounds::Compute)).unwrap();
    b
}

#[test]
fn validator_normalized_invalid() {
    let mut b = one_vec3();
    b.gltf.accessors.as_mut().unwrap()[0].normalized = Some(true);
    assert_error(&b, "ACCESSOR_NORMALIZED_INVALID");
}

#[test]
fn validator_buffer_view_missing() {
    let mut b = one_vec3();
    let a = &mut b.gltf.accessors.as_mut().unwrap()[0];
    a.buffer_view = None;
    a.byte_offset = Some(4);
    assert_error(&b, "ACCESSOR_BUFFER_VIEW_MISSING");
}

#[test]
fn validator_bounds_length_order_type() {
    let mut b = one_vec3();
    b.gltf.accessors.as_mut().unwrap()[0].min = Some(vec![1.0, 2.0]);
    assert_error(&b, "ACCESSOR_BOUNDS_LENGTH");

    let mut b = one_vec3();
    b.gltf.accessors.as_mut().unwrap()[0].min = Some(vec![9.0, 2.0, 3.0]);
    assert_error(&b, "ACCESSOR_BOUNDS_ORDER");

    let mut b = GltfBuilder::new();
    b.add_accessor_from_slice(&[1u8, 2], T::Scalar, AccessorSpec::new().with_bounds(Bounds::Compute)).unwrap();
    b.gltf.accessors.as_mut().unwrap()[0].min = Some(vec![1.5]);
    assert_error(&b, "ACCESSOR_BOUNDS_TYPE");
    b.gltf.accessors.as_mut().unwrap()[0].min = Some(vec![-1.0]);
    assert_error(&b, "ACCESSOR_BOUNDS_TYPE"); // below the u8 range
}

#[test]
fn validator_bounds_mismatch_is_an_error() {
    let mut b = one_vec3();
    b.gltf.accessors.as_mut().unwrap()[0].max = Some(vec![1.0, 2.0, 4.0]);
    assert_error(&b, "ACCESSOR_BOUNDS_MISMATCH");
    assert!(b.to_glb_bytes().is_err(), "export must fail on a bounds mismatch");
    // exact data passes, including the integer domain of a normalized accessor
    let mut b = GltfBuilder::new();
    b.add_accessor_from_slice(&[-127i8, 5], T::Scalar, AccessorSpec::new().with_normalized(true).with_bounds(Bounds::Compute)).unwrap();
    assert!(b.validate().is_valid());
}

#[test]
fn validator_bounds_required_on_position() {
    let mut b = GltfBuilder::new();
    let a = b.add_accessor_from_slice(&[0.0f32, 0.0, 0.0], T::Vec3, AccessorSpec::new()).unwrap();
    let mut p = mesh_tools::Primitive::default();
    p.attributes.insert("POSITION".to_string(), a);
    b.add_mesh(None, vec![p]);
    assert_error(&b, "ACCESSOR_BOUNDS_REQUIRED");
    b.gltf.accessors.as_mut().unwrap()[0].min = Some(vec![0.0; 3]);
    b.gltf.accessors.as_mut().unwrap()[0].max = Some(vec![0.0; 3]);
    assert!(!b.validate().has_error("ACCESSOR_BOUNDS_REQUIRED"));
}

#[test]
fn validator_data_nonfinite() {
    let mut b = GltfBuilder::new();
    let bytes: Vec<u8> = [1.0f32, f32::NAN, 3.0].iter().flat_map(|f| f.to_le_bytes()).collect();
    let v = b.add_data_view(&bytes, None, None).unwrap();
    // The writer refuses this data, so build the accessor by hand.
    let mut a = mesh_tools::models::Accessor::default();
    (a.buffer_view, a.component_type, a.count, a.type_) = (Some(v), 5126, 1, "VEC3".into());
    b.gltf.accessors = Some(vec![a]);
    assert_error(&b, "ACCESSOR_DATA_NONFINITE");
}

#[test]
fn zero_accessor_passes_validation_and_dangling_view_still_errors() {
    let mut b = GltfBuilder::new();
    b.add_zero_accessor(3, T::Vec3, C::F32, false).unwrap();
    assert!(b.validate().is_valid(), "{:?}", b.validate().errors);
    b.gltf.accessors.as_mut().unwrap()[0].buffer_view = Some(7);
    assert_error(&b, "ACCESSOR_BUFFER_VIEW_INVALID");
}

#[test]
fn accessor_in_view_rejects_non_finite_float_bytes() {
    let mut b = GltfBuilder::new();
    let bytes: Vec<u8> = [1.0f32, f32::INFINITY, f32::NAN].iter().flat_map(|f| f.to_le_bytes()).collect();
    let view = b.add_data_view(&bytes, None, None).unwrap();
    let before = state(&b);
    for bounds in [Bounds::None, Bounds::Compute] {
        assert!(invalid(b.add_accessor_in_view(view, 0, 1, T::Vec3, C::F32, false, bounds)).contains("finite"));
    }
    assert_eq!(state(&b), before);
}
