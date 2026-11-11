//! Validation corpus: one document per README feature, checked by
//! 1. the in-crate validator and the independent `gltf` reader (always),
//! 2. the Khronos glTF-Validator (CI, or locally with `GLTF_VALIDATOR=/path/to/gltf_validator`),
//! 3. negative cross-checks: documents broken through the public `gltf` field must be rejected
//!    by both validators.
//!
//! The Khronos tests are `#[ignore]`d so a plain `cargo test` stays offline, but they never pass
//! silently: run with `--ignored` and they FAIL when `GLTF_VALIDATOR` is unset or unusable. CI
//! runs them that way. See docs/VALIDATION.md.
//!
//! Later milestones add their documents to `corpus()`.
#![allow(deprecated)] // the deprecated shims are still covered here

mod common;

use mesh_tools::mesh::MeshBuilder;
use mesh_tools::{AccessorSpec, AccessorType, AnimationPath, Bounds, SparseSpec, CubicKeyframe, GltfBuilder, InterpolationType, MorphTarget, NormalizedComponent, Triangle};
use mesh_tools::compat::{point3, vector2, vector3};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------------------------

fn scene_of(mut b: GltfBuilder, mesh: usize) -> GltfBuilder {
    let node = b.add_node(Some("N".into()), Some(mesh), None, None, None);
    b.add_scene(Some("S".into()), Some(vec![node]));
    b
}

/// A 2x2 quad on the XY plane (4 vertices, 2 triangles, u16 indices).
fn quad_mesh(name: &str) -> MeshBuilder {
    MeshBuilder::new(Some(name.into()))
        .with_positions(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0])
        .with_normals(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0])
        .with_indices(vec![0, 1, 2, 0, 2, 3])
}

fn primitives_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let mat = b.create_basic_material(None, [0.5, 0.5, 0.5, 1.0]);
    let meshes = [
        b.create_box_with_material(1.0, Some(mat)),
        b.create_plane(2.0, 2.0, 2, 2, Some(mat)),
        b.create_sphere(1.0, 8, 6, Some(mat)),
        b.create_cylinder(0.5, 0.5, 1.0, 8, 1, false, Some(mat)),
        b.create_cone(0.5, 1.0, 8, 1, false, Some(mat)),
        b.create_torus(1.0, 0.25, 8, 8, Some(mat)),
        b.create_icosahedron(1.0, Some(mat)),
    ];
    let nodes: Vec<usize> = meshes
        .iter()
        .enumerate()
        .map(|(i, &m)| b.add_node(None, Some(m), Some([i as f32 * 3.0, 0.0, 0.0]), None, None))
        .collect();
    b.add_scene(None, Some(nodes));
    b
}

fn custom_mesh_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let positions = [point3::new(0.0, 0.0, 0.0), point3::new(1.0, 0.0, 0.0), point3::new(0.0, 1.0, 0.0)];
    let normals = vec![vector3::new(0.0, 0.0, 1.0); 3];
    let uv = vec![vector2::new(0.0, 0.0), vector2::new(1.0, 0.0), vector2::new(0.0, 1.0)];
    let mesh = b.create_simple_mesh(Some("Tri".into()), &positions, &[Triangle::new(0, 1, 2)], Some(normals), Some(uv), None);
    scene_of(b, mesh)
}

fn materials_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let a = b.add_material(Some("A".into()), Some([1.0, 0.0, 0.0, 1.0]), Some(0.0), Some(0.5), Some(true));
    let m = b.create_metallic_material(Some("Gold".into()), [1.0, 0.8, 0.2, 1.0], 1.0, 0.2);
    let blend = b.add_textured_material(
        Some("Blend".into()), None, None, None, None, None, Some([0.1, 0.0, 0.0]),
        Some(0.0), Some(1.0), Some("BLEND".into()), None, Some(false),
    ).unwrap();
    let mask = b.add_textured_material(
        Some("Mask".into()), None, None, None, None, None, None,
        Some(0.0), Some(1.0), Some("MASK".into()), Some(0.4), Some(true),
    ).unwrap();
    let nodes: Vec<usize> = [a, m, blend, mask]
        .iter()
        .enumerate()
        .map(|(i, &mat)| {
            let mesh = b.create_box_with_material(1.0, Some(mat));
            b.add_node(None, Some(mesh), Some([i as f32 * 2.0, 0.0, 0.0]), None, None)
        })
        .collect();
    b.add_scene(None, Some(nodes));
    b
}

fn textured_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let tex = b.create_checkerboard_texture(16, 16, 4, [255, 255, 255], [0, 0, 0]).unwrap();
    let uv = b.create_uv_test_texture(16, 16).unwrap();
    let mat = b.add_textured_material(
        Some("Tex".into()), Some(tex), None, Some(uv), Some(tex), Some(uv), Some([1.0, 1.0, 1.0]),
        Some(1.0), Some(1.0), None, None, None,
    ).unwrap();
    let mesh = quad_mesh("TexQuad")
        .with_texcoords(vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0])
        .with_material(mat)
        .build_into(&mut b)
        .unwrap();
    scene_of(b, mesh)
}

/// A textured quad whose image is a base64 data URI (adds nothing to the BIN chunk).
fn image_data_uri_doc() -> GltfBuilder {
    use mesh_tools::{material::MaterialBuilder, texture::{create_uv_test_pattern, image_to_bytes, TextureFormat}};
    let mut b = GltfBuilder::new();
    let png = image_to_bytes(&create_uv_test_pattern(16, 16).unwrap(), TextureFormat::PNG).unwrap();
    let img = b.add_image_data_uri(Some("uv".into()), TextureFormat::PNG, &png).unwrap();
    let tex = b.add_texture(None, img, None).unwrap();
    let mat = b.add_material_from(MaterialBuilder::new(Some("DataUri".into())).with_base_color_texture(tex).try_build().unwrap()).unwrap();
    let mesh = quad_mesh("DataUriQuad")
        .with_texcoords(vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0])
        .with_material(mat)
        .build_into(&mut b)
        .unwrap();
    scene_of(b, mesh)
}

/// Specular-glossiness with a metallic-roughness fallback and a texture on TEXCOORD_1.
fn specular_glossiness_doc() -> GltfBuilder {
    use mesh_tools::{material::MaterialBuilder, PrimitiveSpec, SpecularGlossiness, TexCoords, TextureRef};
    let mut b = GltfBuilder::new();
    let tex = b.create_checkerboard_texture(8, 8, 2, [200, 200, 200], [20, 20, 20]).unwrap();
    let m = MaterialBuilder::new(Some("SG".into()))
        .with_base_color([0.8, 0.8, 0.8, 1.0])
        .with_specular_glossiness(SpecularGlossiness {
            diffuse_factor: Some([0.8, 0.8, 0.8, 1.0]),
            diffuse_texture: Some(TextureRef::new(tex).with_tex_coord(1)),
            specular_factor: Some([0.5, 0.5, 0.5]),
            glossiness_factor: Some(0.7),
            ..Default::default()
        })
        .try_build()
        .unwrap();
    let m = b.add_material_from(m).unwrap();
    let uv = || TexCoords::F32(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
    let p = b
        .create_primitive(PrimitiveSpec {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals: Some(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]),
            texcoords: vec![uv(), uv()],
            material: Some(m),
            ..Default::default()
        })
        .unwrap();
    let mesh = b.add_mesh_checked(None, vec![p], None).unwrap();
    scene_of(b, mesh)
}

/// Every core material slot, with texCoord 0 and 1, scale/strength and alpha mask.
fn pbr_slots_doc() -> GltfBuilder {
    use mesh_tools::{material::MaterialBuilder, AlphaMode, PrimitiveSpec, TexCoords, TextureRef};
    let mut b = GltfBuilder::new();
    let t: Vec<usize> = (0..5).map(|_| b.create_uv_test_texture(16, 16).unwrap()).collect();
    let m = MaterialBuilder::new(Some("Slots".into()))
        .with_base_color([0.1, 0.2, 0.3, 0.4])
        .with_metallic_factor(0.6)
        .with_roughness_factor(0.7)
        .with_emissive_factor([0.5, 0.25, 0.125])
        .with_base_color_texture(t[0])
        .with_metallic_roughness_texture(TextureRef::new(t[1]).with_tex_coord(1))
        .with_normal_texture(t[2], 0.75)
        .with_occlusion_texture(TextureRef::new(t[3]).with_tex_coord(1), 0.4)
        .with_emissive_texture(t[4])
        .with_alpha(AlphaMode::Mask { cutoff: 0.3 })
        .try_build()
        .unwrap();
    let m = b.add_material_from(m).unwrap();
    let uv = || TexCoords::F32(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
    let p = b
        .create_primitive(PrimitiveSpec {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals: Some(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]),
            texcoords: vec![uv(), TexCoords::U8(vec![0, 0, 255, 0, 0, 255])],
            material: Some(m),
            ..Default::default()
        })
        .unwrap();
    let mesh = b.add_mesh_checked(None, vec![p], None).unwrap();
    scene_of(b, mesh)
}

fn hierarchy_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(0.5);
    let q = GltfBuilder::quaternion_from_euler(0.0, 0.5, 0.0);
    let root = b.add_node(Some("Root".into()), None, Some([0.0, 1.0, 0.0]), Some(q), Some([2.0, 2.0, 2.0]));
    let child = b.add_node(Some("Child".into()), Some(mesh), Some([1.0, 0.0, 0.0]), None, None);
    b.add_child_to_node(root, child).unwrap();
    b.add_scene(Some("Main".into()), Some(vec![root]));
    b
}

fn instancing_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(0.5);
    let nodes: Vec<usize> = (0..3)
        .map(|i| b.add_node(None, Some(mesh), Some([i as f32, 0.0, 0.0]), None, None))
        .collect();
    b.add_scene(None, Some(nodes));
    b
}

/// Two scenes with the second as default, a mirrored matrix node, and a TRS node.
fn scenes_matrix_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(0.5);
    let mut mirror = GltfBuilder::matrix_from_trs([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], [2.0; 3]);
    mirror[0] = -2.0;
    let m = b.add_node_with_matrix(Some("M".into()), Some(mesh), mirror).unwrap();
    let t = b.add_node(Some("T".into()), Some(mesh), Some([0.0, 1.0, 0.0]), None, None);
    b.add_scene_checked(Some("A".into()), vec![m]).unwrap();
    let s = b.add_scene_checked(Some("B".into()), vec![t]).unwrap();
    b.set_default_scene(s).unwrap();
    b
}

fn animation_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let node = b.add_node(Some("Anim".into()), Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));
    let anim = b.add_animation(Some("Move".into()));
    b.add_translation_track(anim, node, &[0.0, 1.0], &[[0.0; 3], [1.0, 0.0, 0.0]], InterpolationType::Linear).unwrap();
    b.add_rotation_track(
        anim, node, &[0.0, 1.0], &[[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0]], InterpolationType::Step,
    )
    .unwrap();
    let k = |v: [f32; 3]| CubicKeyframe { in_tangent: [0.0; 3], value: v, out_tangent: [0.0; 3] };
    b.add_cubic_scale_track(anim, node, &[0.0, 1.0], &[k([1.0; 3]), k([2.0; 3])]).unwrap();
    b
}

/// Normalized integer rotation and weights outputs, linear and cubic, on a morphable mesh.
fn animation_normalized_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let up = MorphTarget::new().with_positions(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
    let mesh = quad_mesh("Morph").with_morph_targets(vec![up]).build_into(&mut b).unwrap();
    let a = b.add_node(Some("A".into()), Some(mesh), None, None, None);
    let c = b.add_node(Some("C".into()), Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![a, c]));
    let lin = InterpolationType::Linear;
    let q = [[0.0, 0.0, 0.0, 1.0], [0.0, 0.70710678, 0.0, 0.70710678]];
    let anim = b.add_animation(Some("Linear".into()));
    b.add_rotation_track_normalized(anim, a, &[0.0, 1.0], &q, lin, NormalizedComponent::I8).unwrap();
    b.add_weights_track_normalized(anim, a, &[0.0, 1.0], &[vec![0.0], vec![1.0]], lin, NormalizedComponent::U8).unwrap();
    let anim = b.add_animation(Some("Cubic".into()));
    let kq = |v: [f32; 4]| CubicKeyframe { in_tangent: [0.0; 4], value: v, out_tangent: [0.0; 4] };
    b.add_cubic_rotation_track_normalized(anim, c, &[0.0, 1.0], &[kq(q[0]), kq(q[1])], NormalizedComponent::I16).unwrap();
    let kw = |v: f32| CubicKeyframe { in_tangent: vec![0.0], value: vec![v], out_tangent: vec![0.0] };
    b.add_cubic_weights_track_normalized(anim, c, &[0.0, 1.0], &[kw(0.0), kw(1.0)], NormalizedComponent::U16).unwrap();
    let anim = b.add_animation(Some("Unsigned".into()));
    b.add_rotation_track_normalized(anim, a, &[0.0, 1.0], &q, lin, NormalizedComponent::U16).unwrap();
    b.add_weights_track_normalized(anim, a, &[0.0, 1.0], &[vec![-1.0], vec![0.5]], lin, NormalizedComponent::I16).unwrap();
    b
}

/// One translation sampler driving three nodes.
fn animation_shared_sampler_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let nodes: Vec<usize> = (0..3).map(|_| b.add_node(None, Some(mesh), None, None, None)).collect();
    b.add_scene(None, Some(nodes.clone()));
    let anim = b.add_animation(Some("Shared".into()));
    let (_, sampler) = b
        .add_translation_track(anim, nodes[0], &[0.0, 1.0, 2.0], &[[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]], InterpolationType::Linear)
        .unwrap();
    for &n in &nodes[1..] {
        b.add_channel_with_sampler(anim, sampler, n, AnimationPath::Translation).unwrap();
    }
    b
}

fn skinned_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let names: Vec<String> = ["A", "B"].iter().map(|s| s.to_string()).collect();
    let joints = b.create_bone_chain(&names, &[[0.0; 3], [0.0, 1.0, 0.0]]).unwrap();
    let skin = b.add_skin_from_joints(None, joints.clone(), Some(joints[0])).unwrap();
    let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let js = [[0u16, 1, 0, 0]; 3];
    let ws = [[0.5f32, 0.5, 0.0, 0.0]; 3];
    let mesh = b.create_skinned_mesh(None, &positions, &[0, 1, 2], None, None, &js, &ws, None).unwrap();
    let node = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.add_scene(None, Some(vec![node, joints[0]]));
    b
}

/// Skinned triangle with normalized `u8` weights (200 + 55 = 255) on `u8` joints, computed IBMs.
fn skinned_u8_weights_doc() -> GltfBuilder {
    use mesh_tools::{quantize_u8, JointIndices, SkinSet};
    let mut b = GltfBuilder::new();
    let names: Vec<String> = ["A", "B"].iter().map(|s| s.to_string()).collect();
    let joints = b.create_bone_chain(&names, &[[0.0; 3], [0.0, 1.0, 0.0]]).unwrap();
    let skin = b.add_skin_from_joints(None, joints.clone(), Some(joints[0])).unwrap();
    let w = quantize_u8(&[[200.0 / 255.0, 55.0 / 255.0, 0.0, 0.0]; 3]).unwrap();
    let set = SkinSet::new(JointIndices::U8(vec![[0, 1, 0, 0]; 3]), w);
    let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let prim = b.create_skinned_primitive(&positions, Some(&[0, 1, 2]), None, None, &[set], 2, None).unwrap();
    let mesh = b.add_mesh(None, vec![prim]);
    let node = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.add_scene(None, Some(vec![node, joints[0]]));
    b
}

/// Three joints, two influence sets mixing `u16` and `f32` weights, a skin without inverse bind
/// matrices, and the skin attached with `set_node_skin`.
fn skinned_mixed_sets_doc() -> GltfBuilder {
    use mesh_tools::{JointIndices, SkinSet};
    let mut b = GltfBuilder::new();
    let names: Vec<String> = ["A", "B", "C"].iter().map(|s| s.to_string()).collect();
    let joints = b.create_bone_chain(&names, &[[0.0; 3], [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]]).unwrap();
    let skin = b.add_skin(None, joints.clone(), None, Some(joints[0])).unwrap();

    // Set 0: u16 halves, 32768 + 32767 = 65535 exactly; set 1 carries zero f32 weights.
    let s0 = SkinSet::new(JointIndices::U8(vec![[0, 1, 0, 0]; 3]), vec![[32768u16, 32767, 0, 0]; 3]);
    let s1 = SkinSet::new(JointIndices::U16(vec![[0; 4]; 3]), vec![[0.0f32; 4]; 3]);
    let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let prim = b.create_skinned_primitive(&positions, Some(&[0, 1, 2]), None, None, &[s0, s1], 3, None).unwrap();
    let mesh = b.add_mesh(None, vec![prim]);
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.set_node_skin(node, skin).unwrap();
    b.add_scene(None, Some(vec![node, joints[0]]));
    b
}

/// Asset metadata plus extras and a known extension (KHR_materials_unlit has an empty payload).
fn extensions_doc() -> GltfBuilder {
    use mesh_tools::ExtensionTarget as T;
    let mut b = GltfBuilder::new();
    b.set_copyright("(c) Example").unwrap();
    b.set_generator("corpus").unwrap();
    b.set_min_version("2.0").unwrap();
    let mat = b.create_basic_material(Some("Unlit".into()), [0.2, 0.4, 0.6, 1.0]);
    let mesh = b.create_box_with_material(1.0, Some(mat));
    let mut b = scene_of(b, mesh);
    b.set_extension(T::Material(mat), "KHR_materials_unlit", serde_json::json!({})).unwrap();
    b.set_extras(T::Root, serde_json::json!(["any", "json", 1])).unwrap();
    b.set_extras(T::Asset, serde_json::json!({ "note": "asset extras" })).unwrap();
    for t in [T::Scene(0), T::Node(0), T::Mesh(mesh), T::Material(mat), T::Accessor(0), T::BufferView(0), T::Buffer(0)] {
        b.set_extras(t, serde_json::json!({ "k": 1 })).unwrap();
    }
    b
}

fn morph_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let up = MorphTarget::new().with_positions(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
    let mesh = quad_mesh("Morph")
        .with_morph_targets(vec![up])
        .with_default_weights(vec![0.5])
        .with_target_names(vec!["Up".into()])
        .build_into(&mut b)
        .unwrap();
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.set_node_weights(node, vec![0.25]).unwrap();
    b.add_scene(None, Some(vec![node]));
    let anim = b.add_animation(None);
    b.add_weights_track(anim, node, &[0.0, 1.0], &[vec![0.0], vec![1.0]], InterpolationType::Linear).unwrap();
    b
}

/// One document per README feature. Add new documents here as features land.
/// A mesh plus sparse accessors: a base with substitutions and a zero base (no bufferView).
fn sparse_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let m = b.create_box(1.0);
    let base: Vec<f32> = (0..15).map(|v| v as f32).collect();
    let spec = || AccessorSpec::new().with_bounds(Bounds::Compute);
    b.add_sparse_accessor(5, AccessorType::Vec3, Some(&base[..]), SparseSpec::new(vec![1, 3], vec![9.0; 6]), spec()).unwrap();
    b.add_sparse_accessor::<i16>(4, AccessorType::Vec2, None, SparseSpec::new(vec![2], vec![-5, 9]), spec()).unwrap();
    scene_of(b, m)
}

/// Every primitive mode in one mesh, plus tangents, colors, normalized texcoords and u32 indices.
fn primitive_modes_doc() -> GltfBuilder {
    use mesh_tools::{ColorData, Indices, PrimitiveSpec, TexCoords};
    let mut b = GltfBuilder::new();
    let sq = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
    let rich = b
        .create_primitive(PrimitiveSpec {
            positions: sq.to_vec(),
            indices: Some(Indices::U32(vec![0, 1, 2, 0, 2, 3])),
            normals: Some([0.0, 0.0, 1.0].repeat(4)),
            tangents: Some([1.0, 0.0, 0.0, 1.0].repeat(4)),
            texcoords: vec![TexCoords::U8(vec![0, 0, 255, 0, 255, 255, 0, 255])],
            colors: vec![ColorData::RgbU8([255, 128, 0].repeat(4)), ColorData::Rgba([1.0, 1.0, 1.0, 0.5].repeat(4))],
            extras: Some(serde_json::json!({ "kind": "rich" })),
            ..Default::default()
        })
        .unwrap();
    let prims = vec![
        rich,
        b.create_points(&sq, None).unwrap(),
        b.create_lines(&sq, Some(&[0, 2, 1, 3]), None).unwrap(),
        b.create_line_strip(&sq, false, None, None).unwrap(),
        b.create_line_strip(&sq, true, None, None).unwrap(),
        b.create_triangle_strip(&sq, None, None).unwrap(),
        b.create_triangle_fan(&sq, None, None).unwrap(),
    ];
    let m = b.add_mesh_checked(Some("Modes".into()), prims, Some(serde_json::json!({ "doc": "modes" }))).unwrap();
    scene_of(b, m)
}

/// Perspective cameras (finite and infinite far plane) on nodes looking down -Z.
fn cameras_perspective_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let boxn = b.add_node(Some("Box".into()), Some(mesh), None, None, None);
    let mut nodes = vec![boxn];
    let finite = b.add_perspective_camera(Some("Finite".into()), 0.8, 0.1, Some(100.0), Some(1.5)).unwrap();
    let infinite = b.add_perspective_camera(None, 1.0, 0.25, None, None).unwrap();
    for (i, c) in [finite, infinite].into_iter().enumerate() {
        let n = b.add_node(None, None, Some([i as f32, 0.0, 5.0]), None, None);
        b.set_node_camera(n, c).unwrap();
        nodes.push(n);
    }
    b.add_scene(None, Some(nodes));
    b
}

/// An orthographic camera under a rotated parent, so it follows the node hierarchy.
fn cameras_orthographic_doc() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let boxn = b.add_node(None, Some(mesh), None, None, None);
    let q = GltfBuilder::quaternion_from_euler(0.0, 0.5, 0.0);
    let rig = b.add_node(Some("Rig".into()), None, None, Some(q), None);
    let cam = b.add_orthographic_camera(Some("Ortho".into()), 2.0, 1.0, 0.0, 50.0).unwrap();
    let cn = b.add_node(None, None, Some([0.0, 0.0, 5.0]), None, None);
    b.set_node_camera(cn, cam).unwrap();
    b.add_child_to_node(rig, cn).unwrap();
    b.add_scene(None, Some(vec![boxn, rig]));
    b
}

/// Camera extras on the camera and both projection objects, and a camera on a mesh node.
fn cameras_extras_doc() -> GltfBuilder {
    use mesh_tools::ExtensionTarget as T;
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let p = b.add_perspective_camera(None, 0.5, 0.1, Some(10.0), None).unwrap();
    let o = b.add_orthographic_camera(None, 1.0, 1.0, 0.1, 10.0).unwrap();
    let n = b.add_node(None, Some(mesh), None, None, None);
    b.set_node_camera(n, p).unwrap();
    let n2 = b.add_node(None, None, Some([0.0, 0.0, 3.0]), None, None);
    b.set_node_camera(n2, o).unwrap();
    for t in [T::Camera(p), T::CameraPerspective(p), T::Camera(o), T::CameraOrthographic(o)] {
        b.set_extras(t, serde_json::json!({ "k": 1 })).unwrap();
    }
    b.add_scene(None, Some(vec![n, n2]));
    b
}

/// Extensions and extras on every object kind, including texture infos, sparse objects and the
/// animation channel target.
fn extensions_everywhere_doc() -> GltfBuilder {
    use mesh_tools::{material::MaterialBuilder, AccessorSpec, AccessorType, ExtensionTarget as T, InterpolationType, SparseSpec, SpecularGlossiness, TextureRef, TextureSlot as S};
    use serde_json::json;
    let mut b = GltfBuilder::new();
    let tex = b.create_checkerboard_texture(4, 4, 2, [0; 3], [255; 3]).unwrap();
    let sg = SpecularGlossiness {
        diffuse_texture: Some(TextureRef::new(tex)),
        specular_glossiness_texture: Some(TextureRef::new(tex)),
        ..Default::default()
    };
    let m = MaterialBuilder::new(None)
        .with_base_color_texture(tex)
        .with_metallic_roughness_texture(tex)
        .with_normal_texture(tex, 1.0)
        .with_occlusion_texture(tex, 1.0)
        .with_emissive_texture(tex)
        .with_specular_glossiness(sg)
        .try_build()
        .unwrap();
    let mat = b.add_material_from(m).unwrap();
    let mesh = b.create_box_with_material(1.0, Some(mat));
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));
    let joint = b.add_joint_node(None, None, None, None);
    b.add_skin(None, vec![joint], None, Some(joint)).unwrap();
    let cam = b.add_perspective_camera(None, 0.5, 0.1, Some(10.0), None).unwrap();
    b.set_node_camera(joint, cam).unwrap();
    let ortho = b.add_orthographic_camera(None, 2.0, 1.0, 0.1, 10.0).unwrap();
    let ortho_node = b.add_node(None, None, None, None, None);
    b.set_node_camera(ortho_node, ortho).unwrap();
    let anim = b.add_animation(None);
    b.add_translation_track(anim, node, &[0.0, 1.0], &[[0.0; 3], [1.0; 3]], InterpolationType::Linear).unwrap();
    let sp = SparseSpec::new(vec![1, 3], vec![5.0f32, 6.0]);
    let acc = b.add_sparse_accessor::<f32>(5, AccessorType::Scalar, None, sp, AccessorSpec::new()).unwrap();
    let mut targets = vec![
        T::Root, T::Asset, T::Scene(0), T::Node(node), T::Mesh(mesh), T::Primitive { mesh, primitive: 0 },
        T::Material(mat), T::PbrMetallicRoughness(mat), T::Texture(tex), T::Image(0), T::Sampler(0),
        T::Accessor(0), T::BufferView(0), T::Buffer(0), T::Skin(0), T::Camera(cam), T::CameraPerspective(cam),
        T::Camera(ortho), T::CameraOrthographic(ortho),
        T::Animation(0), T::AnimationChannel { animation: 0, channel: 0 },
        T::AnimationChannelTarget { animation: 0, channel: 0 }, T::AnimationSampler { animation: 0, sampler: 0 },
        T::Sparse(acc), T::SparseIndices(acc), T::SparseValues(acc),
    ];
    targets.extend(
        [S::BaseColor, S::MetallicRoughness, S::Normal, S::Occlusion, S::Emissive, S::Diffuse, S::SpecularGlossiness]
            .map(|slot| T::MaterialTexture { material: mat, slot }),
    );
    for t in targets {
        b.set_extension(t, "VENDOR_corpus", json!({ "a": 1 })).unwrap();
        b.set_extras(t, json!({ "k": 1 })).unwrap();
    }
    b
}

fn corpus() -> Vec<(&'static str, GltfBuilder)> {
    let mut simple = GltfBuilder::new();
    let m = simple.create_box(1.0);
    let simple = scene_of(simple, m);
    vec![
        ("simple_box", simple),
        ("primitives", primitives_doc()),
        ("custom_mesh", custom_mesh_doc()),
        ("materials", materials_doc()),
        ("textured", textured_doc()),
        ("image_data_uri", image_data_uri_doc()),
        ("specular_glossiness", specular_glossiness_doc()),
        ("pbr_slots", pbr_slots_doc()),
        ("hierarchy", hierarchy_doc()),
        ("instancing", instancing_doc()),
        ("scenes_matrix", scenes_matrix_doc()),
        ("animation", animation_doc()),
        ("animation_normalized", animation_normalized_doc()),
        ("animation_shared_sampler", animation_shared_sampler_doc()),
        ("skinned", skinned_doc()),
        ("skinned_u8_weights", skinned_u8_weights_doc()),
        ("skinned_mixed_sets", skinned_mixed_sets_doc()),
        ("morph", morph_doc()),
        ("extensions", extensions_doc()),
        ("sparse", sparse_doc()),
        ("primitive_modes", primitive_modes_doc()),
        ("cameras_perspective", cameras_perspective_doc()),
        ("cameras_orthographic", cameras_orthographic_doc()),
        ("cameras_extras", cameras_extras_doc()),
        ("extensions_everywhere", extensions_everywhere_doc()),
    ]
}

fn corpus_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("corpus");
    std::fs::create_dir_all(&dir).expect("create target/corpus");
    dir
}

/// Write every corpus document to `target/corpus/<name>.glb` and return the paths.
fn write_corpus() -> Vec<PathBuf> {
    corpus()
        .into_iter()
        .map(|(name, b)| {
            let path = corpus_dir().join(format!("{name}.glb"));
            b.export_glb(path.to_str().unwrap()).unwrap_or_else(|e| panic!("corpus doc {name}: {e}"));
            path
        })
        .collect()
}

#[test]
fn corpus_names_are_unique_and_non_empty() {
    let mut names: Vec<_> = corpus().into_iter().map(|(n, _)| n).collect();
    assert!(names.len() >= 12);
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len(), "duplicate corpus names");
}

#[test]
fn corpus_validates_in_crate_without_errors_or_warnings() {
    for (name, b) in corpus() {
        let r = b.validate();
        assert!(r.errors.is_empty() && r.warnings.is_empty(), "{name}: {r:?}");
    }
}

#[test]
fn corpus_reads_back_through_the_independent_gltf_crate() {
    // Representative oracle values derived by hand from the builders above.
    for (name, b) in corpus() {
        let g = common::parse_glb(&b);
        assert!(g.document.meshes().count() >= 1, "{name}: no meshes");
        let expected = if name == "scenes_matrix" { 2 } else { 1 };
        assert_eq!(g.document.scenes().count(), expected, "{name}: scene count");
    }
    let g = common::parse_glb(&scenes_matrix_doc());
    assert_eq!(g.document.default_scene().unwrap().name(), Some("B"));
    let (t, _, s) = g.document.nodes().next().unwrap().transform().decomposed();
    assert_eq!(t, [1.0, 2.0, 3.0]);
    assert_eq!(s.iter().map(|v| v.abs()).collect::<Vec<_>>(), vec![2.0; 3]);
    let g = common::parse_glb(&morph_doc());
    let mesh = g.document.meshes().next().unwrap();
    assert_eq!(mesh.weights(), Some(&[0.5][..]));
    assert_eq!(mesh.primitives().next().unwrap().morph_targets().count(), 1);
    let g = common::parse_glb(&animation_doc());
    assert_eq!(g.document.animations().next().unwrap().channels().count(), 3);
    let g = common::parse_glb(&animation_normalized_doc());
    let chan = g.document.animations().next().unwrap().channels().next().unwrap();
    match chan.reader(|_| g.blob.as_deref()).read_outputs().unwrap() {
        gltf::animation::util::ReadOutputs::Rotations(gltf::animation::util::Rotations::I8(it)) => {
            assert_eq!(it.collect::<Vec<_>>(), [[0, 0, 0, 127], [0, 90, 0, 90]]); // 0.7071 * 127 = 89.8
        }
        _ => panic!("expected I8 rotations"),
    }
    let g = common::parse_glb(&animation_shared_sampler_doc());
    let anim = g.document.animations().next().unwrap();
    assert_eq!((anim.samplers().count(), anim.channels().count()), (1, 3));
    let g = common::parse_glb(&skinned_doc());
    assert_eq!(g.document.skins().next().unwrap().joints().count(), 2);
    let g = common::parse_glb(&skinned_u8_weights_doc());
    let prim = g.document.meshes().next().unwrap().primitives().next().unwrap();
    let reader = prim.reader(|_| g.blob.as_deref());
    match reader.read_weights(0).unwrap() {
        gltf::mesh::util::ReadWeights::U8(it) => assert!(it.into_iter().all(|w| w == [200, 55, 0, 0])),
        _ => panic!("expected U8 weights"),
    }
    let g = common::parse_glb(&skinned_mixed_sets_doc());
    let skin = g.document.skins().next().unwrap();
    assert!(skin.inverse_bind_matrices().is_none());
    assert_eq!(g.document.meshes().next().unwrap().primitives().next().unwrap().get(&gltf::Semantic::Weights(1)).unwrap().data_type(), gltf::accessor::DataType::F32);
    let g = common::parse_glb(&primitives_doc());
    assert_eq!(g.document.meshes().count(), 7);
    let g = common::parse_glb(&cameras_perspective_doc());
    assert_eq!(g.document.cameras().count(), 2);
    let g = common::parse_glb(&cameras_orthographic_doc());
    assert_eq!(g.document.cameras().next().unwrap().name(), Some("Ortho"));
}

#[test]
fn corpus_is_written_to_target_corpus() {
    let paths = write_corpus();
    assert_eq!(paths.len(), corpus().len());
    for p in paths {
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[..4], b"glTF", "{}", p.display());
    }
}

// ---------------------------------------------------------------------------------------------
// Khronos report handling (pure functions, tested without the binary)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Issue {
    code: String,
    severity: u64, // 0 error, 1 warning, 2 info, 3 hint
    pointer: String,
    message: String,
}

/// Parse the JSON report printed by `gltf_validator -o`. Fails on a truncated report, because
/// issues past the validator's cap would be invisible.
fn parse_report(json: &str) -> Result<Vec<Issue>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("report is not JSON: {e}"))?;
    let issues = v.get("issues").ok_or("report has no `issues` object")?;
    if issues.get("truncated").and_then(Value::as_bool) == Some(true) {
        return Err("report is truncated (too many issues); fix the first ones and re-run".into());
    }
    let msgs = issues.get("messages").and_then(Value::as_array).ok_or("report has no `messages`")?;
    msgs.iter()
        .map(|m| {
            Ok(Issue {
                code: m.get("code").and_then(Value::as_str).ok_or("message without code")?.to_string(),
                severity: m.get("severity").and_then(Value::as_u64).ok_or("message without severity")?,
                pointer: m.get("pointer").and_then(Value::as_str).unwrap_or("").to_string(),
                message: m.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Parse `CODE | reason` lines. Comments (`#`) and blank lines are skipped; a missing reason is
/// an error so the allow-list cannot silently accumulate unexplained entries.
fn parse_allowlist(text: &str) -> Result<Vec<String>, String> {
    let mut codes = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (code, reason) = line.split_once('|').ok_or(format!("allowlist line {}: expected `CODE | reason`", i + 1))?;
        if code.trim().is_empty() || reason.trim().is_empty() {
            return Err(format!("allowlist line {}: code and reason are both required", i + 1));
        }
        codes.push(code.trim().to_string());
    }
    Ok(codes)
}

/// Issues that fail the build: every error, and every warning not allow-listed. Infos and hints
/// never fail.
fn blocking<'a>(issues: &'a [Issue], allow: &[String]) -> Vec<&'a Issue> {
    issues
        .iter()
        .filter(|i| i.severity == 0 || (i.severity == 1 && !allow.contains(&i.code)))
        .collect()
}

#[test]
fn report_parsing_and_filtering() {
    let json = r#"{"issues":{"numErrors":1,"numWarnings":2,"messages":[
        {"code":"A","message":"m","severity":0,"pointer":"/a"},
        {"code":"W1","message":"m","severity":1,"pointer":"/b"},
        {"code":"W2","message":"m","severity":1,"pointer":"/c"},
        {"code":"I","message":"m","severity":2,"pointer":"/d"},
        {"code":"H","message":"m","severity":3}]}}"#;
    let issues = parse_report(json).unwrap();
    assert_eq!(issues.len(), 5);
    let allow = vec!["W1".to_string(), "A".to_string()];
    let codes: Vec<_> = blocking(&issues, &allow).iter().map(|i| i.code.as_str()).collect();
    assert_eq!(codes, ["A", "W2"], "errors are never allow-listable; infos and hints never block");
}

#[test]
fn report_errors_are_reported_not_ignored() {
    assert!(parse_report("not json").is_err());
    assert!(parse_report("{}").is_err());
    assert!(parse_report(r#"{"issues":{"truncated":true,"messages":[]}}"#).unwrap_err().contains("truncated"));
    assert!(parse_report(r#"{"issues":{"messages":[{"severity":0}]}}"#).is_err());
}

#[test]
fn allowlist_requires_a_reason() {
    assert_eq!(parse_allowlist("# c\n\nFOO | because\n").unwrap(), ["FOO"]);
    assert!(parse_allowlist("FOO\n").is_err());
    assert!(parse_allowlist("FOO |  \n").is_err());
    assert!(parse_allowlist(" | why\n").is_err());
}

#[test]
fn validator_version_must_match_the_pin() {
    assert!(check_version("gltf_validator 2.0.0-dev.3.10\n", PINNED_VALIDATOR).is_ok());
    let err = check_version("gltf_validator 2.0.0-dev.3.9\n", PINNED_VALIDATOR).unwrap_err();
    assert!(err.contains("2.0.0-dev.3.10") && err.contains("3.9"), "{err}");
    assert!(check_version("", PINNED_VALIDATOR).is_err());
}

#[test]
fn validator_pin_matches_ci_and_local_script() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (file, needle) in [(".github/workflows/ci.yml", "GLTF_VALIDATOR_VERSION: "), ("tests/khronos/run.sh", "VERSION=")] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        let line = text.lines().find(|l| l.trim_start().starts_with(needle)).unwrap_or_else(|| panic!("{file}: no `{needle}` line"));
        assert!(line.trim_start()[needle.len()..].trim() == PINNED_VALIDATOR, "{file} pins {line:?}, expected {PINNED_VALIDATOR}");
    }
}

#[test]
fn checked_in_allowlist_parses() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/khronos/allowlist.txt")).unwrap();
    parse_allowlist(&text).unwrap();
}

// ---------------------------------------------------------------------------------------------
// Khronos validator runner
// ---------------------------------------------------------------------------------------------

/// The validator release the corpus is triaged against (also pinned in `.github/workflows/ci.yml`).
const PINNED_VALIDATOR: &str = "2.0.0-dev.3.10";

/// Fail unless the binary's `--version` output names the pinned release, so a different
/// validator cannot change the verdict unnoticed.
fn check_version(version_output: &str, pinned: &str) -> Result<(), String> {
    if version_output.contains(pinned) {
        Ok(())
    } else {
        Err(format!("expected gltf_validator {pinned}, `--version` printed {:?}", version_output.trim()))
    }
}

/// The validator binary from `GLTF_VALIDATOR`, checked once per call for the pinned version.
/// Panics (fails the test) when unset, unusable or of another version: never a silent skip.
fn validator_bin() -> String {
    let bin = std::env::var("GLTF_VALIDATOR").unwrap_or_else(|_| {
        panic!(
            "GLTF_VALIDATOR is not set. Install the pinned gltf_validator (see docs/VALIDATION.md) \
             and set GLTF_VALIDATOR=/path/to/gltf_validator, or do not pass --ignored."
        )
    });
    let out = Command::new(&bin).arg("--version").output().unwrap_or_else(|e| panic!("cannot run GLTF_VALIDATOR={bin}: {e}"));
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    check_version(&text, PINNED_VALIDATOR).unwrap_or_else(|e| panic!("GLTF_VALIDATOR={bin}: {e}"));
    bin
}

/// Run the validator on `path` and return its issues. Panics (fails the test) if the binary is
/// missing or its output is not a report: never a silent skip.
fn run_khronos(path: &Path) -> Vec<Issue> {
    let bin = validator_bin();
    // -o: JSON report to stdout, -r: also validate embedded resources.
    let out = Command::new(&bin)
        .args(["-o", "-r"])
        .arg(path)
        .output()
        .unwrap_or_else(|e| panic!("cannot run GLTF_VALIDATOR={bin}: {e}"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    parse_report(&stdout).unwrap_or_else(|e| {
        panic!("{}: {e}\nstdout: {stdout}\nstderr: {}", path.display(), String::from_utf8_lossy(&out.stderr))
    })
}

fn allowlist() -> Vec<String> {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/khronos/allowlist.txt")).unwrap();
    parse_allowlist(&text).unwrap()
}

#[test]
#[ignore = "needs the Khronos validator: GLTF_VALIDATOR=/path/to/gltf_validator cargo test --test corpus -- --ignored (CI does this; see docs/VALIDATION.md)"]
fn khronos_accepts_the_corpus() {
    let allow = allowlist();
    let mut failures = Vec::new();
    for path in write_corpus() {
        let issues = run_khronos(&path);
        for i in blocking(&issues, &allow) {
            failures.push(format!("{}: {} (severity {}) at {}: {}", path.display(), i.code, i.severity, i.pointer, i.message));
        }
    }
    assert!(failures.is_empty(), "Khronos validator findings:\n{}", failures.join("\n"));
}

// ---------------------------------------------------------------------------------------------
// Negative cross-checks
// ---------------------------------------------------------------------------------------------

struct Negative {
    name: &'static str,
    doc: GltfBuilder,
    /// In-crate validator code.
    crate_code: &'static str,
    /// Khronos code that must appear among the errors.
    khronos_code: &'static str,
}

fn boxed() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let m = b.create_box(1.0);
    scene_of(b, m)
}

fn negatives() -> Vec<Negative> {
    let mut v = Vec::new();

    let mut b = boxed();
    b.gltf.nodes.as_mut().unwrap()[0].mesh = Some(99);
    v.push(Negative { name: "node mesh dangling", doc: b, crate_code: "NODE_MESH_INVALID", khronos_code: "UNRESOLVED_REFERENCE" });

    let mut b = GltfBuilder::new();
    let a = b.add_node(None, None, None, None, None);
    let c = b.add_node(None, None, None, None, None);
    b.gltf.nodes.as_mut().unwrap()[a].children = Some(vec![c]);
    b.gltf.nodes.as_mut().unwrap()[c].children = Some(vec![a]);
    v.push(Negative { name: "node cycle", doc: b, crate_code: "NODE_CYCLE", khronos_code: "NODE_LOOP" });

    let mut b = GltfBuilder::new();
    let p1 = b.add_node(None, None, None, None, None);
    let p2 = b.add_node(None, None, None, None, None);
    let c = b.add_node(None, None, None, None, None);
    b.gltf.nodes.as_mut().unwrap()[p1].children = Some(vec![c]);
    b.gltf.nodes.as_mut().unwrap()[p2].children = Some(vec![c]);
    v.push(Negative { name: "two parents", doc: b, crate_code: "NODE_PARENT_OVERRIDE", khronos_code: "NODE_PARENT_OVERRIDE" });

    let mut b = GltfBuilder::new();
    let n = b.add_node(None, None, Some([1.0, 0.0, 0.0]), None, None);
    b.gltf.nodes.as_mut().unwrap()[n].matrix =
        Some([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, 0.0, 0.0, 1.0]);
    v.push(Negative { name: "matrix and TRS", doc: b, crate_code: "NODE_MATRIX_AND_TRS", khronos_code: "NODE_MATRIX_TRS" });

    let mut b = boxed();
    b.gltf.buffer_views.as_mut().unwrap()[0].byte_length += 1000;
    v.push(Negative { name: "buffer view past buffer", doc: b, crate_code: "BUFFER_VIEW_RANGE", khronos_code: "BUFFER_VIEW_TOO_LONG" });

    let mut b = boxed();
    b.gltf.accessors.as_mut().unwrap()[0].count *= 100;
    v.push(Negative { name: "accessor past view", doc: b, crate_code: "ACCESSOR_RANGE", khronos_code: "ACCESSOR_TOO_LONG" });

    let mut b = boxed();
    b.gltf.scenes.as_mut().unwrap()[0].nodes = Some(vec![99]);
    v.push(Negative { name: "scene node dangling", doc: b, crate_code: "SCENE_NODE_INVALID", khronos_code: "UNRESOLVED_REFERENCE" });

    let mut b = boxed();
    b.gltf.meshes.as_mut().unwrap()[0].primitives[0].material = Some(99);
    v.push(Negative { name: "material dangling", doc: b, crate_code: "PRIMITIVE_MATERIAL_INVALID", khronos_code: "UNRESOLVED_REFERENCE" });

    // Khronos codes below are from its message catalogue; not run locally (no binary), CI checks them.
    let mut b = pbr_slots_doc();
    b.gltf.materials.as_mut().unwrap()[0].emissive_texture.as_mut().unwrap().index = 99;
    v.push(Negative { name: "material texture dangling", doc: b, crate_code: "MATERIAL_TEXTURE_INVALID", khronos_code: "UNRESOLVED_REFERENCE" });

    let mut b = pbr_slots_doc();
    b.gltf.meshes.as_mut().unwrap()[0].primitives[0].attributes.remove("TEXCOORD_1");
    v.push(Negative { name: "material texcoord missing", doc: b, crate_code: "MATERIAL_TEXCOORD_MISSING", khronos_code: "MESH_PRIMITIVE_TOO_FEW_TEXCOORDS" });

    let mut b = cameras_perspective_doc();
    b.gltf.nodes.as_mut().unwrap()[1].camera = Some(99);
    v.push(Negative { name: "node camera dangling", doc: b, crate_code: "NODE_CAMERA_INVALID", khronos_code: "UNRESOLVED_REFERENCE" });

    let mut b = cameras_perspective_doc();
    b.gltf.cameras.as_mut().unwrap()[0].perspective.as_mut().unwrap().zfar = Some(0.05);
    v.push(Negative { name: "camera zfar below znear", doc: b, crate_code: "CAMERA_ZFAR_INVALID", khronos_code: "CAMERA_ZFAR_LEQUAL_ZNEAR" });

    v
}

#[test]
fn negative_documents_are_valid_before_mutation_and_rejected_in_crate() {
    assert!(boxed().validate().is_valid());
    for n in negatives() {
        let r = n.doc.validate();
        assert!(r.has_error(n.crate_code), "{}: expected {} in {:?}", n.name, n.crate_code, r.errors);
        assert!(n.doc.to_glb_bytes().is_err(), "{}: export must refuse the document", n.name);
    }
}

#[test]
#[ignore = "needs the Khronos validator: GLTF_VALIDATOR=/path/to/gltf_validator cargo test --test corpus -- --ignored (CI does this; see docs/VALIDATION.md)"]
fn khronos_rejects_the_negative_documents() {
    for n in negatives() {
        let path = corpus_dir().join(format!("neg_{}.glb", n.name.replace(' ', "_")));
        std::fs::write(&path, n.doc.to_glb_bytes_unchecked().unwrap()).unwrap();
        let issues = run_khronos(&path);
        assert!(
            issues.iter().any(|i| i.severity == 0 && i.code == n.khronos_code),
            "{}: Khronos did not report error {} (got {:?})",
            n.name,
            n.khronos_code,
            issues.iter().map(|i| (&i.code, i.severity)).collect::<Vec<_>>()
        );
    }
}
