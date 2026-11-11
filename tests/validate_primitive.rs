//! Validator codes for meshes and primitives. Each negative breaks a valid document through the
//! public model and the writer, and expects the stable code.

use mesh_tools::{AccessorSpec, AccessorType, Bounds, BufferViewTarget, GltfBuilder, PrimitiveSpec};

const TRI: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
const UP: [f32; 9] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];

/// A valid document with one mesh holding one indexed triangle with normals.
fn base() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let p = b
        .create_primitive(PrimitiveSpec {
            positions: TRI.to_vec(),
            normals: Some(UP.to_vec()),
            indices: Some(vec![0u16, 1, 2].into()),
            ..Default::default()
        })
        .unwrap();
    b.add_mesh_checked(None, vec![p], None).unwrap();
    assert!(b.validate().is_valid() && b.validate().warnings.is_empty());
    b
}

fn prim(b: &mut GltfBuilder) -> &mut mesh_tools::Primitive {
    &mut b.gltf.meshes.as_mut().unwrap()[0].primitives[0]
}

fn vertex_accessor<C: mesh_tools::Component>(b: &mut GltfBuilder, data: &[C], ty: AccessorType) -> usize {
    let spec = AccessorSpec::new().with_target(BufferViewTarget::ArrayBuffer).with_bounds(Bounds::Compute);
    b.add_accessor_from_slice(data, ty, spec).unwrap()
}

fn index_accessor<C: mesh_tools::Component>(b: &mut GltfBuilder, data: &[C]) -> usize {
    let spec = AccessorSpec::new().with_target(BufferViewTarget::ElementArrayBuffer);
    b.add_accessor_from_slice(data, AccessorType::Scalar, spec).unwrap()
}

#[track_caller]
fn error(b: &GltfBuilder, code: &str) {
    let r = b.validate();
    assert!(r.has_error(code), "expected error {code}, got {:#?}", r);
}

#[track_caller]
fn warning(b: &GltfBuilder, code: &str) {
    let r = b.validate();
    assert!(r.has_warning(code) && r.is_valid(), "expected only warning {code}, got {:#?}", r);
}

#[test]
fn unknown_mode_code() {
    let mut b = base();
    prim(&mut b).mode = Some(9);
    error(&b, "PRIMITIVE_MODE_INVALID");
}

#[test]
fn counts_per_mode_with_a_missing_mode_meaning_triangles() {
    let mut b = base();
    prim(&mut b).mode = Some(1); // 3 indices cannot be lines
    error(&b, "PRIMITIVE_COUNT_INVALID");
    let mut b = base();
    prim(&mut b).mode = Some(5); // strip of 3 is fine
    assert!(b.validate().is_valid());
    prim(&mut b).mode = None; // triangles: 3 is fine
    assert!(b.validate().is_valid());
    let four = index_accessor(&mut b, &[0u16, 1, 2, 0]);
    prim(&mut b).indices = Some(four); // 4 indices as (missing mode =) triangles
    error(&b, "PRIMITIVE_COUNT_INVALID");
    // Non-indexed: the vertex count is used.
    let mut b = base();
    prim(&mut b).indices = None;
    prim(&mut b).mode = Some(1);
    error(&b, "PRIMITIVE_COUNT_INVALID");
    prim(&mut b).mode = Some(0);
    assert!(b.validate().is_valid());
}

#[test]
fn attribute_names() {
    let mut b = base();
    let pos = prim(&mut b).attributes["POSITION"];
    for bad in ["FOO", "TEXCOORD", "TEXCOORD_", "COLOR_x", "position", "_"] {
        let mut b2 = base();
        prim(&mut b2).attributes.insert(bad.to_string(), pos);
        error(&b2, "PRIMITIVE_ATTRIBUTE_NAME_INVALID");
    }
    // Custom attributes start with an underscore; numbered semantics are accepted.
    prim(&mut b).attributes.insert("_BATCH".into(), pos);
    prim(&mut b).attributes.insert("TEXCOORD_12".into(), pos);
    assert!(!b.validate().has_error("PRIMITIVE_ATTRIBUTE_NAME_INVALID"));
}

#[test]
fn attribute_types_follow_the_semantic_table() {
    // (name, accessor builder) pairs that must be rejected.
    let mut cases: Vec<(&str, Box<dyn Fn(&mut GltfBuilder) -> usize>)> = Vec::new();
    cases.push(("POSITION", Box::new(|b| vertex_accessor(b, &[0.0f32; 6], AccessorType::Vec2))));
    cases.push(("NORMAL", Box::new(|b| vertex_accessor(b, &[0.0f32; 12], AccessorType::Vec4))));
    cases.push(("TANGENT", Box::new(|b| vertex_accessor(b, &[0.0f32; 9], AccessorType::Vec3))));
    cases.push(("TEXCOORD_0", Box::new(|b| vertex_accessor(b, &[0.0f32; 9], AccessorType::Vec3))));
    // Integer texcoords must be normalized.
    cases.push(("TEXCOORD_0", Box::new(|b| vertex_accessor(b, &[0u8; 6], AccessorType::Vec2))));
    cases.push(("COLOR_0", Box::new(|b| vertex_accessor(b, &[0.0f32; 6], AccessorType::Vec2))));
    cases.push(("JOINTS_0", Box::new(|b| vertex_accessor(b, &[0.0f32; 12], AccessorType::Vec4))));
    cases.push(("WEIGHTS_0", Box::new(|b| vertex_accessor(b, &[0u16; 12], AccessorType::Vec4))));
    for (name, make) in cases {
        let mut b = base();
        let acc = make(&mut b);
        prim(&mut b).attributes.insert(name.to_string(), acc);
        error(&b, "PRIMITIVE_ATTRIBUTE_TYPE");
    }
    // The valid forms are accepted: normalized u8 colors, float weights, u8 joints.
    let mut b = base();
    let uv = b.add_accessor_from_slice(&[0u8; 6], AccessorType::Vec2, AccessorSpec::new().with_normalized(true).with_target(BufferViewTarget::ArrayBuffer)).unwrap();
    prim(&mut b).attributes.insert("TEXCOORD_0".into(), uv);
    assert!(b.validate().is_valid(), "{:#?}", b.validate());
}

#[test]
fn attribute_counts_must_agree() {
    let mut b = base();
    let short = vertex_accessor(&mut b, &[0.0f32; 6], AccessorType::Vec3); // 2 vertices
    prim(&mut b).attributes.insert("NORMAL".into(), short);
    error(&b, "PRIMITIVE_ATTRIBUTE_COUNT_MISMATCH");
}

#[test]
fn index_values_are_range_checked() {
    let mut b = base();
    let bad = index_accessor(&mut b, &[0u16, 1, 3]); // only 3 vertices
    prim(&mut b).indices = Some(bad);
    error(&b, "PRIMITIVE_INDEX_OUT_OF_RANGE");
    let mut b = base();
    let bad = index_accessor(&mut b, &[0u32, 1, 4_000_000_000]);
    prim(&mut b).indices = Some(bad);
    error(&b, "PRIMITIVE_INDEX_OUT_OF_RANGE");
}

#[test]
fn restart_values_are_rejected_for_every_index_width() {
    let mut b = GltfBuilder::new();
    let pos = vertex_accessor(&mut b, &vec![0.0f32; 3 * 65_537], AccessorType::Vec3);
    let mut p = mesh_tools::Primitive::default();
    p.attributes.insert("POSITION".into(), pos);
    p.mode = Some(0);
    b.add_mesh(None, vec![p]);
    let cases: [usize; 3] = [
        index_accessor(&mut b, &[0u8, 1, 255]),
        index_accessor(&mut b, &[0u16, 1, 65_535]),
        index_accessor(&mut b, &[0u32, 1, u32::MAX]),
    ];
    for (i, acc) in cases.into_iter().enumerate() {
        prim(&mut b).indices = Some(acc);
        let r = b.validate();
        assert!(r.has_error("PRIMITIVE_INDEX_RESTART"), "width case {i}: {r:#?}");
        // The u8 and u16 restart values are valid vertex numbers here, so they are not also out of range.
        if i < 2 {
            assert!(!r.has_error("PRIMITIVE_INDEX_OUT_OF_RANGE"), "{r:#?}");
        }
    }
    // 65535 as a u32 index is legal.
    let ok = index_accessor(&mut b, &[0u32, 1, 65_535]);
    prim(&mut b).indices = Some(ok);
    assert!(b.validate().is_valid(), "{:#?}", b.validate());
}

#[test]
fn tangent_w_must_be_plus_or_minus_one() {
    let mut b = base();
    let t = vertex_accessor(&mut b, &[1.0f32, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, 0.5], AccessorType::Vec4);
    prim(&mut b).attributes.insert("TANGENT".into(), t);
    error(&b, "PRIMITIVE_TANGENT_W_INVALID");
    let ok = vertex_accessor(&mut b, &[1.0f32, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, 1.0], AccessorType::Vec4);
    prim(&mut b).attributes.insert("TANGENT".into(), ok);
    assert!(b.validate().is_valid());
}

/// Sparse tangent/normal accessors are not judged on their base data alone: the base here has
/// w = 0 and a non-unit normal, but the substituted values are fine.
#[test]
fn sparse_tangents_and_normals_are_not_checked_against_their_base() {
    let mut b = base();
    let t = b
        .add_sparse_accessor::<f32>(
            3,
            AccessorType::Vec4,
            Some(&[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0][..]),
            mesh_tools::SparseSpec::new(vec![0, 1, 2], vec![1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, 1.0]),
            AccessorSpec::new().with_target(BufferViewTarget::ArrayBuffer),
        )
        .unwrap();
    let n = b
        .add_sparse_accessor::<f32>(
            3,
            AccessorType::Vec3,
            Some(&[0.0f32; 9][..]),
            mesh_tools::SparseSpec::new(vec![0, 1, 2], UP.to_vec()),
            AccessorSpec::new().with_target(BufferViewTarget::ArrayBuffer),
        )
        .unwrap();
    prim(&mut b).attributes.insert("TANGENT".into(), t);
    prim(&mut b).attributes.insert("NORMAL".into(), n);
    let r = b.validate();
    assert!(r.is_valid() && r.warnings.is_empty(), "{r:#?}");
}

#[test]
fn warnings() {
    // Tangent without a normal.
    let mut b = base();
    let t = vertex_accessor(&mut b, &[1.0f32, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0], AccessorType::Vec4);
    prim(&mut b).attributes.insert("TANGENT".into(), t);
    prim(&mut b).attributes.remove("NORMAL");
    warning(&b, "PRIMITIVE_TANGENT_WITHOUT_NORMAL");

    // TEXCOORD_1 without TEXCOORD_0 (6 floats = 3 VEC2 elements).
    let mut b = base();
    let uv = vertex_accessor(&mut b, &[0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0], AccessorType::Vec2);
    prim(&mut b).attributes.insert("TEXCOORD_1".into(), uv);
    warning(&b, "PRIMITIVE_ATTRIBUTE_SET_GAP");

    // Normals that are not unit length.
    let mut b = base();
    let n = vertex_accessor(&mut b, &[0.0f32, 0.0, 2.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0], AccessorType::Vec3);
    prim(&mut b).attributes.insert("NORMAL".into(), n);
    warning(&b, "PRIMITIVE_NORMAL_NOT_UNIT");

    // A primitive without POSITION.
    let mut b = base();
    prim(&mut b).attributes.remove("POSITION");
    warning(&b, "MESH_PRIMITIVE_NO_POSITION");
}

#[test]
fn every_builder_made_primitive_mode_validates_clean() {
    let mut b = GltfBuilder::new();
    let prims = vec![
        b.create_points(&TRI, None).unwrap(),
        b.create_lines(&TRI, Some(&[0, 1, 1, 2]), None).unwrap(),
        b.create_line_strip(&TRI, true, None, None).unwrap(),
        b.create_triangle_strip(&TRI, None, None).unwrap(),
        b.create_triangle_fan(&TRI, None, None).unwrap(),
    ];
    b.add_mesh_checked(None, prims, None).unwrap();
    let r = b.validate();
    assert!(r.is_valid() && r.warnings.is_empty(), "{r:#?}");
}
