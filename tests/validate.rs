mod common;

use mesh_tools::{GltfBuilder, GltfError, Skin};

/// A valid skinned document: 2-joint chain, skinned mesh, skin, skinned node, scene.
/// Returns (builder, joints, mesh, skin, skinned_node).
fn valid_skinned() -> (GltfBuilder, Vec<usize>, usize, usize, usize) {
    let mut b = GltfBuilder::new();
    let names = vec!["A".to_string(), "B".to_string()];
    let joints = b.create_bone_chain(&names, &[[0.0; 3], [1.0, 0.0, 0.0]]).unwrap();
    let positions = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let mesh = b
        .create_skinned_mesh(
            None,
            &positions,
            &[0, 1, 2],
            None,
            None,
            &[[0, 0, 0, 0], [1, 0, 0, 0], [0, 1, 0, 0]],
            &[[1.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0]],
            None,
        )
        .unwrap();
    let skin = b.add_skin_from_joints(None, joints.clone(), Some(joints[0])).unwrap();
    let node = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.add_scene(None, Some(vec![node, joints[0]]));
    (b, joints, mesh, skin, node)
}

#[track_caller]
fn assert_error(b: &GltfBuilder, code: &str) {
    let r = b.validate();
    assert!(r.has_error(code), "expected {code}, got errors: {:#?}", r.errors);
}

fn json_of(b: &GltfBuilder) -> serde_json::Value {
    let bytes = b.to_glb_bytes_unchecked().unwrap();
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[20..20 + len]).unwrap()
}

#[test]
fn well_formed_documents_pass_and_load_in_the_reference_loader() {
    let (b, ..) = valid_skinned();
    let r = b.validate();
    assert!(r.is_valid(), "{:#?}", r.errors);
    assert!(r.warnings.is_empty(), "{:#?}", r.warnings);
    common::parse_glb(&b);
    assert!(GltfBuilder::new().validate().is_valid());
}

#[test]
fn empty_top_level_arrays_are_omitted_from_the_json() {
    let j = json_of(&GltfBuilder::new());
    for key in ["scenes", "nodes", "meshes", "accessors", "bufferViews", "materials", "textures", "images", "samplers", "skins", "animations"] {
        assert!(j.get(key).is_none(), "{key} must not serialize as an empty array");
    }
    assert!(j.get("buffers").is_some() && j.get("asset").is_some());
}

#[test]
fn skinned_node_requirements() {
    let (mut b, _, _, skin, _) = valid_skinned();
    // A skinned node with no mesh.
    let n = b.add_node(None, None, None, None, None);
    b.gltf.nodes.as_mut().unwrap()[n].skin = Some(skin);
    assert_error(&b, "NODE_SKIN_WITHOUT_MESH");

    // Skin index that doesn't exist.
    let (mut b, _, _, _, node) = valid_skinned();
    b.gltf.nodes.as_mut().unwrap()[node].skin = Some(9);
    assert_error(&b, "NODE_SKIN_INVALID");

    // Skinned node whose mesh has no JOINTS_0/WEIGHTS_0.
    let (mut b, _, _, skin, _) = valid_skinned();
    let plain_mesh = b.create_box(1.0);
    b.add_skinned_node(None, plain_mesh, skin, None, None, None).unwrap();
    assert_error(&b, "SKIN_MESH_MISSING_ATTRIBUTES");
}

#[test]
fn joint_slot_beyond_the_skin_is_read_back_from_the_buffer() {
    let (mut b, joints, _, _, _) = valid_skinned();
    // Weighted slot 5 on a 2-joint skin: the classic "node index instead of slot" bug.
    let positions = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let bad_mesh = b
        .create_skinned_mesh(None, &positions, &[0, 1, 2], None, None,
            &[[5, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]],
            &[[1.0, 0.0, 0.0, 0.0]; 3], None)
        .unwrap();
    let skin2 = b.add_skin(None, joints.clone(), None, None).unwrap();
    b.add_skinned_node(None, bad_mesh, skin2, None, None, None).unwrap();
    let r = b.validate();
    assert!(r.has_error("SKIN_JOINT_OUT_OF_RANGE"), "{:#?}", r.errors);
    assert!(r.errors.iter().any(|e| e.contains("vertex 0 slot 0 is 5")), "{:#?}", r.errors);
}

#[test]
fn unweighted_slots_may_hold_out_of_range_values() {
    let (mut b, joints, _, _, _) = valid_skinned();
    let positions = vec![0.0; 9];
    let mesh = b
        .create_skinned_mesh(None, &positions, &[0, 1, 2], None, None,
            &[[0, 99, 99, 99]; 3], &[[1.0, 0.0, 0.0, 0.0]; 3], None)
        .unwrap();
    let skin = b.add_skin(None, joints, None, None).unwrap();
    b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    assert!(b.validate().is_valid());
}

#[test]
fn skin_structure_errors() {
    let skin_with = |joints: Vec<usize>, skeleton: Option<usize>, ibm: Option<usize>| Skin {
        name: None,
        joints,
        inverse_bind_matrices: ibm,
        skeleton,
        ..Default::default()
    };
    let cases: Vec<(&str, Skin)> = vec![
        ("SKIN_NO_JOINTS", skin_with(vec![], None, None)),
        ("SKIN_JOINT_INVALID", skin_with(vec![99], None, None)),
        ("SKIN_JOINT_DUPLICATE", skin_with(vec![0, 0], None, None)),
        ("SKIN_NO_COMMON_ROOT", skin_with(vec![0, 2], None, None)), // node 2 is a separate root
        ("SKIN_SKELETON_NOT_ANCESTOR", skin_with(vec![0, 1], Some(2), None)),
        ("SKIN_SKELETON_INVALID", skin_with(vec![0, 1], Some(99), None)),
        ("SKIN_IBM_INVALID", skin_with(vec![0, 1], None, Some(999))),
        ("SKIN_IBM_COUNT", skin_with(vec![0, 1], None, Some(0))), // accessor 0 below: 1 MAT4
    ];
    for (code, skin) in cases {
        let mut b = GltfBuilder::new();
        let names = vec!["A".to_string(), "B".to_string()];
        b.create_bone_chain(&names, &[[0.0; 3], [1.0, 0.0, 0.0]]).unwrap(); // nodes 0,1
        b.add_joint_node(None, None, None, None); // node 2, separate root
        b.create_inverse_bind_matrices_accessor_from_f32(&[[0.0; 16]]).unwrap(); // accessor 0
        b.gltf.skins.as_mut().unwrap().push(skin);
        assert_error(&b, code);
    }
    // IBM accessor of the wrong type: use a VEC3 position accessor.
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    b.add_joint_node(None, None, None, None);
    b.gltf.skins.as_mut().unwrap().push(skin_with(vec![0], None, Some(0)));
    assert_error(&b, "SKIN_IBM_TYPE");
}

#[test]
fn hierarchy_errors() {
    let mut b = GltfBuilder::new();
    let (a, c) = (b.add_node(None, None, None, None, None), b.add_node(None, None, None, None, None));
    b.gltf.nodes.as_mut().unwrap()[a].children = Some(vec![c]);
    b.gltf.nodes.as_mut().unwrap()[c].children = Some(vec![a]);
    assert_error(&b, "NODE_CYCLE");

    let mut b = GltfBuilder::new();
    let (p1, p2, k) = (
        b.add_node(None, None, None, None, None),
        b.add_node(None, None, None, None, None),
        b.add_node(None, None, None, None, None),
    );
    b.gltf.nodes.as_mut().unwrap()[p1].children = Some(vec![k]);
    b.gltf.nodes.as_mut().unwrap()[p2].children = Some(vec![k]);
    assert_error(&b, "NODE_PARENT_OVERRIDE");

    let mut b = GltfBuilder::new();
    let n = b.add_node(None, None, None, None, None);
    b.gltf.nodes.as_mut().unwrap()[n].children = Some(vec![42]);
    assert_error(&b, "NODE_CHILD_INVALID");

    let mut b = GltfBuilder::new();
    let n = b.add_node(None, Some(7), None, None, None);
    let _ = n;
    assert_error(&b, "NODE_MESH_INVALID");

    let mut b = GltfBuilder::new();
    let n = b.add_node(None, None, Some([1.0, 0.0, 0.0]), None, None);
    b.gltf.nodes.as_mut().unwrap()[n].matrix = Some([0.0; 16]);
    assert_error(&b, "NODE_MATRIX_AND_TRS");
}

#[test]
fn scene_errors() {
    let mut b = GltfBuilder::new();
    let parent = b.add_node(None, None, None, None, None);
    let child = b.add_node(None, None, None, None, None);
    b.add_child_to_node(parent, child).unwrap();
    b.add_scene(None, Some(vec![child])); // child is not a root
    assert_error(&b, "SCENE_ROOT_HAS_PARENT");

    let mut b = GltfBuilder::new();
    b.add_scene(None, Some(vec![3]));
    assert_error(&b, "SCENE_NODE_INVALID");

    let mut b = GltfBuilder::new();
    b.gltf.scene = Some(0);
    assert_error(&b, "SCENE_DEFAULT_INVALID");
}

#[test]
fn primitive_and_accessor_errors() {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    b.gltf.meshes.as_mut().unwrap()[mesh].primitives[0].material = Some(0);
    assert_error(&b, "PRIMITIVE_MATERIAL_INVALID");

    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    b.gltf.meshes.as_mut().unwrap()[mesh].primitives[0].attributes.insert("TEXCOORD_0".into(), 999);
    assert_error(&b, "PRIMITIVE_ACCESSOR_INVALID");

    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let prim = &mut b.gltf.meshes.as_mut().unwrap()[mesh].primitives[0];
    prim.attributes.insert("JOINTS_0".into(), 0);
    assert_error(&b, "PRIMITIVE_JOINTS_WITHOUT_WEIGHTS");

    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let prim = &mut b.gltf.meshes.as_mut().unwrap()[mesh].primitives[0];
    prim.attributes.insert("WEIGHTS_0".into(), 0);
    assert_error(&b, "PRIMITIVE_WEIGHTS_WITHOUT_JOINTS");

    // Indices that point at a VEC3 accessor.
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    b.gltf.meshes.as_mut().unwrap()[mesh].primitives[0].indices = Some(0);
    assert_error(&b, "PRIMITIVE_INDICES_TYPE");

    // Accessor reading past its buffer view / dangling view / buffer length drift.
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    b.gltf.accessors.as_mut().unwrap()[0].count += 1000;
    assert_error(&b, "ACCESSOR_RANGE");
    b.gltf.accessors.as_mut().unwrap()[0].buffer_view = 999;
    assert_error(&b, "ACCESSOR_BUFFER_VIEW_INVALID");

    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    b.gltf.buffers.as_mut().unwrap()[0].byte_length += 4;
    assert_error(&b, "BUFFER_LENGTH_MISMATCH");
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    b.gltf.buffer_views.as_mut().unwrap()[0].byte_length += 100_000;
    assert_error(&b, "BUFFER_VIEW_RANGE");
}

#[test]
fn animation_errors() {
    let mut b = GltfBuilder::new();
    b.add_animation(None);
    assert_error(&b, "ANIMATION_EMPTY");

    let mk = || {
        let mut b = GltfBuilder::new();
        let n = b.add_node(None, None, None, None, None);
        let a = b.add_animation(None);
        b.add_translation_track(a, n, &[0.0], &[[0.0; 3]], mesh_tools::InterpolationType::Linear).unwrap();
        assert!(b.validate().is_valid());
        b
    };
    let mut b = mk();
    b.gltf.animations.as_mut().unwrap()[0].channels.as_mut().unwrap()[0].sampler = 9;
    assert_error(&b, "ANIMATION_CHANNEL_SAMPLER_INVALID");
    let mut b = mk();
    b.gltf.animations.as_mut().unwrap()[0].channels.as_mut().unwrap()[0].target.node = 9;
    assert_error(&b, "ANIMATION_CHANNEL_NODE_INVALID");
    let mut b = mk();
    b.gltf.animations.as_mut().unwrap()[0].channels.as_mut().unwrap()[0].target.path = "position".into();
    assert_error(&b, "ANIMATION_CHANNEL_PATH_INVALID");
    let mut b = mk();
    b.gltf.animations.as_mut().unwrap()[0].samplers.as_mut().unwrap()[0].output = 99;
    assert_error(&b, "ANIMATION_SAMPLER_ACCESSOR_INVALID");
    let mut b = mk();
    let input = b.gltf.animations.as_ref().unwrap()[0].samplers.as_ref().unwrap()[0].input;
    b.gltf.accessors.as_mut().unwrap()[input].min = None;
    assert_error(&b, "ANIMATION_INPUT_NO_BOUNDS");
    let mut b = mk();
    b.gltf.nodes.as_mut().unwrap()[0].matrix = Some([0.0; 16]);
    assert_error(&b, "ANIMATION_CHANNEL_NODE_MATRIX");
}

#[test]
fn warnings_do_not_block_export() {
    let (mut b, _, _, _, node) = valid_skinned();
    b.gltf.nodes.as_mut().unwrap()[node].translation = Some([1.0, 0.0, 0.0]);
    let r = b.validate();
    assert!(r.has_warning("SKINNED_NODE_LOCAL_TRANSFORM"), "{r:#?}");
    assert!(r.is_valid());
    assert!(b.to_glb_bytes().is_ok());
}

#[test]
fn export_fails_listing_every_error_and_writes_no_file() {
    let mut b = GltfBuilder::new();
    b.add_node(None, Some(7), None, None, None); // NODE_MESH_INVALID
    b.add_scene(None, Some(vec![9])); // SCENE_NODE_INVALID
    let path = std::env::temp_dir().join("mesh_tools_validate_should_not_exist.glb");
    let _ = std::fs::remove_file(&path);

    let Err(GltfError::InvalidData(msg)) = b.export_glb(path.to_str().unwrap()) else { panic!("expected InvalidData") };
    assert!(msg.contains("2 error(s)"), "{msg}");
    assert!(msg.contains("NODE_MESH_INVALID") && msg.contains("SCENE_NODE_INVALID"), "{msg}");
    assert!(!path.exists(), "no file may be written when validation fails");
    assert!(matches!(b.to_glb_bytes(), Err(GltfError::InvalidData(_))));
}

#[test]
fn unchecked_export_is_the_deliberate_escape_hatch() {
    let mut b = GltfBuilder::new();
    b.add_node(None, Some(7), None, None, None);
    assert!(b.to_glb_bytes().is_err());
    assert!(b.to_glb_bytes_unchecked().is_ok());

    let path = std::env::temp_dir().join("mesh_tools_validate_unchecked.glb");
    b.export_glb_unchecked(path.to_str().unwrap()).unwrap();
    assert!(path.exists());
    let _ = std::fs::remove_file(path);
}
