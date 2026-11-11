//! KHR_materials_pbrSpecularGlossiness (deprecated upstream): write path, per-slot texCoord,
//! metallic-roughness fallback, registration and validation, read back through the `gltf` crate.

mod common;

use mesh_tools::material::MaterialBuilder;
use mesh_tools::{GltfBuilder, GltfError, PrimitiveSpec, SpecularGlossiness, TexCoords, TextureRef};
use serde_json::{json, Value};

const SG: &str = "KHR_materials_pbrSpecularGlossiness";

fn textures(b: &mut GltfBuilder, n: usize) -> Vec<usize> {
    (0..n).map(|_| b.create_uv_test_texture(16, 16).unwrap()).collect()
}

fn json_chunk(b: &GltfBuilder) -> Value {
    let glb = b.to_glb_bytes().unwrap();
    let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&glb[20..20 + len]).unwrap()
}

fn invalid_code(e: GltfError) -> &'static str {
    match e {
        GltfError::InvalidMaterial { code, .. } => code,
        other => panic!("expected InvalidMaterial, got {other:?}"),
    }
}

/// Distinct hand-chosen value for every SG and shared slot.
fn full_material(t: &[usize]) -> MaterialBuilder {
    MaterialBuilder::new(Some("SG".into()))
        .with_specular_glossiness(SpecularGlossiness {
            diffuse_factor: Some([0.1, 0.2, 0.3, 0.4]),
            diffuse_texture: Some(TextureRef::new(t[0]).with_tex_coord(1)),
            specular_factor: Some([0.5, 0.25, 0.125]),
            glossiness_factor: Some(0.75),
            specular_glossiness_texture: Some(TextureRef::new(t[1]).with_tex_coord(2)),
        })
        .with_normal_texture(TextureRef::new(t[2]).with_tex_coord(1), 0.75)
        .with_occlusion_texture(t[3], 0.4)
        .with_emissive_texture(TextureRef::new(t[4]).with_tex_coord(2))
}

#[test]
fn every_sg_slot_round_trips_with_per_slot_texcoord() {
    let mut b = GltfBuilder::new();
    let t = textures(&mut b, 5);
    let m = b.add_material_from(full_material(&t).build()).unwrap();
    assert_eq!(m, 0);

    let glb = common::parse_glb(&b);
    let mat = glb.document.materials().next().unwrap();
    let sg = mat.pbr_specular_glossiness().unwrap();
    assert_eq!(sg.diffuse_factor(), [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(sg.specular_factor(), [0.5, 0.25, 0.125]);
    assert_eq!(sg.glossiness_factor(), 0.75);
    let d = sg.diffuse_texture().unwrap();
    assert_eq!((d.texture().index(), d.tex_coord()), (t[0], 1));
    let s = sg.specular_glossiness_texture().unwrap();
    assert_eq!((s.texture().index(), s.tex_coord()), (t[1], 2));
    let n = mat.normal_texture().unwrap();
    assert_eq!((n.texture().index(), n.tex_coord(), n.scale()), (t[2], 1, 0.75));
    let o = mat.occlusion_texture().unwrap();
    assert_eq!((o.texture().index(), o.tex_coord(), o.strength()), (t[3], 0, 0.4));
    let e = mat.emissive_texture().unwrap();
    assert_eq!((e.texture().index(), e.tex_coord()), (t[4], 2));
}

#[test]
fn default_texcoord_is_omitted_and_defaults_stay_off_the_wire() {
    let mut b = GltfBuilder::new();
    let t = textures(&mut b, 2);
    b.add_material_from(
        MaterialBuilder::new(None)
            .with_specular_glossiness(SpecularGlossiness {
                diffuse_texture: Some(t[0].into()),
                specular_glossiness_texture: Some(TextureRef::new(t[1]).with_tex_coord(1)),
                ..Default::default()
            })
            .build(),
    )
    .unwrap();
    let raw = json_chunk(&b);
    assert_eq!(
        raw["materials"][0]["extensions"][SG],
        json!({ "diffuseTexture": { "index": 0 }, "specularGlossinessTexture": { "index": 1, "texCoord": 1 } })
    );
}

#[test]
fn extensions_used_once_never_required() {
    let mut b = GltfBuilder::new();
    let t = textures(&mut b, 5);
    b.add_material_from(full_material(&t).build()).unwrap();
    b.add_material_from(full_material(&t).build()).unwrap();
    assert_eq!(b.gltf.extensions_used.as_deref().unwrap(), [SG]);
    assert!(b.gltf.extensions_required.is_none());
    let raw = json_chunk(&b);
    assert_eq!(raw["extensionsUsed"], json!([SG]));
    assert!(raw.get("extensionsRequired").is_none());
}

#[test]
fn metallic_roughness_fallback_is_written_next_to_sg() {
    let mut b = GltfBuilder::new();
    b.add_material_from(
        MaterialBuilder::new(None)
            .with_base_color([0.9, 0.8, 0.7, 1.0])
            .with_metallic_factor(0.0)
            .with_roughness_factor(0.25)
            .with_specular_glossiness(SpecularGlossiness { glossiness_factor: Some(0.5), ..Default::default() })
            .build(),
    )
    .unwrap();
    let glb = common::parse_glb(&b);
    let mat = glb.document.materials().next().unwrap();
    let pbr = mat.pbr_metallic_roughness();
    assert_eq!(pbr.base_color_factor(), [0.9, 0.8, 0.7, 1.0]);
    assert_eq!(pbr.roughness_factor(), 0.25);
    assert_eq!(mat.pbr_specular_glossiness().unwrap().glossiness_factor(), 0.5);
    // fallback present: no SG_WITHOUT_FALLBACK warning
    assert!(!b.validate().has_warning("SG_WITHOUT_FALLBACK"));
}

#[test]
fn sg_without_fallback_warns_only() {
    let mut b = GltfBuilder::new();
    b.add_material_from(
        MaterialBuilder::new(None)
            .with_specular_glossiness(SpecularGlossiness { glossiness_factor: Some(0.5), ..Default::default() })
            .build(),
    )
    .unwrap();
    let r = b.validate();
    assert!(r.has_warning("SG_WITHOUT_FALLBACK"), "{r:#?}");
    assert!(r.is_valid(), "{r:#?}");
}

#[test]
fn sg_factor_range_is_rejected_for_each_factor() {
    for sg in [
        SpecularGlossiness { diffuse_factor: Some([0.0, 0.0, 0.0, 1.5]), ..Default::default() },
        SpecularGlossiness { specular_factor: Some([-0.1, 0.0, 0.0]), ..Default::default() },
        SpecularGlossiness { glossiness_factor: Some(2.0), ..Default::default() },
    ] {
        let err = MaterialBuilder::new(None).with_specular_glossiness(sg).try_build().unwrap_err();
        assert_eq!(invalid_code(err), "SG_FACTOR_RANGE");
    }
    let err = MaterialBuilder::new(None)
        .with_specular_glossiness(SpecularGlossiness { glossiness_factor: Some(f32::NAN), ..Default::default() })
        .try_build()
        .unwrap_err();
    assert_eq!(invalid_code(err), "MATERIAL_FACTOR_NOT_FINITE");
}

#[test]
fn dangling_sg_texture_is_rejected_atomically() {
    let mut b = GltfBuilder::new();
    textures(&mut b, 1);
    let m = MaterialBuilder::new(None)
        .with_specular_glossiness(SpecularGlossiness { diffuse_texture: Some(5.into()), ..Default::default() })
        .build();
    assert_eq!(invalid_code(b.add_material_from(m).unwrap_err()), "SG_TEXTURE_INVALID");
    assert_eq!(b.material_count(), 0);
    assert!(b.gltf.extensions_used.is_none(), "a failed add must not register the extension");
}

#[test]
fn sg_in_extensions_required_is_a_warning() {
    let mut b = GltfBuilder::new();
    b.add_material_from(MaterialBuilder::new(None).with_base_color([1.0; 4]).with_specular_glossiness(Default::default()).build())
        .unwrap();
    b.require_extension(SG).unwrap();
    let r = b.validate();
    assert!(r.has_warning("SG_EXTENSION_REQUIRED"), "{r:#?}");
    assert!(r.is_valid(), "{r:#?}");
}

#[test]
fn missing_texcoord_set_on_sg_slot_is_reported() {
    let mut b = GltfBuilder::new();
    let t = textures(&mut b, 1);
    let m = b
        .add_material_from(
            MaterialBuilder::new(None)
                .with_specular_glossiness(SpecularGlossiness {
                    diffuse_texture: Some(TextureRef::new(t[0]).with_tex_coord(1)),
                    ..Default::default()
                })
                .build(),
        )
        .unwrap();
    let p = b
        .create_primitive(PrimitiveSpec {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            texcoords: vec![TexCoords::F32(vec![0.0; 6])],
            material: Some(m),
            ..Default::default()
        })
        .unwrap();
    b.add_mesh_checked(None, vec![p], None).unwrap();
    let r = b.validate();
    let e: Vec<_> = r.errors.iter().filter(|e| e.starts_with("MATERIAL_TEXCOORD_MISSING")).collect();
    assert_eq!(e.len(), 1, "{r:#?}");
    assert!(e[0].contains("diffuseTexture") && e[0].contains("TEXCOORD_1"), "{}", e[0]);
}

#[test]
fn hand_edited_sg_values_are_caught_by_validate() {
    let mut b = GltfBuilder::new();
    let m = b.add_material_from(MaterialBuilder::new(None).with_specular_glossiness(Default::default()).build()).unwrap();
    let sg = b.gltf.materials.as_mut().unwrap()[m].extensions.as_mut().unwrap().pbr_specular_glossiness.as_mut().unwrap();
    sg.glossiness_factor = Some(3.0);
    sg.diffuse_texture.get_or_insert_with(Default::default).index = 9;
    let r = b.validate();
    assert!(r.has_error("SG_FACTOR_RANGE"), "{r:#?}");
    assert!(r.has_error("SG_TEXTURE_INVALID"), "{r:#?}");
}

#[test]
#[allow(deprecated)]
fn deprecated_shims_return_result_and_keep_output() {
    let mut b = GltfBuilder::new();
    let t = textures(&mut b, 3);
    let m = b
        .add_specular_glossiness_material(
            Some("Old".into()), Some([0.1, 0.2, 0.3, 1.0]), Some(t[0]), Some([0.5, 0.5, 0.5]), Some(0.5),
            Some(t[1]), Some(t[2]), None, None, None, None, None, None,
        )
        .unwrap();
    assert_eq!(m, 0);
    let glb = common::parse_glb(&b);
    let sg = glb.document.materials().next().unwrap().pbr_specular_glossiness().unwrap();
    assert_eq!(sg.diffuse_texture().unwrap().texture().index(), t[0]);
    assert_eq!(b.gltf.extensions_used.as_deref().unwrap(), [SG]);

    // bad texture, bad factor and bad alpha mode are errors, nothing is stored
    let bad = b.add_specular_glossiness_material(None, None, Some(99), None, None, None, None, None, None, None, None, None, None);
    assert_eq!(invalid_code(bad.unwrap_err()), "SG_TEXTURE_INVALID");
    let bad = b.add_specular_glossiness_material(None, None, None, None, Some(2.0), None, None, None, None, None, None, None, None);
    assert_eq!(invalid_code(bad.unwrap_err()), "SG_FACTOR_RANGE");
    let bad = b.add_specular_glossiness_material(None, None, None, None, None, None, None, None, None, None, Some("NOPE".into()), None, None);
    assert_eq!(invalid_code(bad.unwrap_err()), "MATERIAL_ALPHA_MODE_INVALID");
    assert_eq!(b.gltf.materials.as_ref().unwrap().len(), 1);

    let bad = b.add_textured_material(None, Some(99), None, None, None, None, None, None, None, None, None, None);
    assert_eq!(invalid_code(bad.unwrap_err()), "MATERIAL_TEXTURE_INVALID");
    assert_eq!(b.gltf.materials.as_ref().unwrap().len(), 1);
}

#[test]
fn create_specular_material_rejects_bad_factors() {
    let mut b = GltfBuilder::new();
    let err = b.create_specular_material(None, [1.0; 4], [0.0; 3], 1.5).unwrap_err();
    assert_eq!(invalid_code(err), "SG_FACTOR_RANGE");
    assert_eq!(b.material_count(), 0);
}

#[test]
fn sg_texcoord_2_warns_and_undeclared_extension_is_an_error() {
    let mut b = GltfBuilder::new();
    let t = textures(&mut b, 1);
    let m = MaterialBuilder::new(None)
        .with_base_color([1.0; 4])
        .with_specular_glossiness(SpecularGlossiness {
            diffuse_texture: Some(TextureRef::new(t[0]).with_tex_coord(2)),
            ..Default::default()
        })
        .build();
    // pushed directly: skips add_material_from, so the extension is not registered
    b.gltf.materials.get_or_insert_with(Vec::new).push(m);
    let r = b.validate();
    assert!(r.has_warning("MATERIAL_TEXCOORD_RANGE"), "{r:#?}");
    assert!(r.has_error("EXTENSION_NOT_DECLARED"), "{r:#?}");
}
