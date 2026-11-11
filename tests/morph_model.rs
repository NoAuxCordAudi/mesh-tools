//! M1: the morph-target fields exist in the data model, serialize with glTF names, and are
//! invisible in documents that don't use them.

mod common;

use mesh_tools::{GltfBuilder, Mesh, Node, Primitive};
use std::collections::HashMap;

fn json_of(b: &GltfBuilder) -> String {
    let bytes = b.to_glb_bytes_unchecked().unwrap();
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    String::from_utf8(bytes[20..20 + len].to_vec()).unwrap()
}

#[test]
fn documents_without_morph_targets_serialize_no_morph_keys() {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));
    let j = json_of(&b);
    for key in ["\"targets\"", "\"weights\"", "\"extras\""] {
        assert!(!j.contains(key), "{key} must not appear in a non-morph document: {j}");
    }
    common::parse_glb(&b); // still loads in the reference reader
}

#[test]
fn json_written_before_the_new_fields_still_deserializes() {
    let node: Node = serde_json::from_str(r#"{"name":"n","mesh":0}"#).unwrap();
    assert_eq!(node.weights, None);
    let mesh: Mesh = serde_json::from_str(r#"{"primitives":[{"attributes":{"POSITION":0}}]}"#).unwrap();
    assert_eq!(mesh.weights, None);
    assert!(mesh.extras.is_none());
    assert!(mesh.primitives[0].targets.is_none());
}

#[test]
fn new_fields_use_gltf_key_names_and_round_trip() {
    let target: HashMap<String, usize> = [("POSITION".to_string(), 3), ("NORMAL".to_string(), 4)].into();
    let mut primitive = Primitive::default();
    primitive.attributes = [("POSITION".to_string(), 0)].into();
    primitive.targets = Some(vec![target.clone(), target]);
    let mesh = Mesh {
        primitives: vec![primitive],
        weights: Some(vec![0.0, 0.5]),
        extras: Some(serde_json::json!({ "targetNames": ["smile", "blink"] })),
        ..Default::default()
    };
    let mut node = Node::default();
    node.mesh = Some(0);
    node.weights = Some(vec![1.0, -0.25]);

    let mj = serde_json::to_value(&mesh).unwrap();
    assert_eq!(mj["weights"], serde_json::json!([0.0, 0.5]));
    assert_eq!(mj["extras"]["targetNames"][1], "blink");
    assert_eq!(mj["primitives"][0]["targets"][1]["NORMAL"], 4);
    let nj = serde_json::to_value(&node).unwrap();
    assert_eq!(nj["weights"], serde_json::json!([1.0, -0.25]));

    let mesh_back: Mesh = serde_json::from_value(mj).unwrap();
    assert_eq!(mesh_back.weights, Some(vec![0.0, 0.5]));
    assert_eq!(mesh_back.primitives[0].targets.as_ref().unwrap().len(), 2);
    let node_back: Node = serde_json::from_value(nj).unwrap();
    assert_eq!(node_back.weights, Some(vec![1.0, -0.25]));
}

#[test]
fn builder_created_objects_leave_the_new_fields_unset() {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let node = b.add_node(None, Some(mesh), None, None, None);
    let m = &b.gltf.meshes.as_ref().unwrap()[mesh];
    assert!(m.weights.is_none() && m.extras.is_none() && m.primitives[0].targets.is_none());
    assert!(b.gltf.nodes.as_ref().unwrap()[node].weights.is_none());
}
