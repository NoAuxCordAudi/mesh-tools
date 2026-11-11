//! Material slots, texCoord, scale/strength and alpha modes, read back through the independent
//! `gltf` crate against hand-chosen values.

mod common;

use gltf::material::AlphaMode as GAlpha;
use mesh_tools::material::MaterialBuilder;
use mesh_tools::{AlphaMode, GltfBuilder, GltfError, PrimitiveSpec, TexCoords, TextureRef};

fn five_textures(b: &mut GltfBuilder) -> [usize; 5] {
    let mut t = [0; 5];
    for x in &mut t {
        *x = b.create_uv_test_texture(16, 16).unwrap();
    }
    t
}

/// Triangle with TEXCOORD_0..=2 so every texCoord used below exists.
fn mesh_with_uvs(b: &mut GltfBuilder, material: usize) {
    let uv = || TexCoords::F32(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
    let p = b
        .create_primitive(PrimitiveSpec {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            texcoords: vec![uv(), uv(), uv()],
            material: Some(material),
            ..Default::default()
        })
        .unwrap();
    b.add_mesh_checked(None, vec![p], None).unwrap();
}

#[test]
fn every_slot_round_trips_with_distinct_values() {
    let mut b = GltfBuilder::new();
    let t = five_textures(&mut b);
    let m = MaterialBuilder::new(Some("All".into()))
        .with_base_color([0.1, 0.2, 0.3, 0.4])
        .with_metallic_factor(0.6)
        .with_roughness_factor(0.7)
        .with_emissive_factor([0.5, 0.25, 0.125])
        .with_base_color_texture(t[0])
        .with_metallic_roughness_texture(TextureRef::new(t[1]).with_tex_coord(1))
        .with_normal_texture(TextureRef { texture: t[2], tex_coord: 2 }, 0.75)
        .with_occlusion_texture(TextureRef::new(t[3]).with_tex_coord(1), 0.4)
        .with_emissive_texture(TextureRef::new(t[4]).with_tex_coord(2))
        .try_build()
        .unwrap();
    let mi = b.add_material_from(m).unwrap();
    mesh_with_uvs(&mut b, mi);
    let r = b.validate();
    assert!(r.is_valid(), "{r:#?}");
    // Only the two texCoord 2 slots warn (portability)
    assert_eq!(r.warnings.iter().filter(|w| w.starts_with("MATERIAL_TEXCOORD_RANGE")).count(), 2, "{r:#?}");

    let g = common::parse_glb(&b);
    let m = g.materials().next().unwrap();
    let pbr = m.pbr_metallic_roughness();
    assert_eq!(pbr.base_color_factor(), [0.1, 0.2, 0.3, 0.4]);
    assert_eq!((pbr.metallic_factor(), pbr.roughness_factor()), (0.6, 0.7));
    assert_eq!(m.emissive_factor(), [0.5, 0.25, 0.125]);
    let bc = pbr.base_color_texture().unwrap();
    assert_eq!((bc.texture().index(), bc.tex_coord()), (t[0], 0));
    let mr = pbr.metallic_roughness_texture().unwrap();
    assert_eq!((mr.texture().index(), mr.tex_coord()), (t[1], 1));
    let n = m.normal_texture().unwrap();
    assert_eq!((n.texture().index(), n.tex_coord(), n.scale()), (t[2], 2, 0.75));
    let o = m.occlusion_texture().unwrap();
    assert_eq!((o.texture().index(), o.tex_coord(), o.strength()), (t[3], 1, 0.4));
    let e = m.emissive_texture().unwrap();
    assert_eq!((e.texture().index(), e.tex_coord()), (t[4], 2));
}

#[test]
fn default_tex_coord_scale_and_strength_are_omitted_from_the_json() {
    let mut b = GltfBuilder::new();
    let t = five_textures(&mut b);
    let m = MaterialBuilder::new(None)
        .with_base_color_texture(t[0])
        .with_normal_texture(t[1], 1.0)
        .with_occlusion_texture(t[2], 1.0)
        .try_build()
        .unwrap();
    b.add_material_from(m).unwrap();
    let json = serde_json::to_string(&b.gltf.materials).unwrap();
    assert!(!json.contains("texCoord") && !json.contains("scale") && !json.contains("strength"), "{json}");
}

#[test]
fn negative_and_zero_normal_scale_are_legal() {
    let mut b = GltfBuilder::new();
    let t = five_textures(&mut b);
    for s in [-1.0f32, 0.0] {
        b.add_material_from(MaterialBuilder::new(None).with_normal_texture(t[0], s).try_build().unwrap()).unwrap();
    }
    let g = common::parse_glb(&b); // unused-material warnings only; export succeeded
    let scales: Vec<f32> = g.materials().map(|m| m.normal_texture().unwrap().scale()).collect();
    assert_eq!(scales, vec![-1.0, 0.0]);
}

#[test]
fn alpha_modes_round_trip() {
    let mut b = GltfBuilder::new();
    for mode in [AlphaMode::Opaque, AlphaMode::Mask { cutoff: 0.3 }, AlphaMode::Blend] {
        b.add_material_from(MaterialBuilder::new(None).with_alpha(mode).try_build().unwrap()).unwrap();
    }
    let g = common::parse_glb(&b);
    let ms: Vec<_> = g.materials().collect();
    assert_eq!(ms[0].alpha_mode(), GAlpha::Opaque);
    assert_eq!((ms[1].alpha_mode(), ms[1].alpha_cutoff()), (GAlpha::Mask, Some(0.3)));
    assert_eq!((ms[2].alpha_mode(), ms[2].alpha_cutoff()), (GAlpha::Blend, None));
    // A later non-mask mode clears an earlier cutoff
    let m = MaterialBuilder::new(None).with_alpha(AlphaMode::Mask { cutoff: 0.9 }).with_alpha(AlphaMode::Blend).build();
    assert_eq!(m.alpha_cutoff, None);
}

#[test]
#[allow(deprecated)]
fn deprecated_string_alpha_mode_still_works_but_is_checked_on_build() {
    let m = MaterialBuilder::new(None).with_alpha_mode("MASK".into(), Some(0.4)).try_build().unwrap();
    assert_eq!((m.alpha_mode.as_deref(), m.alpha_cutoff), (Some("MASK"), Some(0.4)));
    let e = MaterialBuilder::new(None).with_alpha_mode("GLASS".into(), None).try_build().unwrap_err();
    assert!(matches!(e, GltfError::InvalidMaterial { code: "MATERIAL_ALPHA_MODE_INVALID", .. }), "{e}");
}

#[test]
fn try_build_rejects_context_free_errors_with_codes() {
    let cases: Vec<(MaterialBuilder, &str)> = vec![
        (MaterialBuilder::new(None).with_base_color([1.5, 0.0, 0.0, 1.0]), "MATERIAL_FACTOR_RANGE"),
        (MaterialBuilder::new(None).with_roughness_factor(-0.1), "MATERIAL_FACTOR_RANGE"),
        (MaterialBuilder::new(None).with_emissive_factor([0.0, 2.0, 0.0]), "MATERIAL_FACTOR_RANGE"),
        (MaterialBuilder::new(None).with_metallic_factor(f32::NAN), "MATERIAL_FACTOR_NOT_FINITE"),
        (MaterialBuilder::new(None).with_normal_texture(0, f32::INFINITY), "MATERIAL_FACTOR_NOT_FINITE"),
        (MaterialBuilder::new(None).with_occlusion_texture(0, 1.2), "MATERIAL_OCCLUSION_STRENGTH_RANGE"),
        (MaterialBuilder::new(None).with_alpha(AlphaMode::Mask { cutoff: -0.5 }), "MATERIAL_ALPHA_CUTOFF_INVALID"),
    ];
    for (builder, code) in cases {
        let e = builder.try_build().unwrap_err();
        match e {
            GltfError::InvalidMaterial { code: c, index: None, .. } => assert_eq!(c, code),
            other => panic!("{other}"),
        }
    }
    // Boundary values are fine
    MaterialBuilder::new(None)
        .with_base_color([0.0, 1.0, 0.0, 1.0])
        .with_occlusion_texture(0, 0.0)
        .with_alpha(AlphaMode::Mask { cutoff: 0.0 })
        .try_build()
        .unwrap();
}

#[test]
fn add_material_from_is_atomic_and_reports_the_slot() {
    let mut b = GltfBuilder::new();
    let t = five_textures(&mut b);
    let ok = b.add_material_from(MaterialBuilder::new(None).with_base_color_texture(t[0]).build()).unwrap();
    assert_eq!(ok, 0);
    let bad = MaterialBuilder::new(None).with_base_color_texture(t[0]).with_emissive_texture(99).build();
    let e = b.add_material_from(bad).unwrap_err();
    match &e {
        GltfError::InvalidMaterial { index: Some(1), code: "MATERIAL_TEXTURE_INVALID", message } => {
            assert!(message.contains("emissiveTexture") && message.contains("99"), "{message}");
        }
        other => panic!("{other}"),
    }
    assert_eq!(b.gltf.materials.as_ref().unwrap().len(), 1);
    // A hand-built bad factor is caught too
    let mut m = MaterialBuilder::new(None).build();
    m.emissive_factor = Some([2.0, 0.0, 0.0]);
    assert!(b.add_material_from(m).is_err());
    assert_eq!(b.gltf.materials.as_ref().unwrap().len(), 1);
}

#[test]
fn existing_textured_material_output_is_unchanged() {
    let mut b = GltfBuilder::new();
    let t = five_textures(&mut b);
    let m = b.add_textured_material(
        None, Some(t[0]), Some(t[1]), Some(t[2]), Some(t[3]), Some(t[4]), None, None, None, None, None, None,
    );
    let json = serde_json::to_string(&b.gltf.materials.as_ref().unwrap()[m]).unwrap();
    let expected = format!(
        r#"{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":{}}},"metallicRoughnessTexture":{{"index":{}}}}},"normalTexture":{{"index":{}}},"occlusionTexture":{{"index":{}}},"emissiveTexture":{{"index":{}}}}}"#,
        t[0], t[1], t[2], t[3], t[4]
    );
    assert_eq!(json, expected);
}
