mod common;

use mesh_tools::{ExtensionTarget, GltfBuilder, GltfError};
use serde_json::json;

fn msg<T: std::fmt::Debug>(r: mesh_tools::Result<T>) -> String {
    match r {
        Err(GltfError::InvalidInput(m)) => m,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

/// Three root nodes, no scene.
fn three_nodes() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    for i in 0..3 {
        b.add_node(Some(format!("n{i}")), None, None, None, None);
    }
    b
}

#[test]
fn set_default_scene_selects_the_scene_the_reader_reports() {
    let mut b = three_nodes();
    b.add_scene(Some("A".into()), Some(vec![0]));
    b.add_scene(Some("B".into()), Some(vec![1]));
    let glb = common::parse_glb(&b);
    assert_eq!(glb.document.default_scene().unwrap().index(), 0);

    b.set_default_scene(1).unwrap();
    let glb = common::parse_glb(&b);
    let d = glb.document.default_scene().unwrap();
    assert_eq!((d.index(), d.name()), (1, Some("B")));
}

#[test]
fn set_default_scene_rejects_a_missing_scene_and_changes_nothing() {
    let mut b = three_nodes();
    b.add_scene(None, Some(vec![0]));
    let m = msg(b.set_default_scene(1));
    assert!(m.contains("scene 1") && m.contains("1 scene"), "{m}");
    assert_eq!(b.gltf.scene, Some(0));
    assert!(GltfBuilder::new().set_default_scene(0).is_err());
}

#[test]
fn clear_default_scene_omits_the_property_and_later_scenes_do_not_reselect() {
    let mut b = three_nodes();
    b.add_scene(None, Some(vec![0]));
    b.clear_default_scene();
    assert!(common::parse_glb(&b).document.default_scene().is_none());
    assert!(b.validate().has_warning("NO_DEFAULT_SCENE"));
    assert!(b.validate().is_valid());

    // A second scene must not silently undo the explicit clear.
    b.add_scene(None, Some(vec![1]));
    assert_eq!(b.gltf.scene, None);
    // Clearing twice, or with no scenes, is harmless.
    b.clear_default_scene().clear_default_scene();
    assert_eq!(GltfBuilder::new().clear_default_scene().gltf.scene, None);
}

#[test]
fn add_scene_checked_round_trips_and_selects_the_first_scene() {
    let mut b = three_nodes();
    let s0 = b.add_scene_checked(Some("first".into()), vec![0, 2]).unwrap();
    let s1 = b.add_scene_checked(None, vec![1]).unwrap();
    assert_eq!((s0, s1), (0, 1));
    let glb = common::parse_glb(&b);
    assert_eq!(glb.document.default_scene().unwrap().index(), 0);
    let scenes: Vec<_> = glb.document.scenes().collect();
    assert_eq!(scenes.len(), 2);
    assert_eq!(scenes[0].name(), Some("first"));
    let roots: Vec<usize> = scenes[0].nodes().map(|n| n.index()).collect();
    assert_eq!(roots, vec![0, 2]);
    assert_eq!(scenes[1].nodes().map(|n| n.index()).collect::<Vec<_>>(), vec![1]);
}

#[test]
fn add_scene_checked_rejects_bad_input_atomically() {
    let mut b = three_nodes();
    b.add_child_to_node(0, 1).unwrap();
    assert!(msg(b.add_scene_checked(None, vec![])).contains("at least one node"));
    assert!(msg(b.add_scene_checked(None, vec![7])).contains("node 7"));
    assert!(msg(b.add_scene_checked(None, vec![2, 2])).contains("twice"));
    assert!(msg(b.add_scene_checked(None, vec![1])).contains("child of node 0"));
    assert_eq!(b.gltf.scenes.as_ref().map_or(0, Vec::len), 0);
    assert_eq!(b.gltf.scene, None);
}

#[test]
fn add_node_to_scene_appends_and_creates_the_list_when_absent() {
    let mut b = three_nodes();
    let s = b.add_scene(Some("empty".into()), None);
    b.add_node_to_scene(s, 2).unwrap();
    b.add_node_to_scene(s, 0).unwrap();
    let glb = common::parse_glb(&b);
    let roots: Vec<usize> = glb.document.scenes().next().unwrap().nodes().map(|n| n.index()).collect();
    assert_eq!(roots, vec![2, 0]);
}

#[test]
fn add_node_to_scene_rejects_bad_input_atomically() {
    let mut b = three_nodes();
    b.add_child_to_node(0, 1).unwrap();
    let s = b.add_scene_checked(None, vec![2]).unwrap();
    assert!(msg(b.add_node_to_scene(5, 0)).contains("scene 5"));
    assert!(msg(b.add_node_to_scene(s, 9)).contains("node 9"));
    assert!(msg(b.add_node_to_scene(s, 2)).contains("already"));
    assert!(msg(b.add_node_to_scene(s, 1)).contains("child of node 0"));
    assert_eq!(b.gltf.scenes.as_ref().unwrap()[s].nodes, Some(vec![2]));
    // The same node may be a root of two different scenes.
    let s2 = b.add_scene_checked(None, vec![0]).unwrap();
    b.add_node_to_scene(s2, 2).unwrap();
    assert!(b.validate().is_valid());
}

#[test]
fn scene_extras_round_trip() {
    let mut b = three_nodes();
    let s = b.add_scene_checked(None, vec![0]).unwrap();
    b.set_extras(ExtensionTarget::Scene(s), json!({"k": 1})).unwrap();
    let glb = common::parse_glb(&b);
    let extras = glb.document.scenes().next().unwrap().extras().clone();
    let v: serde_json::Value = serde_json::from_str(extras.as_ref().unwrap().get()).unwrap();
    assert_eq!(v, json!({"k": 1}));
}

#[test]
fn validator_codes() {
    let mut b = three_nodes();
    b.add_scene(None, Some(vec![]));
    assert!(b.validate().has_error("SCENE_NODES_EMPTY"));
    assert!(b.to_glb_bytes().is_err());

    let mut b = three_nodes();
    b.add_scene(None, Some(vec![1, 1]));
    assert!(b.validate().has_error("SCENE_NODE_DUPLICATE"));

    // No scenes at all: no warning. Scenes but no default: warning only.
    assert!(!GltfBuilder::new().validate().has_warning("NO_DEFAULT_SCENE"));
    let mut b = three_nodes();
    b.add_scene(None, Some(vec![0]));
    b.gltf.scene = None;
    let r = b.validate();
    assert!(r.has_warning("NO_DEFAULT_SCENE") && r.is_valid(), "{r:#?}");
    assert!(b.to_glb_bytes().is_ok());
}
