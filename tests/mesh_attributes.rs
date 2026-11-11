//! Tangents, vertex colors, normalized texcoords and u32 indices, read back through the
//! independent `gltf` crate against hand-derived values.

mod common;

use gltf::accessor::DataType;
use mesh_tools::{ColorData, GltfBuilder, GltfError, Indices, PrimitiveSpec, TexCoords};

fn close(a: &[f32], b: &[f32]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

const TRI: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

fn spec() -> PrimitiveSpec {
    PrimitiveSpec { positions: TRI.to_vec(), indices: Some(vec![0u16, 1, 2].into()), ..Default::default() }
}

fn sizes(b: &GltfBuilder) -> (usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.to_glb_bytes_unchecked().unwrap().len(),
    )
}

fn one_prim(spec: PrimitiveSpec) -> (GltfBuilder, gltf::Gltf) {
    let mut b = GltfBuilder::new();
    let p = b.create_primitive(spec).unwrap();
    b.add_mesh_checked(None, vec![p], None).unwrap();
    let r = b.validate();
    assert!(r.is_valid() && r.warnings.is_empty(), "{r:#?}");
    let g = common::parse_glb(&b);
    (b, g)
}

#[test]
fn tangents_round_trip_with_both_handedness_signs() {
    let t = vec![1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0];
    let (_, g) = one_prim(PrimitiveSpec {
        normals: Some(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]),
        tangents: Some(t),
        ..spec()
    });
    let prim = g.meshes().next().unwrap().primitives().next().unwrap();
    let got: Vec<[f32; 4]> = prim.reader(|_| g.blob.as_deref()).read_tangents().unwrap().collect();
    assert_eq!(got, vec![[1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, -1.0], [0.0, 1.0, 0.0, 1.0]]);
}

#[test]
fn bad_tangents_are_rejected_atomically() {
    let mut b = GltfBuilder::new();
    let before = sizes(&b);
    let w_half = vec![1.0, 0.0, 0.0, 0.5, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0];
    let r = b.create_primitive(PrimitiveSpec { tangents: Some(w_half), ..spec() });
    assert!(matches!(r, Err(GltfError::InvalidInput(m)) if m.contains("w") && m.contains("0.5")));
    let short = vec![1.0, 0.0, 0.0, 1.0];
    assert!(b.create_primitive(PrimitiveSpec { tangents: Some(short), ..spec() }).is_err());
    let nan = vec![f32::NAN, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0];
    assert!(b.create_primitive(PrimitiveSpec { tangents: Some(nan), ..spec() }).is_err());
    assert_eq!(sizes(&b), before);
}

#[test]
fn float_colors_round_trip_as_rgb_and_rgba() {
    let rgb = vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.5];
    let rgba = vec![1.0, 0.0, 0.0, 0.25, 0.0, 1.0, 0.0, 0.5, 0.0, 0.0, 1.0, 1.0];
    let (_, g) = one_prim(PrimitiveSpec { colors: vec![ColorData::Rgb(rgb), ColorData::Rgba(rgba)], ..spec() });
    let r_prim = g.meshes().next().unwrap().primitives().next().unwrap();
    let r = r_prim.reader(|_| g.blob.as_deref());
    let c0: Vec<[f32; 3]> = r.read_colors(0).unwrap().into_rgb_f32().collect();
    assert_eq!(c0, vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.5]]);
    let c1: Vec<[f32; 4]> = r.read_colors(1).unwrap().into_rgba_f32().collect();
    assert_eq!(c1[0], [1.0, 0.0, 0.0, 0.25]);
    assert_eq!(c1[2], [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn integer_colors_are_written_normalized() {
    // u8 RGBA: 255 -> 1.0, 51 -> 0.2; u16 RGB: 65535 -> 1.0, 32768 -> 32768/65535.
    let u8s = vec![255, 0, 0, 51, 0, 255, 0, 255, 0, 0, 255, 0];
    let u16s = vec![65535u16, 0, 0, 0, 65535, 0, 0, 0, 32768];
    let (b, g) = one_prim(PrimitiveSpec {
        colors: vec![ColorData::RgbaU8(u8s), ColorData::RgbU16(u16s)],
        ..spec()
    });
    let prim = g.meshes().next().unwrap().primitives().next().unwrap();
    let acc0 = prim.get(&gltf::Semantic::Colors(0)).unwrap();
    assert_eq!((acc0.data_type(), acc0.normalized()), (DataType::U8, true));
    let acc1 = prim.get(&gltf::Semantic::Colors(1)).unwrap();
    assert_eq!((acc1.data_type(), acc1.normalized()), (DataType::U16, true));
    let r = prim.reader(|_| g.blob.as_deref());
    let raw: Vec<[u8; 4]> = r.read_colors(0).unwrap().into_rgba_u8().collect();
    assert_eq!(raw[0], [255, 0, 0, 51]);
    let f: Vec<[f32; 4]> = r.read_colors(0).unwrap().into_rgba_f32().collect();
    assert!(close(&f[0], &[1.0, 0.0, 0.0, 0.2]), "{:?}", f[0]);
    let w: Vec<[f32; 3]> = r.read_colors(1).unwrap().into_rgb_f32().collect();
    assert!(close(&w[2], &[0.0, 0.0, 32768.0 / 65535.0]), "{:?}", w[2]);
    // 3-byte RGB u8 / 6-byte RGB u16 elements are stored in a 4/8-byte-strided view (4-byte alignment).
    let views = b.gltf.buffer_views.as_ref().unwrap();
    let strides: Vec<_> = views.iter().filter_map(|v| v.byte_stride).collect();
    assert_eq!(strides, vec![8]);
}

#[test]
fn wrong_color_length_is_rejected() {
    let mut b = GltfBuilder::new();
    let r = b.create_primitive(PrimitiveSpec { colors: vec![ColorData::Rgb(vec![1.0; 8])], ..spec() });
    assert!(matches!(r, Err(GltfError::InvalidInput(m)) if m.contains("color set 0")));
    assert!(b.create_primitive(PrimitiveSpec { colors: vec![ColorData::RgbaU8(vec![0; 11])], ..spec() }).is_err());
    assert!(b.create_primitive(PrimitiveSpec { colors: vec![ColorData::Rgb(vec![f32::INFINITY; 9])], ..spec() }).is_err());
    assert_eq!(b.gltf.accessors.as_ref().map_or(0, |a| a.len()), 0);
}

#[test]
fn normalized_texcoords_round_trip() {
    // u8: 0, 255, 51 -> 0, 1, 0.2 ; u16: 65535 -> 1.0
    let (_, g) = one_prim(PrimitiveSpec {
        texcoords: vec![
            TexCoords::F32(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0]),
            TexCoords::U8(vec![0, 0, 255, 0, 51, 255]),
            TexCoords::U16(vec![0, 0, 65535, 0, 0, 65535]),
        ],
        ..spec()
    });
    let prim = g.meshes().next().unwrap().primitives().next().unwrap();
    let r = prim.reader(|_| g.blob.as_deref());
    let u8s: Vec<[f32; 2]> = r.read_tex_coords(1).unwrap().into_f32().collect();
    assert!(close(&u8s[2], &[0.2, 1.0]) && u8s[1] == [1.0, 0.0], "{u8s:?}");
    let u16s: Vec<[f32; 2]> = r.read_tex_coords(2).unwrap().into_f32().collect();
    assert_eq!(u16s[1], [1.0, 0.0]);
    assert_eq!(prim.get(&gltf::Semantic::TexCoords(0)).unwrap().data_type(), DataType::F32);
}

#[test]
fn u32_indices_are_written_as_given_and_may_exceed_65535() {
    // 70001 vertices; index 70000 does not fit a u16.
    let n = 70_001usize;
    let positions: Vec<f32> = (0..n).flat_map(|i| [i as f32, 0.0, 0.0]).collect();
    let (_, g) = one_prim(PrimitiveSpec {
        positions,
        indices: Some(Indices::U32(vec![0, 65_535, 70_000])),
        ..Default::default()
    });
    let prim = g.meshes().next().unwrap().primitives().next().unwrap();
    assert_eq!(prim.indices().unwrap().data_type(), DataType::U32);
    let idx: Vec<u32> = prim.reader(|_| g.blob.as_deref()).read_indices().unwrap().into_u32().collect();
    assert_eq!(idx, vec![0, 65_535, 70_000]);
    // Explicit u8 / u16 widths are honored too.
    let (_, g) = one_prim(PrimitiveSpec { indices: Some(Indices::U8(vec![2, 1, 0])), ..spec() });
    let prim = g.meshes().next().unwrap().primitives().next().unwrap();
    assert_eq!(prim.indices().unwrap().data_type(), DataType::U8);
}

#[test]
fn restart_and_out_of_range_indices_are_rejected_per_width() {
    let mut b = GltfBuilder::new();
    let big = |i: Indices| PrimitiveSpec { indices: Some(i), ..spec() };
    assert!(b.create_primitive(big(Indices::U8(vec![0, 1, 255]))).is_err()); // 255 = restart (and out of range)
    assert!(b.create_primitive(big(Indices::U16(vec![0, 1, 3]))).is_err()); // only 3 vertices
    assert!(b.create_primitive(big(Indices::U32(vec![0, 1, u32::MAX]))).is_err());
    let positions: Vec<f32> = vec![0.0; 3 * 256];
    let restart = PrimitiveSpec { positions, indices: Some(Indices::U8(vec![0, 1, 255])), ..Default::default() };
    assert!(matches!(b.create_primitive(restart), Err(GltfError::InvalidInput(m)) if m.contains("restart")));
    assert_eq!(b.gltf.accessors.as_ref().map_or(0, |a| a.len()), 0);
}

#[test]
fn material_must_exist() {
    let mut b = GltfBuilder::new();
    assert!(b.create_primitive(PrimitiveSpec { material: Some(0), ..spec() }).is_err());
    let m = b.create_basic_material(None, [1.0; 4]);
    let p = b.create_primitive(PrimitiveSpec { material: Some(m), ..spec() }).unwrap();
    assert_eq!(p.material, Some(m));
}
