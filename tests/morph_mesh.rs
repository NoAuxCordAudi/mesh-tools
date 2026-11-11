#![allow(deprecated)] // exercises the deprecated with_primitive_builder
mod common;

use mesh_tools::mesh::MeshBuilder;
use mesh_tools::{GltfBuilder, GltfError, JointIndices, MorphTarget, Primitive, SkinSet};

const POS: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
const NRM: [f32; 9] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];

fn target() -> MorphTarget {
    MorphTarget::new().with_positions(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0])
}

/// A skinned triangle primitive with `n` morph targets attached.
fn morphed_primitive(b: &mut GltfBuilder, n: usize) -> Primitive {
    let set = SkinSet { joints: JointIndices::U8(vec![[0; 4]; 3]), weights: vec![[1.0, 0.0, 0.0, 0.0]; 3] };
    let mut p = b.create_skinned_primitive(&POS, Some(&[0, 1, 2]), Some(&NRM), None, &[set], 1, None).unwrap();
    if n > 0 {
        let targets: Vec<MorphTarget> = (0..n).map(|_| target()).collect();
        b.attach_morph_targets(&mut p, &targets).unwrap();
    }
    p
}

fn json_of(b: &GltfBuilder) -> serde_json::Value {
    let bytes = b.to_glb_bytes_unchecked().unwrap();
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[20..20 + len]).unwrap()
}

fn counts(b: &GltfBuilder) -> (usize, usize, usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffers.as_ref().map_or(0, |a| a.first().map_or(0, |b| b.byte_length)),
        b.gltf.meshes.as_ref().map_or(0, |a| a.len()),
        b.gltf.nodes.as_ref().map_or(0, |a| a.len()),
    )
}

fn msg<T: std::fmt::Debug>(r: mesh_tools::Result<T>) -> String {
    match r {
        Err(GltfError::InvalidInput(m)) => m,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[test]
fn weights_and_names_round_trip() {
    let mut b = GltfBuilder::new();
    let p = morphed_primitive(&mut b, 2);
    let mesh = b
        .add_morphable_mesh(Some("Face".into()), vec![p], Some(vec![0.25, -1.5]), Some(vec!["smile".into(), "blink".into()]))
        .unwrap();
    assert_eq!(mesh, 0);

    let glb = common::parse_glb(&b);
    let m = glb.document.meshes().next().unwrap();
    assert_eq!(m.weights(), Some(&[0.25f32, -1.5][..]), "negative weights are legal and kept as given");
    let j = json_of(&b);
    assert_eq!(j["meshes"][0]["extras"]["targetNames"], serde_json::json!(["smile", "blink"]));
    assert_eq!(j["meshes"][0]["weights"], serde_json::json!([0.25, -1.5]));
}

#[test]
fn omitted_weights_and_names_write_nothing() {
    let mut b = GltfBuilder::new();
    let p = morphed_primitive(&mut b, 1);
    b.add_morphable_mesh(None, vec![p], None, None).unwrap();
    let mesh = &json_of(&b)["meshes"][0];
    assert!(mesh.get("weights").is_none() && mesh.get("extras").is_none(), "{mesh}");
}

#[test]
fn weights_above_one_are_accepted() {
    let mut b = GltfBuilder::new();
    let p = morphed_primitive(&mut b, 1);
    assert!(b.add_morphable_mesh(None, vec![p], Some(vec![3.0]), None).is_ok());
}

#[test]
fn multiple_primitives_with_the_same_target_count_are_fine() {
    let mut b = GltfBuilder::new();
    let (p1, p2) = (morphed_primitive(&mut b, 2), morphed_primitive(&mut b, 2));
    assert!(b.add_morphable_mesh(None, vec![p1, p2], None, None).is_ok());
}

#[test]
fn add_morphable_mesh_rejects_bad_input_atomically() {
    let mut b = GltfBuilder::new();
    let before = counts(&b);
    assert!(msg(b.add_morphable_mesh(None, vec![], None, None)).contains("at least one primitive"));

    let plain = morphed_primitive(&mut b, 0);
    let before_plain = counts(&b);
    assert!(msg(b.add_morphable_mesh(None, vec![plain], None, None)).contains("primitive 0 has no morph targets"));

    let (two, three) = (morphed_primitive(&mut b, 2), morphed_primitive(&mut b, 3));
    let m = msg(b.add_morphable_mesh(None, vec![two, three], None, None));
    assert!(m.contains("primitive 0 has 2, primitive 1 has 3"), "{m}");

    let p = || morphed_primitive(&mut GltfBuilder::new(), 2); // detached builder: only the struct is used
    let after_prims = counts(&b);
    assert!(msg(b.add_morphable_mesh(None, vec![p()], Some(vec![0.0]), None)).contains("1 default weights for 2"));
    assert!(msg(b.add_morphable_mesh(None, vec![p()], Some(vec![0.0, f32::NAN]), None)).contains("weight 1 is not finite"));
    assert!(msg(b.add_morphable_mesh(None, vec![p()], None, Some(vec!["a".into()]))).contains("1 target names for 2"));
    assert_eq!(counts(&b), after_prims, "no mesh may be added by a failed call");
    assert!(before.3 == 0 && before_plain.3 == 0 && after_prims.3 == 0);
}

#[test]
fn set_node_weights_round_trips_and_overrides_mesh_weights() {
    let mut b = GltfBuilder::new();
    let p = morphed_primitive(&mut b, 2);
    let mesh = b.add_morphable_mesh(None, vec![p], Some(vec![0.0, 0.0]), None).unwrap();
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.set_node_weights(node, vec![1.0, 2.0]).unwrap();
    b.add_scene(None, Some(vec![node]));

    let glb = common::parse_glb(&b);
    let n = glb.document.nodes().next().unwrap();
    assert_eq!(n.weights(), Some(&[1.0f32, 2.0][..]));
    assert_eq!(n.mesh().unwrap().weights(), Some(&[0.0f32, 0.0][..]), "mesh defaults are kept separately");

    b.set_node_weights(node, vec![-0.5, 0.5]).unwrap(); // setting again replaces
    assert_eq!(b.gltf.nodes.as_ref().unwrap()[node].weights, Some(vec![-0.5, 0.5]));
}

#[test]
fn set_node_weights_rejects_bad_input_and_leaves_the_node_unchanged() {
    let mut b = GltfBuilder::new();
    let p = morphed_primitive(&mut b, 2);
    let mesh = b.add_morphable_mesh(None, vec![p], None, None).unwrap();
    let morph_node = b.add_node(None, Some(mesh), None, None, None);
    let bare_node = b.add_node(None, None, None, None, None);
    let plain_mesh = b.create_box(1.0);
    let plain_node = b.add_node(None, Some(plain_mesh), None, None, None);
    let dangling_node = b.add_node(None, Some(99), None, None, None);

    assert!(msg(b.set_node_weights(99, vec![0.0])).contains("node 99 does not exist"));
    assert!(msg(b.set_node_weights(bare_node, vec![0.0])).contains("has no mesh"));
    assert!(msg(b.set_node_weights(plain_node, vec![0.0])).contains("no morph targets"));
    assert!(msg(b.set_node_weights(dangling_node, vec![0.0])).contains("mesh 99"));
    assert!(msg(b.set_node_weights(morph_node, vec![0.0])).contains("1 default weights for 2"));
    assert!(msg(b.set_node_weights(morph_node, vec![0.0, f32::INFINITY])).contains("not finite"));
    for n in [morph_node, bare_node, plain_node, dangling_node] {
        assert!(b.gltf.nodes.as_ref().unwrap()[n].weights.is_none());
    }
}

// ---- MeshBuilder ----

fn builder_with_everything() -> MeshBuilder {
    MeshBuilder::new(Some("M".into()))
        .with_positions(POS.to_vec())
        .with_indices(vec![0, 1, 2])
        .with_normals(NRM.to_vec())
        .with_joints(vec![[0; 4]; 3])
        .with_weights(vec![[1.0, 0.0, 0.0, 0.0]; 3])
        .with_joint_count(1)
        .with_morph_targets(vec![target(), target()])
        .with_default_weights(vec![0.5, 0.0])
        .with_target_names(vec!["a".into(), "b".into()])
}

#[test]
fn mesh_builder_output_equals_the_direct_api() {
    let mut via_builder = GltfBuilder::new();
    builder_with_everything().build_into(&mut via_builder).unwrap();

    // The same data through the direct API. MeshBuilder::with_joints takes u16 joints, so use U16 here.
    let mut direct = GltfBuilder::new();
    let set = SkinSet { joints: JointIndices::U16(vec![[0; 4]; 3]), weights: vec![[1.0, 0.0, 0.0, 0.0]; 3] };
    let mut p = direct.create_skinned_primitive(&POS, Some(&[0, 1, 2]), Some(&NRM), None, &[set], 1, None).unwrap();
    direct.attach_morph_targets(&mut p, &[target(), target()]).unwrap();
    direct.add_morphable_mesh(Some("M".into()), vec![p], Some(vec![0.5, 0.0]), Some(vec!["a".into(), "b".into()])).unwrap();

    assert_eq!(via_builder.to_glb_bytes_unchecked().unwrap(), direct.to_glb_bytes_unchecked().unwrap());
    common::parse_glb(&via_builder);
}

#[test]
fn mesh_builder_supports_unskinned_morphing() {
    let mut b = GltfBuilder::new();
    MeshBuilder::new(None)
        .with_positions(POS.to_vec())
        .with_indices(vec![0, 1, 2])
        .with_morph_targets(vec![target()])
        .build_into(&mut b)
        .unwrap();
    let glb = common::parse_glb(&b);
    assert_eq!(glb.document.meshes().next().unwrap().primitives().next().unwrap().morph_targets().count(), 1);
}

#[test]
fn mesh_builder_failures_write_nothing() {
    let cases: Vec<(&str, MeshBuilder, &str)> = vec![
        ("short deltas", builder_with_everything().with_morph_targets(vec![MorphTarget::new().with_positions(vec![0.0; 6])]).with_default_weights(vec![0.0]).with_target_names(vec!["a".into()]), "expected 9"),
        ("normal delta without base normals", MeshBuilder::new(None).with_positions(POS.to_vec()).with_morph_targets(vec![MorphTarget::new().with_normals(vec![0.0; 9])]), "no base NORMAL"),
        ("weights count", builder_with_everything().with_default_weights(vec![0.0]), "1 default weights for 2"),
        ("names count", builder_with_everything().with_target_names(vec!["a".into()]), "1 target names for 2"),
        ("weights without targets", MeshBuilder::new(None).with_positions(POS.to_vec()).with_default_weights(vec![0.0]), "no morph targets"),
        ("names without targets", MeshBuilder::new(None).with_positions(POS.to_vec()).with_target_names(vec!["a".into()]), "no morph targets"),
        ("empty target list", MeshBuilder::new(None).with_positions(POS.to_vec()).with_morph_targets(vec![]), "at least one morph target"),
        // Morph data is fine but the skin data is not: the base primitive must not be left behind.
        ("bad skin data", builder_with_everything().with_weights(vec![[0.5, 0.0, 0.0, 0.0]; 3]), "weights sum"),
        ("custom builder", builder_with_everything().with_primitive_builder(Box::new(|_, _, _, _, _| Primitive::default())), "custom primitive builder"),
    ];
    for (name, mb, needle) in cases {
        let mut b = GltfBuilder::new();
        let before = counts(&b);
        let m = msg(mb.build_into(&mut b));
        assert!(m.contains(needle), "{name}: {m:?} should contain {needle:?}");
        assert_eq!(counts(&b), before, "{name}: failed build_into must not write anything");
    }
}

#[test]
fn custom_builder_alone_still_works_after_the_morph_check() {
    let mut b = GltfBuilder::new();
    let idx = MeshBuilder::new(None)
        .with_positions(POS.to_vec())
        .with_primitive_builder(Box::new(|_, _, _, _, _| Primitive::default()))
        .build_into(&mut b)
        .unwrap();
    assert_eq!(idx, 0);
}

#[test]
fn custom_builder_cannot_be_combined_with_morph_data_alone() {
    // No skinning data here, so only the morph check can reject this.
    let custom = || Box::new(|_: &[f32], _: &[u16], _: Option<&[f32]>, _: Option<Vec<Vec<f32>>>, _: Option<usize>| Primitive::default());
    for (name, mb) in [
        ("targets", MeshBuilder::new(None).with_positions(POS.to_vec()).with_morph_targets(vec![target()])),
        ("weights", MeshBuilder::new(None).with_positions(POS.to_vec()).with_default_weights(vec![0.0])),
        ("names", MeshBuilder::new(None).with_positions(POS.to_vec()).with_target_names(vec!["a".into()])),
    ] {
        let mut b = GltfBuilder::new();
        let m = msg(mb.with_primitive_builder(custom()).build_into(&mut b));
        assert!(m.contains("morph targets, default weights or target names"), "{name}: {m}");
        assert_eq!(counts(&b), (0, 0, 0, 0, 0));
    }
}
