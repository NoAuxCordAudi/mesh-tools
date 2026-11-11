mod common;

use gltf::camera::Projection;
use mesh_tools::{GltfBuilder, ValidationReport};

/// Perspective camera 0 (named), orthographic camera 1, both attached to scene nodes.
fn valid() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let p = b.add_perspective_camera(Some("P".into()), 0.8, 0.1, Some(100.0), Some(1.5)).unwrap();
    let o = b.add_orthographic_camera(None, 2.0, 1.0, 0.5, 50.0).unwrap();
    let n0 = b.add_node(None, None, None, None, None);
    let n1 = b.add_node(None, None, None, None, None);
    b.set_node_camera(n0, p).unwrap();
    b.set_node_camera(n1, o).unwrap();
    b.add_scene(None, Some(vec![n0, n1]));
    b
}

fn edited(edit: impl FnOnce(&mut GltfBuilder)) -> ValidationReport {
    let mut b = valid();
    edit(&mut b);
    b.validate()
}

fn cam(b: &mut GltfBuilder, i: usize) -> &mut mesh_tools::Camera {
    &mut b.gltf.cameras.as_mut().unwrap()[i]
}
fn persp(b: &mut GltfBuilder) -> &mut mesh_tools::Perspective {
    cam(b, 0).perspective.as_mut().unwrap()
}
fn ortho(b: &mut GltfBuilder) -> &mut mesh_tools::Orthographic {
    cam(b, 1).orthographic.as_mut().unwrap()
}

#[track_caller]
fn assert_error(code: &str, edit: impl FnOnce(&mut GltfBuilder)) {
    let r = edited(edit);
    assert!(r.has_error(code), "expected {code}; errors: {:#?}", r.errors);
    assert!(r.errors.iter().filter(|e| !e.starts_with(code)).count() == 0, "unrelated errors: {:#?}", r.errors);
}

#[track_caller]
fn assert_warning(code: &str, edit: impl FnOnce(&mut GltfBuilder)) {
    let r = edited(edit);
    assert!(r.has_warning(code), "expected warning {code}; warnings: {:#?}", r.warnings);
    assert!(r.is_valid(), "warnings must not be errors: {:#?}", r.errors);
}

#[test]
fn baseline_is_clean_and_loads() {
    let b = valid();
    let r = b.validate();
    assert!(r.errors.is_empty() && r.warnings.is_empty(), "{r:?}");
    let g = common::parse_glb(&b);
    assert!(matches!(g.document.cameras().nth(1).unwrap().projection(), Projection::Orthographic(_)));
}

#[test]
fn type_errors() {
    assert_error("CAMERA_TYPE_INVALID", |b| cam(b, 0).type_ = "fisheye".into());
    // The type names one projection but the other object is present or the named one is missing.
    assert_error("CAMERA_TYPE_MISMATCH", |b| cam(b, 0).type_ = "orthographic".into());
    assert_error("CAMERA_TYPE_MISMATCH", |b| cam(b, 0).perspective = None);
    assert_error("CAMERA_TYPE_MISMATCH", |b| {
        let o = b.gltf.cameras.as_ref().unwrap()[1].orthographic.clone();
        cam(b, 0).orthographic = o;
    });
}

#[test]
fn perspective_errors() {
    for v in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert_error("CAMERA_YFOV_INVALID", |b| persp(b).yfov = v);
    }
    for v in [Some(0.0), Some(-1.0), Some(f32::NAN)] {
        assert_error("CAMERA_ASPECT_RATIO_INVALID", |b| persp(b).aspect_ratio = v);
    }
    for v in [0.0, -0.5, f32::NAN] {
        assert_error("CAMERA_ZNEAR_INVALID", |b| persp(b).znear = v);
    }
    for v in [0.0, -5.0, 0.1, 0.05, f32::NAN, f32::INFINITY] {
        assert_error("CAMERA_ZFAR_INVALID", |b| persp(b).zfar = Some(v));
    }
}

#[test]
fn orthographic_errors() {
    for v in [0.0, f32::NAN, f32::INFINITY] {
        assert_error("CAMERA_XMAG_INVALID", |b| ortho(b).xmag = v);
        assert_error("CAMERA_YMAG_INVALID", |b| ortho(b).ymag = v);
    }
    for v in [-0.1, f32::NAN] {
        assert_error("CAMERA_ZNEAR_INVALID", |b| ortho(b).znear = v);
    }
    for v in [0.0, 0.5, 0.25, f32::NAN, f32::INFINITY] {
        assert_error("CAMERA_ZFAR_INVALID", |b| ortho(b).zfar = v);
    }
    // znear == 0 is legal for orthographic cameras.
    let r = edited(|b| ortho(b).znear = 0.0);
    assert!(r.is_valid(), "{r:?}");
}

#[test]
fn warnings() {
    assert_warning("CAMERA_YFOV_TOO_LARGE", |b| persp(b).yfov = std::f32::consts::PI);
    assert_warning("CAMERA_MAG_NEGATIVE", |b| ortho(b).xmag = -2.0);
    assert_warning("CAMERA_MAG_NEGATIVE", |b| ortho(b).ymag = -1.0);
    assert_warning("CAMERA_UNUSED", |b| {
        b.gltf.nodes.as_mut().unwrap()[0].camera = None;
    });
    // yfov just below pi is fine.
    let r = edited(|b| persp(b).yfov = 3.0);
    assert!(r.warnings.is_empty(), "{r:?}");
}

#[test]
fn node_camera_invalid() {
    assert_error("NODE_CAMERA_INVALID", |b| b.gltf.nodes.as_mut().unwrap()[0].camera = Some(2));
    assert_error("NODE_CAMERA_INVALID", |b| {
        b.gltf.cameras = None;
        b.gltf.nodes.as_mut().unwrap()[1].camera = None;
    });
}

#[test]
fn hostile_values_do_not_panic_and_are_refused_at_export() {
    let mut b = valid();
    persp(&mut b).yfov = f32::NAN;
    assert!(b.to_glb_bytes().is_err());
}

#[test]
fn camera_extensions_are_checked_for_declaration() {
    let mut b = valid();
    cam(&mut b, 0).extensions = Some(serde_json::from_str(r#"{"VENDOR_x":{}}"#).unwrap());
    assert!(b.validate().has_error("EXTENSION_NOT_DECLARED"));
}
