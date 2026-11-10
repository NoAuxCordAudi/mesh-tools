mod common;

use mesh_tools::GltfError;

#[test]
fn skinned_node_keeps_parent_tracking_in_sync() {
    let (mut b, mesh, skin, joint) = common::builder_with_mesh_and_skin();
    let skinned = b.add_skinned_node(Some("S".into()), mesh, skin, None, None, None).unwrap();
    // A node created *after* the skinned node must still be parentable. Before the fix the
    // parent table was one entry short, so this returned InvalidIndex.
    let later = b.add_node(Some("Later".into()), None, None, None, None);
    b.add_child_to_node(joint, later).expect("later node is parentable");
    b.add_child_to_node(joint, skinned).expect("skinned node is parentable");
}

#[test]
fn skinned_node_cannot_be_parented_twice() {
    let (mut b, mesh, skin, joint) = common::builder_with_mesh_and_skin();
    let other = b.add_joint_node(None, None, None, None);
    let skinned = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.add_child_to_node(joint, skinned).unwrap();
    assert!(b.add_child_to_node(other, skinned).is_err());
}

#[test]
fn skinned_node_rotation_is_normalized() {
    let (mut b, mesh, skin, _) = common::builder_with_mesh_and_skin();
    let n = b.add_skinned_node(None, mesh, skin, None, Some([0.0, 0.0, 0.0, 2.0]), None).unwrap();
    let r = b.gltf.nodes.as_ref().unwrap()[n].rotation.unwrap();
    assert!((r[3] - 1.0).abs() < 1e-6 && r[0] == 0.0);
}

#[test]
fn skinned_node_references_mesh_and_skin() {
    let (mut b, mesh, skin, _) = common::builder_with_mesh_and_skin();
    let n = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    let node = &b.gltf.nodes.as_ref().unwrap()[n];
    assert_eq!((node.mesh, node.skin), (Some(mesh), Some(skin)));
}

#[test]
fn skinned_node_rejects_bad_mesh_or_skin_index() {
    let (mut b, mesh, skin, _) = common::builder_with_mesh_and_skin();
    let before = b.gltf.nodes.as_ref().map_or(0, |n| n.len());
    assert!(matches!(b.add_skinned_node(None, mesh + 1, skin, None, None, None), Err(GltfError::InvalidIndex)));
    assert!(matches!(b.add_skinned_node(None, mesh, skin + 1, None, None, None), Err(GltfError::InvalidIndex)));
    // Nothing was added on failure.
    assert_eq!(b.gltf.nodes.as_ref().map_or(0, |n| n.len()), before);
}

#[test]
fn skinned_node_without_any_skins_is_rejected() {
    let mut b = mesh_tools::GltfBuilder::new();
    let mesh = b.create_box(1.0);
    assert!(b.add_skinned_node(None, mesh, 0, None, None, None).is_err());
}
