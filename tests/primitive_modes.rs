//! Primitive modes: helper oracles (hand-derived), mode metadata, and every mode read back
//! through the independent `gltf` crate with exact index order.

mod common;

use gltf::mesh::Mode;
use mesh_tools::{fan_to_triangles, strip_to_triangles, GltfBuilder, GltfError, Primitive, PrimitiveMode};

/// Five vertices: a unit square plus one extra point.
const P5: [f32; 15] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 2.0, 1.0, 0.0];

fn counts(b: &GltfBuilder) -> (usize, usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.gltf.meshes.as_ref().map_or(0, |a| a.len()),
        b.to_glb_bytes_unchecked().unwrap().len(),
    )
}

/// Add `p` as a one-primitive mesh, export, and return (mode, positions, indices) from the reader.
fn read_back(mut b: GltfBuilder, p: Primitive) -> (Mode, Vec<[f32; 3]>, Option<Vec<u32>>) {
    b.add_mesh_checked(None, vec![p], None).unwrap();
    let glb = common::parse_glb(&b);
    let prim = glb.meshes().next().unwrap().primitives().next().unwrap();
    let r = prim.reader(|_| glb.blob.as_deref());
    (prim.mode(), r.read_positions().unwrap().collect(), r.read_indices().map(|i| i.into_u32().collect()))
}

#[test]
fn strip_to_triangles_follows_the_spec_winding() {
    // Strip 0..4: (0,1,2), then odd triangles swap the first two: (2,1,3), then (2,3,4).
    assert_eq!(strip_to_triangles(&[0, 1, 2, 3, 4]).unwrap(), vec![0, 1, 2, 2, 1, 3, 2, 3, 4]);
    // Indices are arbitrary vertex numbers, not positions; degenerate triangles are kept.
    assert_eq!(strip_to_triangles(&[7, 7, 9, 4]).unwrap(), vec![7, 7, 9, 9, 7, 4]);
    assert_eq!(strip_to_triangles(&[0, 1, 2]).unwrap(), vec![0, 1, 2]);
}

#[test]
fn fan_to_triangles_shares_the_first_vertex() {
    assert_eq!(fan_to_triangles(&[0, 1, 2, 3, 4]).unwrap(), vec![0, 1, 2, 0, 2, 3, 0, 3, 4]);
    assert_eq!(fan_to_triangles(&[5, 8, 9, 6]).unwrap(), vec![5, 8, 9, 5, 9, 6]);
}

#[test]
fn conversion_helpers_reject_fewer_than_three_indices() {
    for f in [strip_to_triangles, fan_to_triangles] {
        assert!(matches!(f(&[]), Err(GltfError::InvalidInput(_))));
        assert!(matches!(f(&[0, 1]), Err(GltfError::InvalidInput(m)) if m.contains("at least 3")));
    }
}

#[test]
fn mode_metadata_matches_the_gltf_table() {
    use PrimitiveMode::*;
    // (mode, code, min count, count multiple)
    let table = [
        (Points, 0, 1, 1),
        (Lines, 1, 2, 2),
        (LineLoop, 2, 2, 1),
        (LineStrip, 3, 2, 1),
        (Triangles, 4, 3, 3),
        (TriangleStrip, 5, 3, 1),
        (TriangleFan, 6, 3, 1),
    ];
    for (mode, code, min, multiple) in table {
        assert_eq!((mode.code(), mode.min_count(), mode.count_multiple()), (code, min, multiple), "{mode:?}");
        assert_eq!(PrimitiveMode::from_code(code).unwrap(), mode);
    }
    assert_eq!(PrimitiveMode::default(), Triangles);
    assert!(matches!(PrimitiveMode::from_code(7), Err(GltfError::InvalidInput(_))));
    // Counts: 4 lines indices are fine, 3 are not; a strip needs only 3.
    assert!(Lines.is_valid_count(4) && !Lines.is_valid_count(3) && !Lines.is_valid_count(0));
    assert!(TriangleStrip.is_valid_count(7) && !TriangleStrip.is_valid_count(2));
}

#[test]
fn mode_enum_treats_a_missing_mode_as_triangles_and_rejects_unknown_codes() {
    let mut p = Primitive::default();
    assert_eq!(p.mode_enum().unwrap(), PrimitiveMode::Triangles);
    p.mode = Some(5);
    assert_eq!(p.mode_enum().unwrap(), PrimitiveMode::TriangleStrip);
    p.mode = Some(9);
    assert!(p.mode_enum().is_err());
}

#[test]
fn points_round_trip() {
    let mut b = GltfBuilder::new();
    let p = b.create_points(&P5, None).unwrap();
    let (mode, pos, idx) = read_back(b, p);
    assert_eq!(mode, Mode::Points);
    assert_eq!(pos.len(), 5);
    assert_eq!(pos[4], [2.0, 1.0, 0.0]);
    assert_eq!(idx, None);
}

#[test]
fn lines_round_trip_indexed_and_not() {
    let mut b = GltfBuilder::new();
    let p = b.create_lines(&P5[..12], Some(&[0, 2, 1, 3]), None).unwrap();
    let (mode, pos, idx) = read_back(b, p);
    assert_eq!((mode, pos.len(), idx), (Mode::Lines, 4, Some(vec![0, 2, 1, 3])));

    let mut b = GltfBuilder::new();
    let p = b.create_lines(&P5[..12], None, None).unwrap(); // 4 vertices = 2 segments
    let (mode, _, idx) = read_back(b, p);
    assert_eq!((mode, idx), (Mode::Lines, None));
}

#[test]
fn line_strip_and_loop_round_trip() {
    for (closed, expect) in [(false, Mode::LineStrip), (true, Mode::LineLoop)] {
        let mut b = GltfBuilder::new();
        let p = b.create_line_strip(&P5, closed, Some(&[4, 3, 2, 1, 0]), None).unwrap();
        let (mode, _, idx) = read_back(b, p);
        assert_eq!((mode, idx), (expect, Some(vec![4, 3, 2, 1, 0])));
    }
}

#[test]
fn triangle_strip_and_fan_round_trip() {
    let mut b = GltfBuilder::new();
    let p = b.create_triangle_strip(&P5, Some(&[0, 1, 3, 2, 4]), None).unwrap();
    let (mode, _, idx) = read_back(b, p);
    assert_eq!((mode, idx), (Mode::TriangleStrip, Some(vec![0, 1, 3, 2, 4])));

    let mut b = GltfBuilder::new();
    let p = b.create_triangle_fan(&P5, None, None).unwrap(); // non-indexed fan over all 5 vertices
    let (mode, pos, idx) = read_back(b, p);
    assert_eq!((mode, pos.len(), idx), (Mode::TriangleFan, 5, None));
}

#[test]
fn strip_converted_to_triangles_matches_the_strip_it_came_from() {
    let tris = strip_to_triangles(&[0, 1, 3, 2, 4]).unwrap();
    // (0,1,3), (3,1,2), (3,2,4)
    assert_eq!(tris, vec![0, 1, 3, 3, 1, 2, 3, 2, 4]);
    let mut b2 = GltfBuilder::new();
    let spec = mesh_tools::PrimitiveSpec { positions: P5.to_vec(), indices: Some(tris.clone().into()), ..Default::default() };
    let p = b2.create_primitive(spec).unwrap();
    let (mode, _, idx) = read_back(b2, p);
    assert_eq!((mode, idx), (Mode::Triangles, Some(tris)));
}

#[test]
fn wrong_counts_for_a_mode_are_rejected_and_leave_the_document_unchanged() {
    let mut b = GltfBuilder::new();
    let before = counts(&b);
    // 5 vertices cannot be 2-vertex lines; 3 indices neither.
    assert!(matches!(b.create_lines(&P5, None, None), Err(GltfError::InvalidInput(m)) if m.contains("multiple of 2")));
    assert!(b.create_lines(&P5, Some(&[0, 1, 2]), None).is_err());
    // Strips and fans need 3; a line strip needs 2; points need 1 (empty is rejected as no positions).
    assert!(b.create_triangle_strip(&P5, Some(&[0, 1]), None).is_err());
    assert!(b.create_triangle_fan(&P5[..6], None, None).is_err());
    assert!(b.create_line_strip(&P5[..3], false, None, None).is_err());
    assert!(b.create_points(&[], None).is_err());
    // An index out of range for the vertices, and the restart value.
    assert!(b.create_lines(&P5, Some(&[0, 5]), None).is_err());
    assert!(b.create_lines(&P5, Some(&[0, 65535]), None).is_err());
    assert_eq!(counts(&b), before);
}
