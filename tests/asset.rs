mod common;

use mesh_tools::{GltfBuilder, GltfError};

fn is_invalid_input<T: std::fmt::Debug>(r: mesh_tools::Result<T>) -> bool {
    matches!(r, Err(GltfError::InvalidInput(_)))
}

fn asset_of(b: &GltfBuilder) -> gltf::json::asset::Asset {
    common::parse_glb(b).document.as_json().asset.clone()
}

#[test]
fn default_asset_is_unchanged() {
    let a = asset_of(&GltfBuilder::new());
    assert_eq!(a.version, "2.0");
    assert_eq!(a.generator.as_deref(), Some("Rust glTF Export Library"));
    assert_eq!(a.copyright, None);
    assert_eq!(a.min_version, None);
}

#[test]
fn setters_round_trip_through_gltf_crate() {
    let mut b = GltfBuilder::new();
    b.set_copyright("(c) 2024 Example").unwrap();
    b.set_generator("tool 1.2").unwrap();
    b.set_min_version("2.0").unwrap();
    let a = asset_of(&b);
    assert_eq!(a.copyright.as_deref(), Some("(c) 2024 Example"));
    assert_eq!(a.generator.as_deref(), Some("tool 1.2"));
    assert_eq!(a.min_version.as_deref(), Some("2.0"));
    assert_eq!(a.version, "2.0");

    b.clear_generator();
    assert_eq!(asset_of(&b).generator, None);
}

#[test]
fn setters_reject_bad_input_and_leave_document_unchanged() {
    let mut b = GltfBuilder::new();
    b.set_copyright("c").unwrap();
    b.set_min_version("2.0").unwrap();
    assert!(is_invalid_input(b.set_copyright("")));
    assert!(is_invalid_input(b.set_generator("")));
    for bad in ["", "2", "2.", ".0", "a.b", "2.0.1", "2.1", "3.0", "-1.0", "2.0 "] {
        assert!(is_invalid_input(b.set_min_version(bad)), "{bad:?} must be rejected");
    }
    assert_eq!(b.gltf.asset.copyright.as_deref(), Some("c"));
    assert_eq!(b.gltf.asset.min_version.as_deref(), Some("2.0"));
    assert_eq!(b.gltf.asset.generator.as_deref(), Some("Rust glTF Export Library"));
}

fn codes(edit: impl FnOnce(&mut mesh_tools::Asset)) -> mesh_tools::ValidationReport {
    let mut b = GltfBuilder::new();
    edit(&mut b.gltf.asset);
    b.validate()
}

#[test]
fn validator_codes() {
    assert!(codes(|a| a.version = "2".into()).has_error("ASSET_VERSION_INVALID"));
    assert!(codes(|a| a.version = "".into()).has_error("ASSET_VERSION_INVALID"));
    assert!(codes(|a| a.min_version = Some("x".into())).has_error("ASSET_MIN_VERSION_INVALID"));
    assert!(codes(|a| a.min_version = Some("2.1".into())).has_error("ASSET_MIN_VERSION_EXCEEDS_VERSION"));
    assert!(codes(|a| a.min_version = Some("10.0".into())).has_error("ASSET_MIN_VERSION_EXCEEDS_VERSION"));
    assert!(codes(|a| a.copyright = Some(String::new())).has_error("ASSET_COPYRIGHT_EMPTY"));
    assert!(codes(|a| a.generator = Some(String::new())).has_error("ASSET_GENERATOR_EMPTY"));
    // valid forms
    assert!(codes(|a| a.min_version = Some("2.0".into())).is_valid());
    assert!(codes(|a| a.min_version = Some("1.0".into())).is_valid());
    assert!(codes(|a| a.generator = None).is_valid());
}

#[test]
fn invalid_asset_blocks_export() {
    let mut b = GltfBuilder::new();
    b.gltf.asset.copyright = Some(String::new());
    assert!(matches!(b.to_glb_bytes(), Err(GltfError::InvalidData(m)) if m.contains("ASSET_COPYRIGHT_EMPTY")));
}
