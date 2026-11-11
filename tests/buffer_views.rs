//! Interleaved vertex data and `add_data_view`, read back through the independent `gltf` reader
//! with hand-derived byte oracles.

mod common;

use gltf::accessor::Iter;
use mesh_tools::{AccessorType as T, Bounds, BufferViewTarget, GltfBuilder, GltfError, InterleavedAttribute as Attr};

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

const POS: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
const NRM: [f32; 9] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
const UV: [f32; 6] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0];

#[test]
fn position_normal_texcoord_stride_32() {
    let mut b = GltfBuilder::new();
    let attrs = [
        Attr::new("POSITION", &POS, T::Vec3, false, Bounds::Compute).unwrap(),
        Attr::new("NORMAL", &NRM, T::Vec3, false, Bounds::None).unwrap(),
        Attr::new("TEXCOORD_0", &UV, T::Vec2, false, Bounds::None).unwrap(),
    ];
    let made = b.add_interleaved_attributes(3, &attrs).unwrap();
    assert_eq!(made.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["POSITION", "NORMAL", "TEXCOORD_0"]);
    assert!(b.validate().is_valid(), "{:?}", b.validate().errors);

    let bytes = b.to_glb_bytes().unwrap();
    let bin = bin_chunk(&bytes);
    let glb = gltf::Gltf::from_slice(&bytes).unwrap();
    let doc = &glb.document;
    let get = |i: usize| doc.accessors().nth(made[i].1).unwrap();
    let view = get(0).view().unwrap();
    assert_eq!(view.stride(), Some(32));
    assert_eq!(view.length(), 96);
    assert_eq!(view.target(), Some(gltf::buffer::Target::ArrayBuffer));
    // One shared view, offsets 0 / 12 / 24 (hand-derived: 12 + 12 + 8 = 32).
    assert_eq!([get(0).offset(), get(1).offset(), get(2).offset()], [0, 12, 24]);
    assert!([get(1).view().unwrap().index(), get(2).view().unwrap().index()].iter().all(|&i| i == view.index()));
    assert_eq!(get(0).min().unwrap(), serde_json::json!([0.0, 0.0, 0.0]));
    assert_eq!(get(0).max().unwrap(), serde_json::json!([1.0, 1.0, 0.0]));
    assert!(get(1).min().is_none());

    let src = |_: gltf::Buffer| Some(&bin[..]);
    let p: Vec<[f32; 3]> = Iter::new(get(0), src).unwrap().collect();
    let n: Vec<[f32; 3]> = Iter::new(get(1), src).unwrap().collect();
    let t: Vec<[f32; 2]> = Iter::new(get(2), src).unwrap().collect();
    assert_eq!(p, [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    assert_eq!(n, [[0.0, 0.0, 1.0]; 3]);
    assert_eq!(t, [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);

    // Raw bytes of vertex 1: position (1,0,0), normal (0,0,1), uv (1,0).
    let v1 = &bin[view.offset() + 32..view.offset() + 64];
    let mut want = Vec::new();
    for f in [1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0] {
        want.extend(f.to_le_bytes());
    }
    assert_eq!(v1, &want[..]);
}

#[test]
fn mixed_sizes_offsets_0_12_16_stride_20() {
    let mut b = GltfBuilder::new();
    let color: [u8; 8] = [255, 0, 0, 255, 0, 255, 0, 128];
    let uv: [u16; 4] = [0, 65535, 32768, 1];
    let attrs = [
        Attr::new("POSITION", &POS[..6], T::Vec3, false, Bounds::Compute).unwrap(),
        Attr::new("COLOR_0", &color, T::Vec4, true, Bounds::None).unwrap(),
        Attr::new("TEXCOORD_0", &uv, T::Vec2, true, Bounds::None).unwrap(),
    ];
    let made = b.add_interleaved_attributes(2, &attrs).unwrap();
    assert!(b.validate().is_valid(), "{:?}", b.validate().errors);
    let bytes = b.to_glb_bytes().unwrap();
    let bin = bin_chunk(&bytes);
    let glb = gltf::Gltf::from_slice(&bytes).unwrap();
    let get = |i: usize| glb.document.accessors().nth(made[i].1).unwrap();
    assert_eq!(get(0).view().unwrap().stride(), Some(20));
    assert_eq!([get(0).offset(), get(1).offset(), get(2).offset()], [0, 12, 16]);
    assert!(get(1).normalized() && get(2).normalized() && !get(0).normalized());
    let src = |_: gltf::Buffer| Some(&bin[..]);
    let c: Vec<[u8; 4]> = Iter::new(get(1), src).unwrap().collect();
    let t: Vec<[u16; 2]> = Iter::new(get(2), src).unwrap().collect();
    assert_eq!(c, [[255, 0, 0, 255], [0, 255, 0, 128]]);
    assert_eq!(t, [[0, 65535], [32768, 1]]);
}

#[test]
fn unaligned_elements_are_padded_to_four_bytes() {
    // u8 VEC3 is 3 bytes: the second attribute starts at 4, the stride is 8.
    let mut b = GltfBuilder::new();
    let a = [1u8, 2, 3, 4, 5, 6];
    let c = [7u8, 8, 9, 10, 11, 12];
    let attrs = [
        Attr::new("_A", &a, T::Vec3, false, Bounds::None).unwrap(),
        Attr::new("_B", &c, T::Vec3, false, Bounds::None).unwrap(),
    ];
    let made = b.add_interleaved_attributes(2, &attrs).unwrap();
    let bytes = b.to_glb_bytes().unwrap();
    let bin = bin_chunk(&bytes);
    let glb = gltf::Gltf::from_slice(&bytes).unwrap();
    let acc = glb.document.accessors().nth(made[1].1).unwrap();
    assert_eq!((acc.offset(), acc.view().unwrap().stride()), (4, Some(8)));
    let off = acc.view().unwrap().offset();
    assert_eq!(&bin[off..off + 16], &[1, 2, 3, 0, 7, 8, 9, 0, 4, 5, 6, 0, 10, 11, 12, 0]);
}

#[test]
fn stride_252_is_accepted_and_256_is_rejected_unchanged() {
    let one = [0.0f32; 4];
    let mk = |n: usize, last_vec3: bool| -> Vec<Attr> {
        (0..n)
            .map(|i| {
                if last_vec3 && i == n - 1 {
                    Attr::new(format!("_A{i}"), &one[..3], T::Vec3, false, Bounds::None).unwrap()
                } else {
                    Attr::new(format!("_A{i}"), &one, T::Vec4, false, Bounds::None).unwrap()
                }
            })
            .collect()
    };
    let mut b = GltfBuilder::new();
    b.add_interleaved_attributes(1, &mk(16, true)).unwrap(); // 15 * 16 + 12 = 252
    assert_eq!(b.gltf.buffer_views.as_ref().unwrap()[0].byte_stride, Some(252));
    assert!(b.validate().is_valid());

    let mut b = GltfBuilder::new();
    b.add_data_view(&[0u8; 4], None, None).unwrap();
    let before = state(&b);
    let msg = invalid(b.add_interleaved_attributes(1, &mk(16, false))); // 256
    assert!(msg.contains("252"), "{msg}");
    assert_eq!(state(&b), before);
}

#[test]
fn bad_input_is_rejected_unchanged() {
    let mut b = GltfBuilder::new();
    b.add_data_view(&[0u8; 4], None, None).unwrap();
    let before = state(&b);
    let pos = Attr::new("POSITION", &POS, T::Vec3, false, Bounds::Compute).unwrap();
    let uv = Attr::new("TEXCOORD_0", &UV[..4], T::Vec2, false, Bounds::None).unwrap();
    assert!(invalid(b.add_interleaved_attributes(3, &[pos.clone(), uv])).contains("TEXCOORD_0"));
    assert!(invalid(b.add_interleaved_attributes(3, &[pos.clone(), pos.clone()])).contains("duplicate"));
    assert!(invalid(b.add_interleaved_attributes(3, &[])).contains("at least one"));
    assert!(invalid(b.add_interleaved_attributes(0, &[pos])).contains("vertex count"));
    assert_eq!(state(&b), before);
    // Constructor errors.
    assert!(Attr::new("POSITION", &[f32::NAN, 0.0, 0.0], T::Vec3, false, Bounds::None).is_err());
    assert!(Attr::new("POSITION", &POS[..4], T::Vec3, false, Bounds::None).is_err());
    assert!(Attr::new("POSITION", &[0.0f32; 0], T::Vec3, false, Bounds::None).is_err());
    assert!(Attr::new("X", &POS, T::Vec3, true, Bounds::None).is_err());
    assert!(Attr::new("", &POS, T::Vec3, false, Bounds::None).is_err());
    assert!(Attr::new("POSITION", &POS, T::Vec3, false, Bounds::Explicit { min: vec![9.0; 3], max: vec![9.0; 3] }).is_err());
}

#[test]
fn data_view_stride_and_target() {
    let mut b = GltfBuilder::new();
    let v = b.add_data_view(&[0u8; 32], Some(BufferViewTarget::ArrayBuffer), Some(16)).unwrap();
    let view = &b.gltf.buffer_views.as_ref().unwrap()[v];
    assert_eq!((view.byte_stride, view.target), (Some(16), Some(34962)));
    let before = state(&b);
    for stride in [0, 2, 6, 256, 253] {
        assert!(b.add_data_view(&[0u8; 32], None, Some(stride)).is_err(), "stride {stride}");
    }
    assert!(b.add_data_view(&[0u8; 32], Some(BufferViewTarget::ElementArrayBuffer), Some(4)).is_err());
    assert_eq!(state(&b), before);
}
