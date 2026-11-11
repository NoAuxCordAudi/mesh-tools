//! Cylinder and cone generators, checked against geometry computed independently from the
//! output positions (face normals via cross products), not against the generator's own logic.

use mesh_tools::primitives::{generate_cone, generate_cylinder};

type Mesh = (
    Vec<mesh_tools::compat::Point3<f32>>,
    Vec<mesh_tools::Triangle>,
    Vec<mesh_tools::compat::Vector3<f32>>,
    Vec<mesh_tools::compat::Vector2<f32>>,
);

fn tri(m: &Mesh, t: &mesh_tools::Triangle) -> [[f32; 3]; 3] {
    [t.a, t.b, t.c].map(|i| {
        let p = &m.0[i as usize];
        [p.x, p.y, p.z]
    })
}

/// Unnormalized geometric normal (counter-clockwise = front face).
fn face_normal(p: [[f32; 3]; 3]) -> [f32; 3] {
    let u = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let v = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn centroid(p: [[f32; 3]; 3]) -> [f32; 3] {
    [0, 1, 2].map(|k| (p[0][k] + p[1][k] + p[2][k]) / 3.0)
}

fn area(n: [f32; 3]) -> f32 {
    dot(n, n).sqrt() / 2.0
}

/// All indices must address real vertices, and all attribute arrays must have equal length.
fn assert_well_formed(m: &Mesh) {
    let n = m.0.len();
    assert_eq!(m.2.len(), n, "normals length");
    assert_eq!(m.3.len(), n, "uvs length");
    for t in &m.1 {
        for i in [t.a, t.b, t.c] {
            assert!((i as usize) < n, "index {i} out of range for {n} vertices");
        }
    }
}

/// Each face must wind counter-clockwise seen from the side its vertex normals point to,
/// and each vertex normal must be a unit vector.
fn assert_faces_agree_with_vertex_normals(m: &Mesh) {
    for t in &m.1 {
        let p = tri(m, t);
        let n = face_normal(p);
        let avg: [f32; 3] = [0, 1, 2].map(|k| {
            [t.a, t.b, t.c].iter().map(|&i| {
                let v = &m.2[i as usize];
                [v.x, v.y, v.z][k]
            }).sum::<f32>()
        });
        assert!(dot(n, avg) > 0.0, "face {p:?} winds against its vertex normals");
    }
    for v in &m.2 {
        let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
        assert!((len - 1.0).abs() < 1e-5, "non-unit normal {v:?}");
    }
}

#[test]
fn closed_cylinder_is_well_formed() {
    let m = generate_cylinder(1.0, 1.0, 2.0, 8, 2, false);
    assert_well_formed(&m);
    // sides: 9*3 vertices; each cap: centre + 9 rim vertices
    assert_eq!(m.0.len(), 27 + 10 + 10);
}

#[test]
fn closed_cylinder_triangles_face_outward() {
    let m = generate_cylinder(1.0, 1.0, 2.0, 8, 2, false);
    for t in &m.1 {
        let p = tri(&m, t);
        let n = face_normal(p);
        let c = centroid(p);
        let on_top = p.iter().all(|v| (v[1] - 1.0).abs() < 1e-6);
        let on_bottom = p.iter().all(|v| (v[1] + 1.0).abs() < 1e-6);
        if on_top {
            assert!(n[1] > 0.0, "top cap faces down: {p:?}");
        } else if on_bottom {
            assert!(n[1] < 0.0, "bottom cap faces up: {p:?}");
        } else {
            assert!(dot(n, [c[0], 0.0, c[2]]) > 0.0, "side faces inward: {p:?}");
        }
    }
}

#[test]
fn closed_cylinder_vertex_normals_match_faces() {
    assert_faces_agree_with_vertex_normals(&generate_cylinder(1.0, 1.0, 2.0, 8, 2, false));
}

#[test]
fn caps_use_their_own_centre_vertex() {
    let m = generate_cylinder(1.0, 1.0, 2.0, 8, 1, false);
    // Every cap triangle must touch exactly one vertex on the axis (x = z = 0).
    for t in &m.1 {
        let p = tri(&m, t);
        if p.iter().all(|v| v[1] == p[0][1]) {
            let on_axis = p.iter().filter(|v| v[0] == 0.0 && v[2] == 0.0).count();
            assert_eq!(on_axis, 1, "cap triangle without its centre vertex: {p:?}");
        }
    }
}

#[test]
fn open_cylinder_has_no_caps() {
    let m = generate_cylinder(1.0, 1.0, 2.0, 8, 1, true);
    assert_well_formed(&m);
    assert_eq!(m.0.len(), 18);
    assert_eq!(m.1.len(), 16);
}

#[test]
fn sloped_side_normals_are_perpendicular_to_the_surface() {
    // Frustum: bottom radius 2, top radius 1, height 2. The slant direction at angle 0 is
    // (rt - rb, h) = (-1, 2) in (x, y); the normal must be perpendicular to it and point outward.
    let m = generate_cylinder(1.0, 2.0, 2.0, 8, 1, true);
    let (p, n) = (&m.0, &m.2);
    for i in 0..p.len() {
        let theta = p[i].z.atan2(p[i].x);
        let slant = [-theta.cos() * 1.0, 2.0, -theta.sin() * 1.0];
        let normal = [n[i].x, n[i].y, n[i].z];
        assert!(dot(normal, slant).abs() < 1e-5, "normal {normal:?} not perpendicular to slant at vertex {i}");
        assert!(normal[0] * theta.cos() + normal[2] * theta.sin() > 0.0, "normal points inward at vertex {i}");
    }
    assert_faces_agree_with_vertex_normals(&m);
}

#[test]
fn cone_is_closed_outward_with_no_degenerate_triangles() {
    let m = generate_cone(1.0, 2.0, 8, 2, false);
    assert_well_formed(&m);
    assert_faces_agree_with_vertex_normals(&m);
    // No zero-area triangles anywhere: neither an apex cap nor collapsed side faces.
    for t in &m.1 {
        let p = tri(&m, t);
        assert!(area(face_normal(p)) > 1e-6, "zero-area triangle: {p:?}");
    }
    // Base cap faces down; lateral faces face outward (positive radial component) and upward.
    for t in &m.1 {
        let p = tri(&m, t);
        let n = face_normal(p);
        if p.iter().all(|v| (v[1] + 1.0).abs() < 1e-6) {
            assert!(n[1] < 0.0, "base faces up: {p:?}");
        } else {
            let c = centroid(p);
            assert!(dot(n, [c[0], 0.0, c[2]]) > 0.0, "cone side faces inward: {p:?}");
            assert!(n[1] > 0.0, "cone side should tilt upward: {p:?}");
        }
    }
}

#[test]
fn cone_normals_tilt_upward_by_the_slope() {
    // radius 1, height 2: outward normal = (2cosθ, 1, 2sinθ)/√5 → y = 1/√5.
    let m = generate_cone(1.0, 2.0, 8, 1, true);
    for n in &m.2 {
        assert!((n.y - 1.0 / 5f32.sqrt()).abs() < 1e-5, "normal.y = {}", n.y);
    }
}

#[test]
fn inverted_cone_has_no_degenerate_triangles_and_a_top_cap_only() {
    // radius_top 2, radius_bottom 0: a point at the bottom, so only the top cap exists.
    let m = generate_cylinder(2.0, 0.0, 2.0, 8, 2, false);
    assert_well_formed(&m);
    assert_faces_agree_with_vertex_normals(&m);
    for t in &m.1 {
        let p = tri(&m, t);
        assert!(area(face_normal(p)) > 1e-6, "zero-area triangle: {p:?}");
        if p.iter().all(|v| v[1] == p[0][1]) {
            assert!(p[0][1] > 0.0 && face_normal(p)[1] > 0.0, "unexpected or wrong cap: {p:?}");
        }
    }
}
