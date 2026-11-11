mod common;

use gltf::camera::Projection;
use mesh_tools::{ExtensionTarget, GltfBuilder};
use serde_json::json;

/// A scene with one node per camera, so the independent reader reaches every camera.
fn doc() -> (GltfBuilder, usize, usize, usize, usize) {
    let mut b = GltfBuilder::new();
    let p = b.add_perspective_camera(Some("P".into()), 0.8, 0.1, Some(100.0), Some(1.5)).unwrap();
    let inf = b.add_perspective_camera(None, 1.0, 0.25, None, None).unwrap();
    let o = b.add_orthographic_camera(Some("O".into()), 2.0, 1.0, 0.5, 50.0).unwrap();
    let nodes: Vec<usize> = [p, inf, o]
        .iter()
        .map(|&c| {
            let n = b.add_node(None, None, Some([0.0, 0.0, 5.0]), None, None);
            b.set_node_camera(n, c).unwrap();
            n
        })
        .collect();
    b.add_scene(None, Some(nodes.clone()));
    (b, p, inf, o, nodes[0])
}

#[test]
fn cameras_read_back_through_the_gltf_crate_with_hand_derived_values() {
    let (b, p, inf, o, _) = doc();
    assert_eq!((p, inf, o), (0, 1, 2));
    assert!(b.validate().errors.is_empty(), "{:?}", b.validate());
    let g = common::parse_glb(&b);
    let cams: Vec<_> = g.document.cameras().collect();
    assert_eq!(cams.len(), 3);

    assert_eq!(cams[0].name(), Some("P"));
    let Projection::Perspective(p) = cams[0].projection() else { panic!("not perspective") };
    assert_eq!((p.yfov(), p.znear(), p.zfar(), p.aspect_ratio()), (0.8, 0.1, Some(100.0), Some(1.5)));

    assert_eq!(cams[1].name(), None);
    let Projection::Perspective(p) = cams[1].projection() else { panic!("not perspective") };
    assert_eq!((p.yfov(), p.znear(), p.zfar(), p.aspect_ratio()), (1.0, 0.25, None, None));

    assert_eq!(cams[2].name(), Some("O"));
    let Projection::Orthographic(o) = cams[2].projection() else { panic!("not orthographic") };
    assert_eq!((o.xmag(), o.ymag(), o.znear(), o.zfar()), (2.0, 1.0, 0.5, 50.0));

    let nodes: Vec<_> = g.document.nodes().map(|n| n.camera().map(|c| c.index())).collect();
    assert_eq!(nodes, [Some(0), Some(1), Some(2)]);
}

#[test]
fn json_shape_matches_the_spec() {
    let (b, ..) = doc();
    let v: serde_json::Value = serde_json::from_str(&b.to_gltf_string_embedded().unwrap()).unwrap();
    assert_eq!(
        v["cameras"][0],
        json!({ "name": "P", "type": "perspective",
                "perspective": { "yfov": 0.8f32, "znear": 0.1f32, "zfar": 100.0, "aspectRatio": 1.5 } })
    );
    assert_eq!(v["cameras"][1]["perspective"].as_object().unwrap().len(), 2, "no zfar/aspectRatio keys");
    assert_eq!(v["cameras"][2]["type"], "orthographic");
    assert_eq!(v["nodes"][2]["camera"], 2);
}

#[test]
fn documents_without_cameras_have_no_camera_keys_and_are_unchanged() {
    let mut b = GltfBuilder::new();
    let m = b.create_box(1.0);
    let n = b.add_node(None, Some(m), None, None, None);
    b.add_scene(None, Some(vec![n]));
    let s = b.to_gltf_string_embedded().unwrap();
    assert!(!s.contains("camera"));
    // Byte-identical to the previous commit's output (hash recorded from commit e2dc001).
    let glb = b.to_glb_bytes().unwrap();
    assert_eq!(fnv1a(&glb), GOLDEN_BOX_GLB_FNV, "output without cameras must not change");
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, &c| (h ^ c as u64).wrapping_mul(0x100000001b3))
}
const GOLDEN_BOX_GLB_FNV: u64 = 0x4e9bf7fb746e6bf9;

#[test]
fn invalid_cameras_are_rejected_and_leave_the_document_unchanged() {
    let mut b = GltfBuilder::new();
    let ok = b.add_perspective_camera(None, 1.0, 0.1, Some(10.0), None).unwrap();
    let before = serde_json::to_string(&b.gltf).unwrap();
    let nan = f32::NAN;
    let inf = f32::INFINITY;
    let errs = [
        b.add_perspective_camera(None, 0.0, 0.1, None, None),
        b.add_perspective_camera(None, -1.0, 0.1, None, None),
        b.add_perspective_camera(None, nan, 0.1, None, None),
        b.add_perspective_camera(None, 1.0, 0.0, None, None),
        b.add_perspective_camera(None, 1.0, inf, None, None),
        b.add_perspective_camera(None, 1.0, 1.0, Some(1.0), None),
        b.add_perspective_camera(None, 1.0, 1.0, Some(0.5), None),
        b.add_perspective_camera(None, 1.0, 1.0, Some(inf), None),
        b.add_perspective_camera(None, 1.0, 0.1, None, Some(0.0)),
        b.add_perspective_camera(None, 1.0, 0.1, None, Some(nan)),
        b.add_orthographic_camera(None, 0.0, 1.0, 0.1, 1.0),
        b.add_orthographic_camera(None, 1.0, 0.0, 0.1, 1.0),
        b.add_orthographic_camera(None, nan, 1.0, 0.1, 1.0),
        b.add_orthographic_camera(None, 1.0, 1.0, -0.1, 1.0),
        b.add_orthographic_camera(None, 1.0, 1.0, 1.0, 1.0),
        b.add_orthographic_camera(None, 1.0, 1.0, 0.0, inf),
    ];
    for (i, e) in errs.iter().enumerate() {
        assert!(e.is_err(), "case {i} must fail");
    }
    assert_eq!(serde_json::to_string(&b.gltf).unwrap(), before);
    assert_eq!(b.gltf.cameras.as_ref().unwrap().len(), ok + 1);
}

#[test]
fn edge_values_are_accepted() {
    let mut b = GltfBuilder::new();
    // Orthographic znear may be 0; negative magnitudes mirror the view (warned by the validator).
    b.add_orthographic_camera(None, -2.0, -1.0, 0.0, 1.0).unwrap();
    // A very wide yfov is legal (warned by the validator).
    b.add_perspective_camera(None, 3.2, 0.1, None, None).unwrap();
}

#[test]
fn set_and_clear_node_camera_are_explicit_and_atomic() {
    let mut b = GltfBuilder::new();
    let c0 = b.add_perspective_camera(None, 1.0, 0.1, None, None).unwrap();
    let c1 = b.add_orthographic_camera(None, 1.0, 1.0, 0.1, 2.0).unwrap();
    let n = b.add_node(None, None, None, None, None);
    b.set_node_camera(n, c0).unwrap();
    assert_eq!(b.gltf.nodes.as_ref().unwrap()[n].camera, Some(c0));

    // Replacing needs an explicit clear.
    let e = b.set_node_camera(n, c1).unwrap_err().to_string();
    assert!(e.contains("already"), "{e}");
    assert_eq!(b.gltf.nodes.as_ref().unwrap()[n].camera, Some(c0));
    b.clear_node_camera(n).unwrap();
    assert_eq!(b.gltf.nodes.as_ref().unwrap()[n].camera, None);
    b.set_node_camera(n, c1).unwrap();

    assert!(b.set_node_camera(99, c0).is_err());
    let n2 = b.add_node(None, None, None, None, None);
    assert!(b.set_node_camera(n2, 99).is_err());
    assert_eq!(b.gltf.nodes.as_ref().unwrap()[n2].camera, None);
    assert!(b.clear_node_camera(99).is_err());
    b.clear_node_camera(n2).unwrap(); // nothing to clear is not an error
}

#[test]
fn camera_extras_and_extensions_reach_the_reader() {
    let mut b = GltfBuilder::new();
    let c = b.add_perspective_camera(None, 1.0, 0.1, None, None).unwrap();
    b.set_extras(ExtensionTarget::Camera(c), json!({ "k": 1 })).unwrap();
    b.set_extension(ExtensionTarget::Camera(c), "VENDOR_cam", json!({ "a": 1 })).unwrap();
    b.set_extras(ExtensionTarget::CameraPerspective(c), json!("p")).unwrap();
    b.set_extension(ExtensionTarget::CameraPerspective(c), "VENDOR_persp", json!({})).unwrap();
    let o = b.add_orthographic_camera(None, 1.0, 1.0, 0.1, 2.0).unwrap();
    b.set_extras(ExtensionTarget::CameraOrthographic(o), json!([1])).unwrap();
    let n = b.add_node(None, None, None, None, None);
    b.set_node_camera(n, c).unwrap();
    let n2 = b.add_node(None, None, None, None, None);
    b.set_node_camera(n2, o).unwrap();
    b.add_scene(None, Some(vec![n, n2]));
    assert!(b.validate().errors.is_empty(), "{:?}", b.validate());
    let g = common::parse_glb(&b);
    let cams: Vec<_> = g.document.cameras().collect();
    assert_eq!(cams[0].extras().as_ref().unwrap().get(), r#"{"k":1}"#);
    assert!(cams[0].extension_value("VENDOR_cam").is_some());
    let Projection::Perspective(p) = cams[0].projection() else { panic!() };
    assert_eq!(p.extras().as_ref().unwrap().get(), r#""p""#);
    let Projection::Orthographic(oo) = cams[1].projection() else { panic!() };
    assert_eq!(oo.extras().as_ref().unwrap().get(), "[1]");

    // Targets that do not exist, or the wrong projection, are errors.
    assert!(b.set_extras(ExtensionTarget::Camera(9), json!(1)).is_err());
    assert!(b.set_extras(ExtensionTarget::CameraPerspective(o), json!(1)).is_err());
    assert!(b.set_extras(ExtensionTarget::CameraOrthographic(c), json!(1)).is_err());
}
