mod common;

use mesh_tools::{ExtensionTarget as T, GltfBuilder, GltfError};
use serde_json::{json, Value};

const EXT: &str = "VENDOR_test";

fn payload() -> Value {
    json!({ "k": [1, 2], "s": "x" })
}

fn is_invalid_input<T: std::fmt::Debug>(r: mesh_tools::Result<T>) -> bool {
    matches!(r, Err(GltfError::InvalidInput(_)))
}

/// Document with one of everything addressable. Indices: all 0 (mesh 0 has one primitive).
fn rich() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let tex = b.create_checkerboard_texture(4, 4, 2, [0; 3], [255; 3]).unwrap();
    let mat = b.create_basic_material(None, [1.0; 4]);
    assert_eq!((tex, mat), (0, 0));
    let mesh = b.create_box_with_material(1.0, Some(mat));
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));
    let joint = b.add_joint_node(None, None, None, None);
    b.add_skin(None, vec![joint], None, Some(joint)).unwrap();
    let anim = b.add_animation(None);
    b.add_translation_track(anim, node, &[0.0, 1.0], &[[0.0; 3], [1.0; 3]], mesh_tools::InterpolationType::Linear).unwrap();
    b
}

/// Raw JSON of an exported GLB (for objects the gltf crate cannot expose, like asset.extensions).
fn json_chunk(b: &GltfBuilder) -> Value {
    let bytes = b.to_glb_bytes().unwrap();
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&bytes[20..20 + len]).unwrap()
}

fn extras_of(raw: &Option<Box<serde_json::value::RawValue>>) -> Value {
    serde_json::from_str(raw.as_ref().expect("extras present").get()).unwrap()
}

/// Every target except Asset (checked separately) must carry both the extension and extras
/// when read back with the gltf crate.
#[test]
fn every_target_round_trips_extension_and_extras() {
    let mut b = rich();
    let targets = [
        T::Root,
        T::Asset,
        T::Scene(0),
        T::Node(0),
        T::Mesh(0),
        T::Primitive { mesh: 0, primitive: 0 },
        T::Material(0),
        T::PbrMetallicRoughness(0),
        T::Texture(0),
        T::Image(0),
        T::Sampler(0),
        T::Accessor(0),
        T::BufferView(0),
        T::Buffer(0),
        T::Skin(0),
        T::Animation(0),
        T::AnimationChannel { animation: 0, channel: 0 },
        T::AnimationSampler { animation: 0, sampler: 0 },
    ];
    for (i, t) in targets.iter().enumerate() {
        b.set_extension(*t, EXT, payload()).unwrap_or_else(|e| panic!("{t:?}: {e}"));
        b.set_extras(*t, json!({ "target": i })).unwrap_or_else(|e| panic!("{t:?}: {e}"));
    }
    assert_eq!(b.gltf.extensions_used.as_deref(), Some(&[EXT.to_string()][..]));
    assert!(b.validate().is_valid(), "{:?}", b.validate());

    let glb = common::parse_glb(&b);
    let r = glb.document.as_json();
    macro_rules! check {
        ($obj:expr, $i:expr) => {{
            let o = $obj;
            assert_eq!(o.extensions.as_ref().unwrap().others[EXT], payload(), "extension on target {}", $i);
            assert_eq!(extras_of(&o.extras), json!({ "target": $i }), "extras on target {}", $i);
        }};
    }
    // Root: extensions live in `root.extensions`.
    assert_eq!(r.extensions.as_ref().unwrap().others[EXT], payload());
    assert_eq!(extras_of(&r.extras), json!({ "target": 0 }));
    check!(&r.scenes[0], 2);
    check!(&r.nodes[0], 3);
    check!(&r.meshes[0], 4);
    check!(&r.meshes[0].primitives[0], 5);
    check!(&r.materials[0], 6);
    check!(r.materials[0].pbr_metallic_roughness.clone(), 7);
    check!(&r.textures[0], 8);
    check!(&r.images[0], 9);
    check!(&r.samplers[0], 10);
    check!(&r.accessors[0], 11);
    check!(&r.buffer_views[0], 12);
    check!(&r.buffers[0], 13);
    check!(&r.skins[0], 14);
    check!(&r.animations[0], 15);

    // Asset, animation channels and animation samplers: the gltf crate has no extension
    // accessor for these, so read the raw JSON.
    let raw = json_chunk(&b);
    for (obj, i) in [(&raw["animations"][0]["channels"][0], 16), (&raw["animations"][0]["samplers"][0], 17)] {
        assert_eq!(obj["extensions"][EXT], payload());
        assert_eq!(obj["extras"], json!({ "target": i }));
    }
    assert_eq!(raw["asset"]["extensions"][EXT], payload());
    assert_eq!(raw["asset"]["extras"], json!({ "target": 1 }));
    assert_eq!(raw["extensionsUsed"], json!([EXT]));
}

#[test]
fn out_of_range_targets_fail_atomically() {
    let mut b = rich();
    for t in [
        T::Scene(1),
        T::Node(7),
        T::Mesh(1),
        T::Primitive { mesh: 0, primitive: 1 },
        T::Primitive { mesh: 3, primitive: 0 },
        T::Material(1),
        T::Texture(1),
        T::Skin(1),
        T::Animation(1),
        T::AnimationChannel { animation: 0, channel: 1 },
        T::AnimationSampler { animation: 0, sampler: 1 },
        T::AnimationSampler { animation: 2, sampler: 0 },
    ] {
        assert!(is_invalid_input(b.set_extension(t, EXT, payload())), "{t:?}");
        assert!(is_invalid_input(b.set_extras(t, json!(1))), "{t:?}");
    }
    assert!(b.gltf.extensions_used.is_none(), "failed calls must not register extensions");
    // A material without pbrMetallicRoughness has no such target.
    b.gltf.materials.as_mut().unwrap()[0].pbr_metallic_roughness = None;
    assert!(is_invalid_input(b.set_extension(T::PbrMetallicRoughness(0), EXT, payload())));
    assert!(b.gltf.extensions_used.is_none());
}

#[test]
fn bad_payloads_and_names_are_rejected_without_side_effects() {
    let mut b = rich();
    for v in [json!(1), json!("s"), json!([1]), json!(null), json!(true)] {
        assert!(is_invalid_input(b.set_extension(T::Node(0), EXT, v)));
    }
    assert!(is_invalid_input(b.set_extension(T::Node(0), "", payload())));
    assert!(is_invalid_input(b.declare_extension_used("")));
    assert!(is_invalid_input(b.require_extension("")));
    assert!(b.gltf.extensions_used.is_none());
    assert!(b.gltf.extensions_required.is_none());
    assert!(b.gltf.nodes.as_ref().unwrap()[0].extensions.is_none());
}

#[test]
fn bookkeeping_is_idempotent_and_required_implies_used() {
    let mut b = rich();
    b.set_extension(T::Node(0), EXT, payload()).unwrap();
    b.set_extension(T::Scene(0), EXT, payload()).unwrap();
    b.declare_extension_used(EXT).unwrap();
    b.declare_extension_used("KHR_texture_transform").unwrap();
    b.require_extension("KHR_texture_transform").unwrap();
    b.require_extension("KHR_texture_transform").unwrap();
    assert_eq!(b.gltf.extensions_used.as_deref().unwrap(), [EXT, "KHR_texture_transform"]);
    assert_eq!(b.gltf.extensions_required.as_deref().unwrap(), ["KHR_texture_transform"]);

    // Setting again replaces the payload.
    b.set_extension(T::Node(0), EXT, json!({ "new": true })).unwrap();
    let glb = common::parse_glb(&b);
    assert_eq!(glb.document.as_json().nodes[0].extensions.as_ref().unwrap().others[EXT], json!({ "new": true }));
    assert_eq!(glb.document.as_json().extensions_required, vec!["KHR_texture_transform".to_string()]);
}

#[test]
fn require_extension_marks_it_used() {
    let mut b = GltfBuilder::new();
    b.require_extension("KHR_draco_mesh_compression").unwrap();
    assert_eq!(b.gltf.extensions_used.as_deref().unwrap(), ["KHR_draco_mesh_compression"]);
    assert_eq!(b.gltf.extensions_required.as_deref().unwrap(), ["KHR_draco_mesh_compression"]);
}

#[test]
fn root_extras_may_be_any_json_value() {
    for v in [json!([1, 2]), json!("text"), json!(3.5), json!(null), json!(true), json!({ "a": { "b": [] } })] {
        let mut b = GltfBuilder::new();
        b.set_extras(T::Root, v.clone()).unwrap();
        let glb = common::parse_glb(&b);
        // `null` extras read back as absent through the gltf crate; compare raw JSON then.
        assert_eq!(json_chunk(&b).get("extras").cloned().unwrap_or(Value::Null), v);
        if !v.is_null() {
            assert_eq!(extras_of(&glb.document.as_json().extras), v);
        }
    }
}

#[test]
fn extras_keep_non_ascii_and_nesting() {
    let mut b = GltfBuilder::new();
    let node = b.add_node(None, None, None, None, None);
    let v = json!({ "name": "Zażółć", "deep": [[1], { "x": null }], "num": -0.25 });
    b.set_extras(T::Node(node), v.clone()).unwrap();
    let glb = common::parse_glb(&b);
    assert_eq!(extras_of(&glb.document.as_json().nodes[0].extras), v);
}

// ---- validator codes ----

fn valid_with_ext() -> GltfBuilder {
    let mut b = rich();
    b.set_extension(T::Node(0), EXT, payload()).unwrap();
    b
}

#[test]
fn validator_extension_not_declared() {
    let mut b = valid_with_ext();
    b.gltf.extensions_used = None;
    let r = b.validate();
    assert!(r.has_error("EXTENSION_NOT_DECLARED"), "{r:?}");
    assert!(b.to_glb_bytes().is_err());

    // also for a material-level extension reached through MaterialExtensions.other
    let mut b = rich();
    b.set_extension(T::Material(0), "VENDOR_m", payload()).unwrap();
    b.gltf.extensions_used = None;
    assert!(b.validate().has_error("EXTENSION_NOT_DECLARED"));
}

#[test]
fn validator_extension_value_not_object() {
    let mut b = valid_with_ext();
    b.gltf.nodes.as_mut().unwrap()[0].extensions.as_mut().unwrap().insert(EXT.into(), json!(5));
    assert!(b.validate().has_error("EXTENSION_VALUE_NOT_OBJECT"));
}

#[test]
fn validator_used_list_problems() {
    let mut b = valid_with_ext();
    b.gltf.extensions_used = Some(vec![EXT.into(), EXT.into()]);
    assert!(b.validate().has_error("EXTENSIONS_USED_DUPLICATE"));

    let mut b = valid_with_ext();
    b.gltf.extensions_used = Some(vec![]);
    assert!(b.validate().has_error("EXTENSIONS_USED_EMPTY"));

    let mut b = valid_with_ext();
    b.gltf.extensions_required = Some(vec![]);
    assert!(b.validate().has_error("EXTENSIONS_REQUIRED_EMPTY"));

    let mut b = valid_with_ext();
    b.gltf.extensions_required = Some(vec!["VENDOR_never_used".into()]);
    assert!(b.validate().has_error("EXTENSIONS_REQUIRED_NOT_USED"));
}

#[test]
fn empty_lists_error_on_export_instead_of_being_stripped() {
    for field in ["used", "required"] {
        let mut b = rich();
        match field {
            "used" => b.gltf.extensions_used = Some(vec![]),
            _ => b.gltf.extensions_required = Some(vec![]),
        }
        assert!(matches!(b.to_glb_bytes(), Err(GltfError::InvalidData(_))));
        // the unchecked path keeps the (invalid) empty array visible rather than hiding it
        let bytes = b.to_glb_bytes_unchecked().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let key = if field == "used" { "\"extensionsUsed\":[]" } else { "\"extensionsRequired\":[]" };
        assert!(text.contains(key), "{key} missing");
    }
}

#[test]
fn validator_warnings() {
    let mut b = valid_with_ext();
    assert!(b.validate().warnings.iter().all(|w| !w.starts_with("EXTENSION_")));
    b.declare_extension_used("VENDOR_nothing_attached").unwrap();
    assert!(b.validate().has_warning("EXTENSION_DECLARED_UNUSED"));
    assert!(b.validate().is_valid());

    let mut b = rich();
    b.declare_extension_used("lowercase").unwrap();
    assert!(b.validate().has_warning("EXTENSION_NAME_INVALID"));
    for ok in ["KHR_materials_unlit", "EXT_mesh_gpu_instancing", "A1_x"] {
        assert!(valid_name_ok(ok), "{ok}");
    }
}

fn valid_name_ok(n: &str) -> bool {
    let mut b = rich();
    b.declare_extension_used(n).unwrap();
    !b.validate().has_warning("EXTENSION_NAME_INVALID")
}

#[test]
fn extras_are_not_searched_for_extensions() {
    // an "extensions" key inside extras is application data, not an extension use
    let mut b = rich();
    b.set_extras(T::Node(0), json!({ "extensions": { "NOT_AN_EXT": {} } })).unwrap();
    assert!(b.validate().is_valid());
    assert!(!b.validate().has_error("EXTENSION_NOT_DECLARED"));
}

// ---- mesh extras vs morph targetNames ----

fn morph_mesh(b: &mut GltfBuilder, names: Option<Vec<String>>) -> usize {
    let pos = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let set = mesh_tools::SkinSet { joints: mesh_tools::JointIndices::U8(vec![[0; 4]; 3]), weights: vec![[1.0, 0.0, 0.0, 0.0]; 3] };
    let mut p = b.create_skinned_primitive(&pos, Some(&[0, 1, 2]), None, None, &[set], 1, None).unwrap();
    let targets = vec![
        mesh_tools::MorphTarget::new().with_positions(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]),
        mesh_tools::MorphTarget::new().with_positions(vec![1.0; 9]),
    ];
    b.attach_morph_targets(&mut p, &targets).unwrap();
    b.add_morphable_mesh(None, vec![p], None, names).unwrap()
}

fn mesh_extras(b: &GltfBuilder) -> Value {
    extras_of(&common::parse_glb(b).document.as_json().meshes[0].extras)
}

#[test]
fn mesh_extras_after_target_names_are_merged() {
    let mut b = GltfBuilder::new();
    let m = morph_mesh(&mut b, Some(vec!["a".into(), "b".into()]));
    b.set_extras(T::Mesh(m), json!({ "author": "me" })).unwrap();
    assert_eq!(mesh_extras(&b), json!({ "author": "me", "targetNames": ["a", "b"] }));
    // replacing again keeps the names and drops the old keys
    b.set_extras(T::Mesh(m), json!({ "other": 1 })).unwrap();
    assert_eq!(mesh_extras(&b), json!({ "other": 1, "targetNames": ["a", "b"] }));
    assert!(b.validate().is_valid());
}

#[test]
fn target_names_after_mesh_extras_are_merged() {
    let mut b = GltfBuilder::new();
    let m = morph_mesh(&mut b, None);
    b.set_extras(T::Mesh(m), json!({ "author": "me" })).unwrap();
    b.set_mesh_target_names(m, vec!["a".into(), "b".into()]).unwrap();
    assert_eq!(mesh_extras(&b), json!({ "author": "me", "targetNames": ["a", "b"] }));
    // names can be replaced; other extras are kept
    b.set_mesh_target_names(m, vec!["c".into(), "d".into()]).unwrap();
    assert_eq!(mesh_extras(&b), json!({ "author": "me", "targetNames": ["c", "d"] }));
}

#[test]
fn mesh_extras_clashes_and_non_objects_fail_atomically() {
    let mut b = GltfBuilder::new();
    let m = morph_mesh(&mut b, Some(vec!["a".into(), "b".into()]));
    let before = mesh_extras(&b);
    assert!(is_invalid_input(b.set_extras(T::Mesh(m), json!({ "targetNames": ["x", "y"] }))));
    assert!(is_invalid_input(b.set_extras(T::Mesh(m), json!([1]))));
    assert_eq!(mesh_extras(&b), before);

    // non-object extras block adding names, and wrong counts / non-morph meshes are rejected
    let m2 = morph_mesh(&mut b, None);
    b.set_extras(T::Mesh(m2), json!("text")).unwrap();
    assert!(is_invalid_input(b.set_mesh_target_names(m2, vec!["a".into(), "b".into()])));
    assert_eq!(b.gltf.meshes.as_ref().unwrap()[m2].extras, Some(json!("text")));
    b.set_extras(T::Mesh(m2), json!({})).unwrap();
    assert!(is_invalid_input(b.set_mesh_target_names(m2, vec!["only one".into()])));
    assert!(is_invalid_input(b.set_mesh_target_names(99, vec![])));
    let plain = b.create_box(1.0);
    assert!(is_invalid_input(b.set_mesh_target_names(plain, vec!["a".into()])));
}

#[test]
fn morph_target_names_still_written_by_add_morphable_mesh() {
    let mut b = GltfBuilder::new();
    morph_mesh(&mut b, Some(vec!["a".into(), "b".into()]));
    assert_eq!(mesh_extras(&b), json!({ "targetNames": ["a", "b"] }));
}

// ---- specular-glossiness migration ----

#[test]
fn specular_glossiness_output_is_unchanged() {
    let mut b = GltfBuilder::new();
    let m = b.create_specular_material(Some("S".into()), [0.5, 0.25, 0.125, 1.0], [0.5, 0.25, 0.75], 0.25);
    assert_eq!(m, 0);
    b.create_specular_material(None, [1.0; 4], [0.0; 3], 1.0);
    assert_eq!(b.gltf.extensions_used.as_deref().unwrap(), ["KHR_materials_pbrSpecularGlossiness"], "registered once");

    let raw = json_chunk(&b);
    assert_eq!(raw["extensionsUsed"], json!(["KHR_materials_pbrSpecularGlossiness"]));
    assert_eq!(
        raw["materials"][0],
        json!({
            "name": "S",
            "alphaMode": "OPAQUE",
            "doubleSided": false,
            "extensions": { "KHR_materials_pbrSpecularGlossiness": {
                "diffuseFactor": [0.5, 0.25, 0.125, 1.0],
                "specularFactor": [0.5, 0.25, 0.75],
                "glossinessFactor": 0.25
            } }
        })
    );

    let glb = common::parse_glb(&b);
    let sg = glb.document.materials().next().unwrap().pbr_specular_glossiness().unwrap();
    assert_eq!(sg.diffuse_factor(), [0.5, 0.25, 0.125, 1.0]);
    assert_eq!(sg.specular_factor(), [0.5, 0.25, 0.75]);
    assert_eq!(sg.glossiness_factor(), 0.25);
    assert!(b.validate().is_valid());
}

#[test]
fn spec_gloss_extension_via_registry_uses_typed_field() {
    let mut b = GltfBuilder::new();
    let m = b.create_basic_material(None, [1.0; 4]);
    b.set_extension(T::Material(m), "KHR_materials_pbrSpecularGlossiness", json!({ "glossinessFactor": 0.5 })).unwrap();
    let sg = common::parse_glb(&b).document.materials().next().unwrap().pbr_specular_glossiness().unwrap().glossiness_factor();
    assert_eq!(sg, 0.5);
    // a payload that does not fit the extension schema is an error and changes nothing
    assert!(is_invalid_input(b.set_extension(
        T::Material(m),
        "KHR_materials_pbrSpecularGlossiness",
        json!({ "glossinessFactor": "high" })
    )));
    assert_eq!(b.gltf.materials.as_ref().unwrap()[m].extensions.as_ref().unwrap().pbr_specular_glossiness.as_ref().unwrap().glossiness_factor, Some(0.5));
}

#[test]
fn rejected_spec_gloss_payload_on_fresh_material_leaves_no_trace() {
    let mut b = GltfBuilder::new();
    let m = b.create_basic_material(None, [1.0; 4]);
    assert!(is_invalid_input(b.set_extension(
        T::Material(m),
        "KHR_materials_pbrSpecularGlossiness",
        json!({ "glossinessFactor": "high" })
    )));
    assert!(b.gltf.materials.as_ref().unwrap()[m].extensions.is_none());
    assert!(b.gltf.extensions_used.is_none());
}
