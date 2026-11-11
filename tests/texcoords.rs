//! Normalized u8/u16 TEXCOORD sets sampled by a material's texCoord, end to end.

mod common;

use mesh_tools::material::MaterialBuilder;
use mesh_tools::{GltfBuilder, PrimitiveSpec, TexCoords, TextureRef};

#[test]
fn material_samples_a_normalized_u16_set_and_a_u8_set() {
    let mut b = GltfBuilder::new();
    let t0 = b.create_uv_test_texture(16, 16).unwrap();
    let t1 = b.create_uv_test_texture(16, 16).unwrap();
    let m = MaterialBuilder::new(None)
        .with_base_color_texture(TextureRef::new(t0).with_tex_coord(1))
        .with_emissive_texture(TextureRef::new(t1).with_tex_coord(2))
        .try_build()
        .unwrap();
    let mi = b.add_material_from(m).unwrap();
    let p = b
        .create_primitive(PrimitiveSpec {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            texcoords: vec![
                TexCoords::F32(vec![0.0; 6]),
                TexCoords::U16(vec![0, 0, 32768, 65535, 65535, 0]),
                TexCoords::U8(vec![0, 0, 128, 255, 255, 0]),
            ],
            material: Some(mi),
            ..Default::default()
        })
        .unwrap();
    b.add_mesh_checked(None, vec![p], None).unwrap();
    let r = b.validate();
    assert!(r.is_valid(), "{r:#?}");

    let g = common::parse_glb(&b);
    let prim = g.meshes().next().unwrap().primitives().next().unwrap();
    let rd = prim.reader(|_| g.blob.as_deref());
    let u16s: Vec<[f32; 2]> = rd.read_tex_coords(1).unwrap().into_f32().collect();
    assert_eq!(u16s[0], [0.0, 0.0]);
    assert!((u16s[1][0] - 32768.0 / 65535.0).abs() < 1e-6 && u16s[1][1] == 1.0, "{u16s:?}");
    let u8s: Vec<[f32; 2]> = rd.read_tex_coords(2).unwrap().into_f32().collect();
    assert!((u8s[1][0] - 128.0 / 255.0).abs() < 1e-6 && u8s[1][1] == 1.0 && u8s[2] == [1.0, 0.0], "{u8s:?}");
    // The material points at those sets
    let m = g.materials().next().unwrap();
    assert_eq!(m.pbr_metallic_roughness().base_color_texture().unwrap().tex_coord(), 1);
    assert_eq!(m.emissive_texture().unwrap().tex_coord(), 2);
}
