//! Plane generator and checkerboard textures. Geometry is checked from the output positions.

use image::{GenericImageView, Rgba};
use mesh_tools::primitives::generate_plane;
use mesh_tools::texture::{
    create_colored_checkerboard, create_uv_test_pattern, generate_checkerboard, TextureError,
};

#[test]
fn plane_faces_up_counter_clockwise_from_plus_y() {
    let (p, idx, n, _) = generate_plane(4.0, 2.0, 3, 2);
    assert_eq!(idx.len(), 3 * 2 * 2);
    for t in &idx {
        let [a, b, c] = [t.a, t.b, t.c].map(|i| p[i as usize]);
        let (u, v) = ((b.x - a.x, b.y - a.y, b.z - a.z), (c.x - a.x, c.y - a.y, c.z - a.z));
        let ny = u.2 * v.0 - u.0 * v.2; // y component of u x v
        assert!(ny > 0.0, "triangle winds clockwise seen from +Y: {a:?} {b:?} {c:?}");
    }
    assert!(n.iter().all(|v| (v.x, v.y, v.z) == (0.0, 1.0, 0.0)));
}

#[test]
fn plane_covers_the_requested_extent() {
    let (p, _, _, uv) = generate_plane(4.0, 2.0, 3, 2);
    assert_eq!(p.len(), 4 * 3);
    let (min_x, max_x) = p.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v.x), hi.max(v.x)));
    let (min_z, max_z) = p.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v.z), hi.max(v.z)));
    assert_eq!((min_x, max_x, min_z, max_z), (-2.0, 2.0, -1.0, 1.0));
    assert!(uv.iter().all(|t| (0.0..=1.0).contains(&t.x) && (0.0..=1.0).contains(&t.y)));
}

#[test]
fn plane_with_zero_segments_is_a_single_finite_quad() {
    for (w, d) in [(0, 0), (0, 3), (3, 0)] {
        let (p, idx, n, uv) = generate_plane(2.0, 2.0, w, d);
        let (gw, gd) = (w.max(1), d.max(1));
        assert_eq!(p.len(), (gw + 1) * (gd + 1), "{w}x{d}");
        assert_eq!(idx.len(), gw * gd * 2);
        assert_eq!((n.len(), uv.len()), (p.len(), p.len()));
        assert!(p.iter().all(|v| v.x.is_finite() && v.y.is_finite() && v.z.is_finite()), "{w}x{d}");
        assert!(uv.iter().all(|t| t.x.is_finite() && t.y.is_finite()), "{w}x{d}");
    }
}

fn is_invalid_param(e: TextureError) -> bool {
    matches!(e, TextureError::InvalidParameter(_))
}

#[test]
fn checkerboard_zero_cell_size_is_an_error_not_a_panic() {
    let c = Rgba([255, 255, 255, 255]);
    assert!(is_invalid_param(generate_checkerboard(8, 8, 0, c, c).unwrap_err()));
    assert!(is_invalid_param(create_colored_checkerboard(8, 8, 0, [0; 3], [1; 3]).unwrap_err()));
}

#[test]
fn uv_test_pattern_too_small_for_an_8x8_grid_is_an_error() {
    assert!(is_invalid_param(create_uv_test_pattern(7, 64).unwrap_err()));
    assert!(create_uv_test_pattern(8, 8).is_ok());
}

#[test]
fn checkerboard_pixels_alternate_by_cell() {
    let (a, b) = (Rgba([1, 2, 3, 255]), Rgba([9, 8, 7, 255]));
    let img = generate_checkerboard(8, 4, 2, a, b).unwrap();
    assert_eq!(img.dimensions(), (8, 4));
    for (x, y, want) in [(0, 0, a), (1, 1, a), (2, 0, b), (0, 2, b), (2, 2, a), (6, 0, b), (6, 2, a)] {
        assert_eq!(img.get_pixel(x, y), want, "pixel ({x},{y})");
    }
}

#[test]
fn builder_reports_zero_cell_size_as_an_error() {
    let mut builder = mesh_tools::GltfBuilder::new();
    assert!(builder.create_checkerboard_texture(8, 8, 0, [0; 3], [255; 3]).is_err());
    assert!(builder.create_uv_test_texture(4, 4).is_err());
    assert!(builder.gltf.textures.is_none() || builder.gltf.textures.as_ref().unwrap().is_empty());
}
