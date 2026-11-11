//! Samplers: every filter/wrap combination read back through the independent `gltf` crate
//! against hand-written spec values (glTF 2.0 sampler table: 9728/9729 mag; 9728, 9729, 9984-9987
//! min; 10497, 33071, 33648 wrap).

mod common;

use gltf::texture::{MagFilter as GMag, MinFilter as GMin, WrappingMode as GWrap};
use mesh_tools::{GltfBuilder, MagFilter, MinFilter, SamplerDesc, WrapMode};

const MAGS: [(MagFilter, u32, GMag); 2] = [
    (MagFilter::Nearest, 9728, GMag::Nearest),
    (MagFilter::Linear, 9729, GMag::Linear),
];
const MINS: [(MinFilter, u32, GMin); 6] = [
    (MinFilter::Nearest, 9728, GMin::Nearest),
    (MinFilter::Linear, 9729, GMin::Linear),
    (MinFilter::NearestMipmapNearest, 9984, GMin::NearestMipmapNearest),
    (MinFilter::LinearMipmapNearest, 9985, GMin::LinearMipmapNearest),
    (MinFilter::NearestMipmapLinear, 9986, GMin::NearestMipmapLinear),
    (MinFilter::LinearMipmapLinear, 9987, GMin::LinearMipmapLinear),
];
const WRAPS: [(WrapMode, u32, GWrap); 3] = [
    (WrapMode::Repeat, 10497, GWrap::Repeat),
    (WrapMode::ClampToEdge, 33071, GWrap::ClampToEdge),
    (WrapMode::MirroredRepeat, 33648, GWrap::MirroredRepeat),
];

#[test]
fn all_108_combinations_round_trip() {
    let mut b = GltfBuilder::new();
    let mut expected = Vec::new();
    for &(mag, mag_code, gmag) in &MAGS {
        for &(min, min_code, gmin) in &MINS {
            for &(ws, ws_code, gws) in &WRAPS {
                for &(wt, wt_code, gwt) in &WRAPS {
                    let desc = SamplerDesc {
                        mag_filter: Some(mag),
                        min_filter: Some(min),
                        wrap_s: Some(ws),
                        wrap_t: Some(wt),
                    };
                    let i = b.add_sampler(desc);
                    assert_eq!(i, expected.len());
                    expected.push((mag_code, gmag, min_code, gmin, ws_code, gws, wt_code, gwt));
                }
            }
        }
    }
    assert_eq!(expected.len(), 108);
    let g = common::parse_glb(&b);
    let samplers: Vec<_> = g.document.samplers().collect();
    assert_eq!(samplers.len(), 108);
    for (s, e) in samplers.iter().zip(&expected) {
        assert_eq!(s.mag_filter(), Some(e.1));
        assert_eq!(s.mag_filter().unwrap().as_gl_enum(), e.0);
        assert_eq!(s.min_filter(), Some(e.3));
        assert_eq!(s.min_filter().unwrap().as_gl_enum(), e.2);
        assert_eq!(s.wrap_s(), e.5);
        assert_eq!(s.wrap_s().as_gl_enum(), e.4);
        assert_eq!(s.wrap_t(), e.7);
        assert_eq!(s.wrap_t().as_gl_enum(), e.6);
    }
}

#[test]
fn omitted_fields_read_back_with_spec_defaults() {
    let mut b = GltfBuilder::new();
    b.add_sampler(SamplerDesc::default());
    // No keys in the JSON at all
    let json = serde_json::to_value(&b.gltf).unwrap();
    assert_eq!(json["samplers"][0], serde_json::json!({}));
    let g = common::parse_glb(&b);
    let s = g.document.samplers().next().unwrap();
    assert_eq!(s.mag_filter(), None);
    assert_eq!(s.min_filter(), None);
    // Spec default wrap is REPEAT (10497)
    assert_eq!(s.wrap_s(), GWrap::Repeat);
    assert_eq!(s.wrap_t(), GWrap::Repeat);
}

#[test]
fn codes_and_from_code_agree_with_spec_numbers() {
    for &(m, code, _) in &MAGS {
        assert_eq!(m.code() as u32, code);
        assert_eq!(MagFilter::from_code(code as usize), Some(m));
    }
    for &(m, code, _) in &MINS {
        assert_eq!(m.code() as u32, code);
        assert_eq!(MinFilter::from_code(code as usize), Some(m));
    }
    for &(m, code, _) in &WRAPS {
        assert_eq!(m.code() as u32, code);
        assert_eq!(WrapMode::from_code(code as usize), Some(m));
    }
    // The mag filter has no mipmap modes
    assert_eq!(MagFilter::from_code(9984), None);
    assert_eq!(MinFilter::from_code(1), None);
    assert_eq!(WrapMode::from_code(9728), None);
}

#[test]
fn get_or_add_dedups_by_full_equality() {
    let mut b = GltfBuilder::new();
    let a = SamplerDesc { mag_filter: Some(MagFilter::Nearest), ..Default::default() };
    let i0 = b.get_or_add_sampler(a.clone());
    assert_eq!(b.get_or_add_sampler(a.clone()), i0);
    let other = SamplerDesc { mag_filter: Some(MagFilter::Linear), ..Default::default() };
    let i1 = b.get_or_add_sampler(other);
    assert_ne!(i0, i1);
    // add_sampler never dedups
    let i2 = b.add_sampler(a.clone());
    assert_ne!(i2, i0);
    // get_or_add returns the first match
    assert_eq!(b.get_or_add_sampler(a), i0);
    assert_eq!(b.gltf.samplers.as_ref().unwrap().len(), 3);
    // Empty desc is distinct from a populated one
    let e = b.get_or_add_sampler(SamplerDesc::default());
    assert_eq!(b.get_or_add_sampler(SamplerDesc::default()), e);
}

#[test]
fn get_or_add_does_not_reuse_samplers_with_name_or_extras() {
    let mut b = GltfBuilder::new();
    let i0 = b.add_sampler(SamplerDesc::default());
    b.set_extras(mesh_tools::ExtensionTarget::Sampler(i0), serde_json::json!({"k": 1})).unwrap();
    let i1 = b.get_or_add_sampler(SamplerDesc::default());
    assert_ne!(i0, i1);
}

#[test]
fn shared_default_sampler_for_textures() {
    let mut b = GltfBuilder::new();
    let t0 = b.create_checkerboard_texture(4, 4, 2, [255, 0, 0], [0, 0, 255]).unwrap();
    let t1 = b.create_uv_test_texture(16, 16).unwrap();
    assert_eq!(b.gltf.samplers.as_ref().unwrap().len(), 1);
    let g = common::parse_glb(&b);
    let tex: Vec<_> = g.document.textures().collect();
    assert_eq!(tex.len(), 2);
    assert_eq!((t0, t1), (0, 1));
    let s = tex[0].sampler().index().unwrap();
    assert_eq!(tex[1].sampler().index().unwrap(), s);
    let samp = g.document.samplers().next().unwrap();
    assert_eq!(samp.mag_filter().unwrap().as_gl_enum(), 9729);
    assert_eq!(samp.min_filter().unwrap().as_gl_enum(), 9987);
    assert_eq!(samp.wrap_s().as_gl_enum(), 10497);
    assert_eq!(samp.wrap_t().as_gl_enum(), 10497);
}

#[test]
fn create_texture_with_explicit_sampler() {
    let mut b = GltfBuilder::new();
    let img = mesh_tools::texture::create_uv_test_pattern(8, 8).unwrap();
    let desc = SamplerDesc {
        mag_filter: Some(MagFilter::Nearest),
        min_filter: Some(MinFilter::Nearest),
        wrap_s: Some(WrapMode::ClampToEdge),
        wrap_t: Some(WrapMode::MirroredRepeat),
    };
    let t = b
        .create_texture_from_image_with_sampler(None, &img, mesh_tools::texture::TextureFormat::PNG, desc.clone())
        .unwrap();
    let t2 = b
        .create_texture_from_image_with_sampler(None, &img, mesh_tools::texture::TextureFormat::PNG, desc)
        .unwrap();
    assert_eq!(b.gltf.samplers.as_ref().unwrap().len(), 1);
    let g = common::parse_glb(&b);
    let tex = g.document.textures().nth(t).unwrap();
    let s = tex.sampler();
    assert_eq!(s.mag_filter(), Some(GMag::Nearest));
    assert_eq!(s.min_filter(), Some(GMin::Nearest));
    assert_eq!(s.wrap_s(), GWrap::ClampToEdge);
    assert_eq!(s.wrap_t(), GWrap::MirroredRepeat);
    assert_eq!(g.document.textures().nth(t2).unwrap().sampler().index(), s.index());
}

#[test]
fn add_texture_validates_indices_atomically() {
    let mut b = GltfBuilder::new();
    assert!(b.add_texture(None, 0, None).is_err());
    assert!(b.gltf.textures.as_ref().map_or(true, |v| v.is_empty()));
    let img = b.add_image_uri(None, "a.png", None).unwrap();
    assert!(b.add_texture(None, img, Some(0)).is_err());
    assert!(b.gltf.textures.as_ref().map_or(true, |v| v.is_empty()));
    let s = b.add_sampler(SamplerDesc::default());
    let t = b.add_texture(Some("t".into()), img, Some(s)).unwrap();
    assert_eq!(t, 0);
    let t = b.add_texture(None, img, None).unwrap();
    assert_eq!(t, 1);
    let g = common::parse_glb(&b);
    assert_eq!(g.document.textures().nth(1).unwrap().sampler().index(), None);
}

#[test]
fn validator_flags_hand_edited_sampler_values() {
    let mut b = GltfBuilder::new();
    let s = b.add_sampler(SamplerDesc::default());
    b.gltf.samplers.as_mut().unwrap()[s].mag_filter = Some(9984); // mipmap mode is not a mag filter
    b.gltf.samplers.as_mut().unwrap()[s].min_filter = Some(1);
    b.gltf.samplers.as_mut().unwrap()[s].wrap_s = Some(9728);
    b.gltf.samplers.as_mut().unwrap()[s].wrap_t = Some(0);
    let r = b.validate();
    assert!(r.has_error("SAMPLER_MAG_FILTER_INVALID"), "{r:?}");
    assert!(r.has_error("SAMPLER_MIN_FILTER_INVALID"), "{r:?}");
    assert_eq!(r.errors.iter().filter(|e| e.starts_with("SAMPLER_WRAP_INVALID")).count(), 2);
    assert!(b.to_glb_bytes().is_err());
}

#[test]
fn validator_flags_texture_references_and_unused_objects() {
    let mut b = GltfBuilder::new();
    let img = b.add_image_uri(None, "a.png", None).unwrap();
    b.add_sampler(SamplerDesc::default());
    let r = b.validate();
    assert!(r.has_warning("SAMPLER_UNUSED"), "{r:?}");
    assert!(r.has_warning("IMAGE_UNUSED"), "{r:?}");
    b.add_texture(None, img, None).unwrap();
    let r = b.validate();
    assert!(!r.has_warning("IMAGE_UNUSED"), "{r:?}");
    assert!(r.has_warning("SAMPLER_UNUSED"));
    b.gltf.textures.as_mut().unwrap()[0].source = 7;
    b.gltf.textures.as_mut().unwrap()[0].sampler = Some(9);
    let r = b.validate();
    assert!(r.has_error("TEXTURE_SOURCE_OUT_OF_RANGE"), "{r:?}");
    assert!(r.has_error("TEXTURE_SAMPLER_OUT_OF_RANGE"), "{r:?}");
}

#[test]
fn sampler_name_round_trips_and_blocks_dedup() {
    let mut b = GltfBuilder::new();
    let s = b.add_sampler(SamplerDesc::default());
    b.set_sampler_name(s, Some("pixelated".into())).unwrap();
    assert!(b.set_sampler_name(3, Some("x".into())).is_err());
    assert_ne!(b.get_or_add_sampler(SamplerDesc::default()), s);
    let img = b.add_image_uri(None, "a.png", None).unwrap();
    b.add_texture(None, img, Some(s)).unwrap();
    let g = common::parse_glb(&b);
    assert_eq!(g.document.samplers().next().unwrap().name(), Some("pixelated"));
}
