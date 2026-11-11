//! Multiple primitives per mesh, `add_mesh_checked`, and mesh/primitive extras.

mod common;

use mesh_tools::mesh::MeshBuilder;
use mesh_tools::{ExtensionTarget, GltfBuilder, GltfError, MorphTarget, Primitive, PrimitiveMode, PrimitiveSpec};
use serde_json::json;

const TRI: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

fn mesh_count(b: &GltfBuilder) -> usize {
    b.gltf.meshes.as_ref().map_or(0, |m| m.len())
}

fn tri(b: &mut GltfBuilder, material: Option<usize>) -> Primitive {
    b.create_primitive(PrimitiveSpec { positions: TRI.to_vec(), material, ..Default::default() }).unwrap()
}

#[test]
fn two_primitives_with_different_materials_and_modes() {
    let mut b = GltfBuilder::new();
    let red = b.create_basic_material(Some("red".into()), [1.0, 0.0, 0.0, 1.0]);
    let blue = b.create_basic_material(Some("blue".into()), [0.0, 0.0, 1.0, 1.0]);
    let p0 = tri(&mut b, Some(red));
    let mut p1 = b.create_lines(&TRI, Some(&[0, 1, 1, 2]), Some(blue)).unwrap();
    p1.extras = Some(json!({"role": "outline"}));
    let m = b.add_mesh_checked(Some("Two".into()), vec![p0, p1], Some(json!({"author": "x"}))).unwrap();
    let n = b.add_node(None, Some(m), None, None, None);
    b.add_scene(None, Some(vec![n]));

    let glb = common::parse_glb(&b);
    let mesh = glb.meshes().next().unwrap();
    assert_eq!(mesh.primitives().count(), 2);
    let prims: Vec<_> = mesh.primitives().collect();
    assert_eq!(prims[0].material().index(), Some(red));
    assert_eq!(prims[1].material().index(), Some(blue));
    assert_eq!(prims[0].mode(), gltf::mesh::Mode::Triangles);
    assert_eq!(prims[1].mode(), gltf::mesh::Mode::Lines);
    assert_eq!(mesh.extras().as_ref().unwrap().get(), r#"{"author":"x"}"#);
    assert_eq!(prims[1].extras().as_ref().unwrap().get(), r#"{"role":"outline"}"#);
    assert!(prims[0].extras().is_none());
}

#[test]
fn add_mesh_checked_rejects_bad_input_and_leaves_the_document_unchanged() {
    let mut b = GltfBuilder::new();
    let good = tri(&mut b, None);
    assert!(matches!(b.add_mesh_checked(None, vec![], None), Err(GltfError::InvalidInput(_))));

    let mut bad_acc = tri(&mut b, None);
    bad_acc.attributes.insert("NORMAL".into(), 99);
    let mut bad_mat = tri(&mut b, None);
    bad_mat.material = Some(5);
    let mut bad_mode = tri(&mut b, None);
    bad_mode.mode = Some(11);
    let mut bad_name = tri(&mut b, None);
    let pos = bad_name.attributes["POSITION"];
    bad_name.attributes.insert("FOO".into(), pos);
    let mut bad_count = tri(&mut b, None);
    bad_count.mode = Some(1); // 3 vertices cannot be lines
    let mut bad_type = tri(&mut b, None);
    let pos = bad_type.attributes["POSITION"];
    bad_type.attributes.insert("TEXCOORD_0".into(), pos); // VEC3 is not a texcoord
    for (label, p) in [
        ("accessor", bad_acc),
        ("material", bad_mat),
        ("mode", bad_mode),
        ("name", bad_name),
        ("count", bad_count),
        ("type", bad_type),
    ] {
        let r = b.add_mesh_checked(None, vec![good_clone(&good), p], None);
        assert!(matches!(r, Err(GltfError::InvalidInput(_))), "{label}: {r:?}");
        assert_eq!(mesh_count(&b), 0, "{label} must not add a mesh");
    }
    // The error names the primitive.
    let mut p = tri(&mut b, None);
    p.material = Some(5);
    let msg = format!("{}", b.add_mesh_checked(None, vec![good_clone(&good), p], None).unwrap_err());
    assert!(msg.contains("primitive 1"), "{msg}");
}

fn good_clone(p: &Primitive) -> Primitive {
    let mut c = Primitive::default();
    c.attributes = p.attributes.clone();
    c.indices = p.indices;
    c.material = p.material;
    c.mode = p.mode;
    c
}

#[test]
fn set_mesh_and_primitive_extras_round_trip_and_errors_are_atomic() {
    let mut b = GltfBuilder::new();
    let p = tri(&mut b, None);
    let m = b.add_mesh_checked(None, vec![p], None).unwrap();
    b.set_mesh_extras(m, json!([1, 2])).unwrap();
    b.set_primitive_extras(m, 0, json!({"k": true})).unwrap();
    assert!(b.set_mesh_extras(9, json!({})).is_err());
    assert!(b.set_primitive_extras(m, 3, json!({})).is_err());
    assert!(b.set_primitive_extras(9, 0, json!({})).is_err());
    let glb = common::parse_glb(&b);
    let mesh = glb.meshes().next().unwrap();
    assert_eq!(mesh.extras().as_ref().unwrap().get(), "[1,2]");
    assert_eq!(mesh.primitives().next().unwrap().extras().as_ref().unwrap().get(), r#"{"k":true}"#);
    // Same slots as the generic registry.
    assert_eq!(b.gltf.meshes.as_ref().unwrap()[0].extras, Some(json!([1, 2])));
    b.set_extras(ExtensionTarget::Mesh(m), json!(null)).unwrap();
}

#[test]
fn mesh_builder_tangents_colors_mode_extras_and_u32_indices() {
    let mut b = GltfBuilder::new();
    let m = MeshBuilder::new(Some("MB".into()))
        .with_positions(TRI.to_vec())
        .with_normals(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0])
        .with_tangents(vec![1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, -1.0])
        .with_colors(vec![mesh_tools::ColorData::Rgb(vec![1.0; 9])])
        .with_indices_u32(vec![0, 1, 2])
        .with_mode(PrimitiveMode::Triangles)
        .with_primitive_extras(json!({"p": 1}))
        .with_mesh_extras(json!({"m": 2}))
        .build_into(&mut b)
        .unwrap();
    let glb = common::parse_glb(&b);
    let mesh = glb.meshes().nth(m).unwrap();
    assert_eq!(mesh.extras().as_ref().unwrap().get(), r#"{"m":2}"#);
    let prim = mesh.primitives().next().unwrap();
    assert_eq!(prim.extras().as_ref().unwrap().get(), r#"{"p":1}"#);
    assert_eq!(prim.indices().unwrap().data_type(), gltf::accessor::DataType::U32);
    assert!(prim.get(&gltf::Semantic::Tangents).is_some() && prim.get(&gltf::Semantic::Colors(0)).is_some());
}

#[test]
fn mesh_builder_lines_mode_checks_counts_and_is_atomic() {
    let mut b = GltfBuilder::new();
    let r = MeshBuilder::new(None).with_positions(TRI.to_vec()).with_mode(PrimitiveMode::Lines).build_into(&mut b);
    assert!(r.is_err());
    assert_eq!(b.gltf.accessors.as_ref().map_or(0, |a| a.len()), 0);
    assert_eq!(mesh_count(&b), 0);
    let ok = MeshBuilder::new(None).with_positions(TRI.to_vec()).with_indices(vec![0, 1, 1, 2]).with_mode(PrimitiveMode::Lines).build_into(&mut b);
    assert!(ok.is_ok());
}

#[test]
fn mesh_builder_extras_do_not_clobber_morph_target_names() {
    let mut b = GltfBuilder::new();
    let target = || MorphTarget::new().with_positions(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
    let m = MeshBuilder::new(None)
        .with_positions(TRI.to_vec())
        .with_morph_targets(vec![target()])
        .with_target_names(vec!["up".into()])
        .with_mesh_extras(json!({"k": 1}))
        .build_into(&mut b)
        .unwrap();
    assert_eq!(b.gltf.meshes.as_ref().unwrap()[m].extras, Some(json!({"k": 1, "targetNames": ["up"]})));
    // Non-object extras with target names, or a clashing key, fail before anything is written.
    let before = b.gltf.accessors.as_ref().unwrap().len();
    for extras in [json!(3), json!({"targetNames": []})] {
        let r = MeshBuilder::new(None)
            .with_positions(TRI.to_vec())
            .with_morph_targets(vec![target()])
            .with_target_names(vec!["up".into()])
            .with_mesh_extras(extras)
            .build_into(&mut b);
        assert!(r.is_err());
    }
    assert_eq!(b.gltf.accessors.as_ref().unwrap().len(), before);
    assert_eq!(mesh_count(&b), 1);
}
