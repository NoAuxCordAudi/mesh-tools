//! `validate()` material codes, including hand-edited documents.

use mesh_tools::material::MaterialBuilder;
use mesh_tools::{GltfBuilder, PrimitiveSpec, TexCoords};

fn tri(uv_sets: usize, material: Option<usize>) -> PrimitiveSpec {
    PrimitiveSpec {
        positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        texcoords: (0..uv_sets).map(|_| TexCoords::F32(vec![0.0; 6])).collect(),
        material,
        ..Default::default()
    }
}

fn doc_with_texture() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    b.create_uv_test_texture(16, 16).unwrap();
    b
}

#[test]
fn missing_texcoord_set_names_material_slot_set_mesh_and_primitive() {
    let mut b = doc_with_texture();
    let m = b.add_material_from(MaterialBuilder::new(None).with_base_color_texture(mesh_tools::TextureRef::new(0).with_tex_coord(1)).build()).unwrap();
    let ok = b.create_primitive(tri(2, Some(m))).unwrap();
    let bad = b.create_primitive(tri(1, Some(m))).unwrap();
    let bad2 = b.create_primitive(tri(1, Some(m))).unwrap();
    b.add_mesh_checked(None, vec![ok], None).unwrap();
    b.add_mesh_checked(None, vec![bad], None).unwrap();
    let ok2 = b.create_primitive(tri(2, Some(m))).unwrap();
    b.add_mesh_checked(None, vec![ok2, bad2], None).unwrap();
    let r = b.validate();
    let found: Vec<_> = r.errors.iter().filter(|e| e.starts_with("MATERIAL_TEXCOORD_MISSING")).collect();
    assert_eq!(found.len(), 2, "{r:#?}");
    assert!(found[0].contains("material 0") && found[0].contains("baseColorTexture") && found[0].contains("TEXCOORD_1")
        && found[0].contains("mesh 1") && found[0].contains("primitive 0"), "{}", found[0]);
    assert!(found[1].contains("mesh 2") && found[1].contains("primitive 1"), "{}", found[1]);
    // Export is blocked
    assert!(b.to_glb_bytes().is_err());
}

#[test]
fn texcoord_zero_missing_is_also_reported() {
    let mut b = doc_with_texture();
    let m = b.add_material_from(MaterialBuilder::new(None).with_emissive_texture(0).build()).unwrap();
    let p = b.create_primitive(tri(0, Some(m))).unwrap();
    b.add_mesh_checked(None, vec![p], None).unwrap();
    assert!(b.validate().has_error("MATERIAL_TEXCOORD_MISSING"));
}

#[test]
fn untextured_material_needs_no_uvs_and_unused_is_a_warning() {
    let mut b = GltfBuilder::new();
    let used = b.add_material_from(MaterialBuilder::new(None).with_metallic_factor(0.5).build()).unwrap();
    b.add_material_from(MaterialBuilder::new(None).build()).unwrap();
    let p = b.create_primitive(tri(0, Some(used))).unwrap();
    b.add_mesh_checked(None, vec![p], None).unwrap();
    let r = b.validate();
    assert!(r.is_valid(), "{r:#?}");
    assert_eq!(r.warnings, vec!["MATERIAL_UNUSED: material 1 is not used by any primitive".to_string()]);
}

#[test]
fn hand_edited_materials_are_reported() {
    let mut b = doc_with_texture();
    let mut m = MaterialBuilder::new(None).with_normal_texture(0, 0.0).with_occlusion_texture(7, 1.0).build();
    m.alpha_mode = Some("GLASS".into());
    m.alpha_cutoff = Some(-1.0);
    m.emissive_factor = Some([f32::NAN, 0.0, 2.0]);
    b.gltf.materials = Some(vec![m]);
    let r = b.validate();
    for code in [
        "MATERIAL_ALPHA_MODE_INVALID",
        "MATERIAL_ALPHA_CUTOFF_INVALID",
        "MATERIAL_TEXTURE_INVALID",
        "MATERIAL_FACTOR_NOT_FINITE",
        "MATERIAL_FACTOR_RANGE",
    ] {
        assert!(r.has_error(code), "{code} missing: {r:#?}");
    }
    assert!(r.has_warning("MATERIAL_NORMAL_SCALE_ZERO"), "{r:#?}");
}

#[test]
fn alpha_cutoff_without_mask_warns_and_occlusion_strength_is_checked() {
    let mut b = GltfBuilder::new();
    let mut m = MaterialBuilder::new(None).build();
    m.alpha_cutoff = Some(0.5);
    let mut n = MaterialBuilder::new(None).with_occlusion_texture(0, 0.5).build();
    n.occlusion_texture.as_mut().unwrap().strength = Some(1.5);
    b.gltf.materials = Some(vec![m, n]);
    let r = b.validate();
    assert!(r.has_warning("MATERIAL_ALPHA_CUTOFF_UNUSED"), "{r:#?}");
    assert!(r.has_error("MATERIAL_OCCLUSION_STRENGTH_RANGE"), "{r:#?}");
}
