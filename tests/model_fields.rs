//! M1(a): every model struct carries `extensions` and `extras`; defaults serialize unchanged.

use mesh_tools::*;
use serde_json::{json, Value};

fn ext() -> Extensions {
    let mut m = Extensions::new();
    m.insert("VENDOR_x".into(), json!({ "a": 1 }));
    m
}

/// Serializes `v` and checks the extras/extensions keys appear with the expected payloads,
/// then deserializes back and re-serializes to the same JSON.
fn check<T: serde::Serialize + serde::de::DeserializeOwned>(v: T) {
    let j = serde_json::to_value(&v).unwrap();
    assert_eq!(j["extensions"], json!({ "VENDOR_x": { "a": 1 } }), "{j}");
    assert_eq!(j["extras"], json!([1, "two"]), "{j}");
    let back: T = serde_json::from_value(j.clone()).unwrap();
    assert_eq!(serde_json::to_value(&back).unwrap(), j);
}

macro_rules! with {
    ($t:ty) => {{
        let mut x = <$t>::default();
        x.extensions = Some(ext());
        x.extras = Some(json!([1, "two"]));
        x
    }};
}

#[test]
fn every_struct_round_trips_extras_and_extensions() {
    check(with!(Gltf));
    check(with!(Asset));
    check(with!(Scene));
    check(with!(Node));
    check(with!(Mesh));
    check(with!(Primitive));
    check(with!(Accessor));
    check(with!(BufferView));
    check(with!(Buffer));
    let mut m = Material::default();
    m.extensions = Some(MaterialExtensions { other: ext(), ..Default::default() });
    m.extras = Some(json!([1, "two"]));
    check(m);
    check(with!(PbrMetallicRoughness));
    check(with!(TextureInfo));
    check(with!(NormalTextureInfo));
    check(with!(OcclusionTextureInfo));
    check(with!(Texture));
    check(with!(Image));
    check(with!(Sampler));
    check(with!(Animation));
    check(with!(AnimationChannel));
    check(with!(AnimationChannelTarget));
    check(with!(AnimationSampler));
    check(with!(Skin));
    check(with!(PbrSpecularGlossiness));
}

#[test]
fn material_extensions_keep_unknown_entries() {
    let src = json!({ "KHR_materials_pbrSpecularGlossiness": { "glossinessFactor": 0.5 },
                      "KHR_other": { "k": true } });
    let m: MaterialExtensions = serde_json::from_value(src.clone()).unwrap();
    assert!(m.pbr_specular_glossiness.is_some());
    assert_eq!(m.other["KHR_other"], json!({ "k": true }));
    assert_eq!(serde_json::to_value(&m).unwrap(), src);
}

#[test]
fn gltf_extras_accepts_any_json_value() {
    let mut g = Gltf::default();
    g.extras = Some(json!("text"));
    assert_eq!(serde_json::to_value(&g).unwrap()["extras"], json!("text"));
}

#[test]
fn asset_min_version_serializes_as_camel_case() {
    let mut a = Asset::default();
    a.version = "2.0".into();
    a.min_version = Some("2.0".into());
    assert_eq!(serde_json::to_value(&a).unwrap(), json!({ "version": "2.0", "minVersion": "2.0" }));
}

fn assert_no_key(v: &Value, keys: &[&str]) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                assert!(!keys.contains(&k.as_str()), "unexpected key {k}");
                assert_no_key(x, keys);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| assert_no_key(x, keys)),
        _ => {}
    }
}

#[test]
fn default_document_has_no_new_keys() {
    let mut b = GltfBuilder::new();
    let mat = b.create_basic_material(None, [1.0, 0.0, 0.0, 1.0]);
    let mesh = b.create_box_with_material(1.0, Some(mat));
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));
    let j: Value = serde_json::to_value(b.inspect_gltf()).unwrap();
    assert_no_key(&j, &["extras", "extensions", "minVersion"]);
    assert_eq!(j["asset"], json!({ "version": "2.0", "generator": "Rust glTF Export Library" }));
}

/// Reads extras/extensions back through the independent `gltf` crate.
#[test]
fn extras_and_extensions_survive_glb_and_read_back_via_gltf_crate() {
    let mut b = GltfBuilder::new();
    let mesh = b.create_box(1.0);
    let node = b.add_node(Some("n".into()), Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));
    b.gltf.extras = Some(json!({ "root": 1 }));
    b.gltf.asset.extras = Some(json!("asset-extra"));
    b.gltf.asset.min_version = Some("2.0".into());
    let n = &mut b.gltf.nodes.as_mut().unwrap()[node];
    n.extras = Some(json!([1, 2]));
    n.extensions = Some(ext());
    b.gltf.meshes.as_mut().unwrap()[mesh].extras = Some(json!({ "m": true }));

    let glb = gltf::Gltf::from_slice(&b.to_glb_bytes().unwrap()).unwrap();
    let doc = &glb.document;
    let get = |r: &Option<Box<serde_json::value::RawValue>>| {
        serde_json::from_str::<Value>(r.as_ref().unwrap().get()).unwrap()
    };
    assert_eq!(get(&doc.as_json().extras), json!({ "root": 1 }));
    assert_eq!(get(&doc.as_json().asset.extras), json!("asset-extra"));
    assert_eq!(doc.as_json().asset.min_version.as_deref(), Some("2.0"));
    let nj = &doc.as_json().nodes[node];
    assert_eq!(get(&nj.extras), json!([1, 2]));
    assert_eq!(nj.extensions.as_ref().unwrap().others["VENDOR_x"], json!({ "a": 1 }));
    assert_eq!(get(&doc.as_json().meshes[mesh].extras), json!({ "m": true }));
}
