mod common;

use mesh_tools::{Accessor, Buffer, BufferView, GltfBuilder};

/// FNV-1a 64-bit, a stable dependency-free fingerprint for golden bytes.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn glb(b: &GltfBuilder) -> Vec<u8> {
    b.to_glb_bytes().unwrap()
}

/// BIN chunk payload of a GLB produced by this crate (JSON chunk first).
fn bin_chunk(bytes: &[u8]) -> &[u8] {
    let jl = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let at = 20 + jl;
    let bl = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    &bytes[at + 8..at + 8 + bl]
}

#[track_caller]
fn assert_error(b: &GltfBuilder, code: &str) {
    let r = b.validate();
    assert!(r.has_error(code), "expected error {code}, got: {:#?}", r.errors);
}

#[track_caller]
fn assert_no_error(b: &GltfBuilder, code: &str) {
    let r = b.validate();
    assert!(!r.has_error(code), "unexpected {code}: {:#?}", r.errors);
}

// ---- golden bytes (captured from 0.7.0 output; must never change) ----

#[test]
fn golden_box_plane_cylinder_are_byte_stable() {
    let mut b = GltfBuilder::new();
    b.create_box(2.0);
    let g = glb(&b);
    assert_eq!((g.len(), fnv(&g)), GOLDEN_BOX, "box GLB changed");

    let mut b = GltfBuilder::new();
    b.create_plane(2.0, 3.0, 2, 3, None);
    let g = glb(&b);
    assert_eq!((g.len(), fnv(&g)), GOLDEN_PLANE, "plane GLB changed");

    let mut b = GltfBuilder::new();
    b.create_cylinder(1.0, 2.0, 3.0, 8, 2, false, None);
    let g = glb(&b);
    assert_eq!((g.len(), fnv(&g)), GOLDEN_CYLINDER, "cylinder GLB changed");
}

const GOLDEN_BOX: (usize, u64) = (1188, 0x2cf16543e892b344);
const GOLDEN_PLANE: (usize, u64) = (1320, 0xffe1e3d5ccfcdc19);
const GOLDEN_CYLINDER: (usize, u64) = (2880, 0xe7a29dc4f8b555e2);

// ---- little-endian safety ----

#[test]
fn box_binary_equals_explicit_little_endian_concatenation() {
    let mut b = GltfBuilder::new();
    b.create_box(2.0);
    let bytes = glb(&b);
    let doc = gltf::Gltf::from_slice(&bytes).unwrap();
    let bin = bin_chunk(&bytes);
    // Rebuild the expected BIN from the reader's own decoded accessors, little-endian.
    let reader_buf = gltf::buffer::Data(bin.to_vec());
    let mut expected: Vec<u8> = Vec::new();
    let mut check = |acc: gltf::Accessor| {
        let view = acc.view().unwrap();
        while expected.len() < view.offset() {
            expected.push(0);
        }
        let r = gltf::accessor::util::Iter::<[f32; 3]>::new(acc.clone(), |_| Some(&reader_buf.0[..]));
        if let Some(it) = r {
            for v in it {
                for c in v {
                    expected.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
    };
    let prim = doc.meshes().next().unwrap().primitives().next().unwrap();
    // POSITION is the first view in the buffer.
    check(prim.get(&gltf::Semantic::Positions).unwrap());
    assert_eq!(&bin[..expected.len()], &expected[..]);
    // Hand oracle: a 2.0 box has corner (-1,-1,1) first; -1.0f32 LE is 00 00 80 BF, 1.0f32 is 00 00 80 3F.
    assert_eq!(&bin[..12], &[0, 0, 0x80, 0xBF, 0, 0, 0x80, 0xBF, 0, 0, 0x80, 0x3F]);
}

// ---- empty document / lazy buffer ----

#[test]
fn empty_document_has_no_buffer_and_still_exports() {
    let b = GltfBuilder::new();
    assert!(b.gltf.buffers.as_ref().map_or(true, |v| v.is_empty()));
    assert!(b.validate().is_valid(), "{:?}", b.validate().errors);
    let bytes = glb(&b);
    let doc = gltf::Gltf::from_slice(&bytes).unwrap();
    assert_eq!(doc.buffers().count(), 0);
    assert!(doc.blob.is_none());
}

#[test]
fn first_data_write_creates_buffer_zero_with_matching_length() {
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    let bytes = glb(&b);
    let doc = gltf::Gltf::from_slice(&bytes).unwrap();
    assert_eq!(doc.buffers().count(), 1);
    assert_eq!(doc.buffers().next().unwrap().length(), bin_chunk(&bytes).len());
}

#[test]
fn buffer_empty_when_buffer_declares_zero_bytes() {
    let mut b = GltfBuilder::new();
    b.gltf.buffers = Some(vec![Buffer::default()]);
    assert_error(&b, "BUFFER_EMPTY");
}

#[test]
fn buffer_length_mismatch_covers_every_buffer_index() {
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    b.gltf.buffers.as_mut().unwrap().push({ let mut x = Buffer::default(); x.byte_length = 16; x });
    assert_error(&b, "BUFFER_URI_MISSING");
    // an external (uri) buffer cannot be checked and is accepted
    b.gltf.buffers.as_mut().unwrap()[1].uri = Some("x.bin".into());
    assert_no_error(&b, "BUFFER_URI_MISSING");
    assert_no_error(&b, "BUFFER_LENGTH_MISMATCH");
}

// ---- names ----

#[test]
fn names_on_buffer_view_accessor_buffer_round_trip_through_the_reader() {
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    b.set_buffer_name(0, Some("buf".into())).unwrap();
    b.set_buffer_view_name(0, Some("view".into())).unwrap();
    b.set_accessor_name(0, Some("acc".into())).unwrap();
    let doc = common::parse_glb(&b);
    assert_eq!(doc.buffers().next().unwrap().name(), Some("buf"));
    assert_eq!(doc.views().next().unwrap().name(), Some("view"));
    assert_eq!(doc.accessors().next().unwrap().name(), Some("acc"));
    // clearing and out-of-range
    b.set_accessor_name(0, None).unwrap();
    assert!(b.gltf.accessors.as_ref().unwrap()[0].name.is_none());
    let before = b.gltf.buffers.as_ref().unwrap().len();
    assert!(b.set_buffer_name(9, Some("x".into())).is_err());
    assert!(b.set_buffer_view_name(99, None).is_err());
    assert!(b.set_accessor_name(99, None).is_err());
    assert_eq!(b.gltf.buffers.as_ref().unwrap().len(), before);
}

// ---- view/accessor rules ----

/// A document with one 4-byte-tight vec3 float accessor on its own view (36 bytes, 3 verts).
fn doc_with_view(view: BufferView, acc: Accessor) -> GltfBuilder {
    // A plane supplies a few KiB of real buffer data; the views/accessors are then replaced.
    let mut b = GltfBuilder::new();
    b.create_plane(1.0, 1.0, 8, 8, None);
    b.gltf.meshes = None;
    b.gltf.buffer_views = Some(vec![view]);
    b.gltf.accessors = Some(vec![acc]);
    b
}

// The model structs are #[non_exhaustive]: build them from Default.
fn view(offset: usize, len: usize) -> BufferView {
    let mut v = BufferView::default();
    v.byte_offset = offset;
    v.byte_length = len;
    v
}

fn view_with(offset: usize, len: usize, stride: Option<usize>, target: Option<usize>) -> BufferView {
    let mut v = view(offset, len);
    v.byte_stride = stride;
    v.target = target;
    v
}

fn acc(component: usize, ty: &str, count: usize) -> Accessor {
    let mut a = Accessor::default();
    a.buffer_view = Some(0);
    a.component_type = component;
    a.count = count;
    a.type_ = ty.into();
    a
}

#[test]
fn view_length_zero_and_target_invalid() {
    let mut b = doc_with_view(view(0, 0), acc(5126, "SCALAR", 1));
    assert_error(&b, "BUFFER_VIEW_LENGTH_ZERO");
    b.gltf.buffer_views.as_mut().unwrap()[0] = view_with(0, 8, None, Some(1234));
    assert_error(&b, "BUFFER_VIEW_TARGET_INVALID");
}

#[test]
fn view_stride_rules() {
    let stride = |s: usize| view_with(0, 128, Some(s), Some(34962));
    let b = doc_with_view(stride(2), acc(5126, "SCALAR", 1));
    assert_error(&b, "BUFFER_VIEW_STRIDE_RANGE");
    let b = doc_with_view(stride(256), acc(5126, "SCALAR", 1));
    assert_error(&b, "BUFFER_VIEW_STRIDE_RANGE");
    let b = doc_with_view(stride(6), acc(5123, "SCALAR", 1));
    assert_error(&b, "BUFFER_VIEW_STRIDE_ALIGNMENT");
    let b = doc_with_view(stride(5), acc(5123, "SCALAR", 1));
    assert_error(&b, "ACCESSOR_STRIDE_ALIGNMENT");
    // 8 is smaller than a vec3 float (12)
    let b = doc_with_view(stride(8), acc(5126, "VEC3", 2));
    assert_error(&b, "BUFFER_VIEW_STRIDE_TOO_SMALL");
    let b = doc_with_view(stride(16), acc(5126, "VEC3", 2));
    assert!(b.validate().is_valid(), "{:?}", b.validate().errors);
    // stride on index data
    let b = doc_with_view(view_with(0, 64, Some(8), Some(34963)), acc(5125, "SCALAR", 2));
    assert_error(&b, "BUFFER_VIEW_STRIDE_ON_NON_VERTEX");
}

#[test]
fn view_target_must_match_use() {
    let mut b = doc_with_view(view_with(0, 12, None, Some(34962)), acc(5125, "SCALAR", 3));
    // accessor 0 (in an ARRAY_BUFFER view) used as indices
    b.gltf.accessors.as_mut().unwrap().push(acc(5126, "VEC3", 1));
    let mut prim = mesh_tools::Primitive::default();
    prim.attributes.insert("POSITION".into(), 1);
    prim.indices = Some(0);
    b.gltf.meshes = Some(vec![mesh_tools::Mesh { primitives: vec![prim], ..Default::default() }]);
    assert_error(&b, "BUFFER_VIEW_TARGET_MISMATCH");
    // vertex attribute in an ELEMENT_ARRAY_BUFFER view
    let mut b = doc_with_view(view_with(0, 12, None, Some(34963)), acc(5126, "VEC3", 1));
    let mut prim = mesh_tools::Primitive::default();
    prim.attributes.insert("POSITION".into(), 0);
    b.gltf.meshes = Some(vec![mesh_tools::Mesh { primitives: vec![prim], ..Default::default() }]);
    assert_error(&b, "BUFFER_VIEW_TARGET_MISMATCH");
}

#[test]
fn accessor_alignment_checks() {
    let mut b = doc_with_view(view(0, 64), acc(5126, "SCALAR", 2));
    b.gltf.accessors.as_mut().unwrap()[0].byte_offset = Some(2);
    assert_error(&b, "ACCESSOR_OFFSET_ALIGNMENT");
    // view offset misaligned for the component size
    let b = doc_with_view(view(2, 64), acc(5126, "SCALAR", 2));
    assert_error(&b, "ACCESSOR_OFFSET_ALIGNMENT");
    let b = doc_with_view(view(2, 64), acc(5123, "SCALAR", 2));
    assert_no_error(&b, "ACCESSOR_OFFSET_ALIGNMENT");
}

#[test]
fn view_offset_alignment_is_a_warning_for_target_views() {
    let b = doc_with_view(view_with(2, 64, None, Some(34962)), acc(5123, "SCALAR", 2));
    assert!(b.validate().has_warning("BUFFER_VIEW_OFFSET_ALIGNMENT"), "{:?}", b.validate().warnings);
}

#[test]
fn padded_matrix_element_sizes() {
    // (component, type, bytes per element)
    for (c, t, n) in [
        (5120, "MAT2", 8), (5121, "MAT2", 8), (5120, "MAT3", 12), (5121, "MAT3", 12),
        (5122, "MAT3", 24), (5123, "MAT3", 24), (5122, "MAT2", 8), (5126, "MAT3", 36), (5120, "MAT4", 16),
    ] {
        // exactly one element fits in n bytes, not in n-4
        let ok = doc_with_view(view(0, n), acc(c, t, 1));
        assert_no_error(&ok, "ACCESSOR_RANGE");
        let bad = doc_with_view(view(0, n - 4), acc(c, t, 1));
        assert_error(&bad, "ACCESSOR_RANGE");
    }
}

// ---- hostile hand-edited values ----

#[test]
fn usize_max_values_error_instead_of_panicking() {
    let max = usize::MAX;
    let mut b = doc_with_view(view(max, max), acc(5126, "VEC3", max));
    assert_error(&b, "BUFFER_VIEW_RANGE");
    b.gltf.buffer_views.as_mut().unwrap()[0] = view_with(0, 64, Some(max), None);
    b.gltf.accessors.as_mut().unwrap()[0].byte_offset = Some(max);
    let r = b.validate();
    assert!(r.has_error("ACCESSOR_RANGE") || r.has_error("BUFFER_VIEW_STRIDE_RANGE"));
    b.gltf.accessors.as_mut().unwrap()[0].count = max;
    b.gltf.accessors.as_mut().unwrap()[0].byte_offset = None;
    assert_error(&b, "ACCESSOR_RANGE");
    assert!(b.to_glb_bytes().is_err());
    b.gltf.buffers.as_mut().unwrap()[0].byte_length = max;
    let _ = b.validate();
}

#[test]
fn usize_max_accessor_used_as_joints_does_not_panic() {
    let b = doc_with_view(view_with(usize::MAX, 8, Some(usize::MAX), None), acc(5121, "VEC4", usize::MAX));
    let _ = b.validate();
}


#[test]
fn stride_on_animation_and_ibm_accessors_is_an_error() {
    use mesh_tools::{Animation, AnimationSampler, Skin};
    // accessor 0 sits in a strided view; used as sampler input -> error
    let mut b = doc_with_view(view_with(0, 64, Some(8), None), acc(5126, "SCALAR", 2));
    let mut s = AnimationSampler::default();
    s.input = 0;
    s.output = 0;
    let mut an = Animation::default();
    an.samplers = Some(vec![s]);
    b.gltf.animations = Some(vec![an]);
    assert_error(&b, "BUFFER_VIEW_STRIDE_ON_NON_VERTEX");
    // same view as inverse bind matrices
    let mut b = doc_with_view(view_with(0, 128, Some(64), None), acc(5126, "MAT4", 1));
    let mut sk = Skin::default();
    sk.inverse_bind_matrices = Some(0);
    b.gltf.skins = Some(vec![sk]);
    assert_error(&b, "BUFFER_VIEW_STRIDE_ON_NON_VERTEX");
}
