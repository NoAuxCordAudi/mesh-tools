//! # Document Validation
//!
//! [`GltfBuilder::validate`] checks the document for structural glTF errors that the
//! individual builder methods cannot see (because the document can also be edited through
//! the public `gltf` field, and because some rules span several objects): dangling indices,
//! buffers, buffer views (range, target, stride, alignment) and accessors (padded element sizes,
//! checked arithmetic, so hand-edited `usize::MAX` values give errors, not panics),
//! hierarchy problems, skin consistency, morph targets with their default and node weights and
//! target names, and animation targets, including the output type and element count of every
//! channel (`weights` channels are checked against the morph target count of the node's mesh).
//!
//! Export ([`GltfBuilder::to_glb_bytes`], [`GltfBuilder::export_glb`]) fails with
//! `GltfError::InvalidData` listing *all* errors. Use
//! [`to_glb_bytes_unchecked`](GltfBuilder::to_glb_bytes_unchecked) /
//! [`export_glb_unchecked`](GltfBuilder::export_glb_unchecked) to skip the check deliberately.
//!
//! Every message starts with a stable upper-case code (for example `SKIN_JOINT_OUT_OF_RANGE:`)
//! so callers and tests can match on it.
//!
//! Warnings never block export.

use crate::builder::GltfBuilder;
use crate::builder_animation::{expected_output_count, interpolation_from_str, output_component_allowed, AnimationPath, InterpolationType};
use crate::accessor::{component_size, decode_element, element_size, integer_range, type_shape};
use crate::constants::buffer_view_target;
use crate::sampler::{MagFilter, MinFilter, WrapMode};
use crate::texture::TextureFormat;
use crate::models::{Accessor, Mesh, Primitive};
use crate::primitive_spec::check_primitive;

/// Result of [`GltfBuilder::validate`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    /// Problems that make the file invalid or wrong. These block export.
    pub errors: Vec<String>,
    /// Suspicious but legal constructs. These never block export.
    pub warnings: Vec<String>,
}

impl ValidationReport {
    /// True if there are no errors (warnings are allowed).
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// True if any error message starts with `code`.
    pub fn has_error(&self, code: &str) -> bool {
        self.errors.iter().any(|e| e.starts_with(code))
    }

    /// True if any warning message starts with `code`.
    pub fn has_warning(&self, code: &str) -> bool {
        self.warnings.iter().any(|w| w.starts_with(code))
    }
}

/// Extension names are `VENDOR_name`: an upper-case vendor prefix, an underscore, then a name.
fn valid_extension_name(n: &str) -> bool {
    match n.split_once('_') {
        Some((vendor, rest)) => {
            !vendor.is_empty()
                && vendor.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                && !rest.is_empty()
                && rest.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        }
        None => false,
    }
}

/// Collect the names under every `extensions` object, reporting non-object payloads.
/// `path` is only for messages. `extras` values are skipped, and so are extension payloads
/// (opaque vendor data, where a key named `extensions` means nothing to glTF). The one exception
/// is the typed spec-gloss payload, whose texture infos are glTF objects that can carry extensions.
fn collect_extensions(
    v: &serde_json::Value,
    path: &str,
    found: &mut std::collections::BTreeSet<String>,
    r: &mut ValidationReport,
) {
    use serde_json::Value;
    match v {
        Value::Object(o) => {
            for (k, child) in o {
                if k == "extras" {
                    continue;
                }
                let here = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                if k == "extensions" {
                    if let Value::Object(exts) = child {
                        for (name, payload) in exts {
                            found.insert(name.clone());
                            if !payload.is_object() {
                                r.errors.push(format!(
                                    "EXTENSION_VALUE_NOT_OBJECT: {here}.{name} must be a JSON object"
                                ));
                            } else if name == crate::builder_extensions::SPEC_GLOSS {
                                collect_extensions(payload, &format!("{here}.{name}"), found, r);
                            }
                        }
                    }
                } else {
                    collect_extensions(child, &here, found, r);
                }
            }
        }
        Value::Array(a) => {
            for (i, child) in a.iter().enumerate() {
                collect_extensions(child, &format!("{path}[{i}]"), found, r);
            }
        }
        _ => {}
    }
}

/// Allowed deviation of a normal's length from 1 (the Khronos validator's tolerance).
const NORMAL_UNIT_TOLERANCE: f64 = 0.00674;

const ANIMATION_PATHS: [&str; 4] = ["translation", "rotation", "scale", "weights"];

impl GltfBuilder {
    /// Validate the whole document. See the [module docs](crate::validate).
    pub fn validate(&self) -> ValidationReport {
        let mut r = ValidationReport::default();
        let doc = Doc::new(self);
        doc.check_asset(&mut r);
        doc.check_extensions(&mut r);
        doc.check_buffers(&mut r);
        doc.check_accessors(&mut r);
        doc.check_view_usage(&mut r);
        doc.check_meshes(&mut r);
        doc.check_morphs(&mut r);
        let parents = doc.check_hierarchy(&mut r);
        doc.check_scenes(&parents, &mut r);
        doc.check_skins(&parents, &mut r);
        doc.check_skinned_nodes(&parents, &mut r);
        doc.check_animations(&mut r);
        doc.check_cameras(&mut r);
        doc.check_textures(&mut r);
        doc.check_materials(&mut r);
        r
    }
}

impl<'a> Doc<'a> {
    /// Materials (glTF 3.9.x): factor ranges, alpha, texture references, and that every
    /// primitive using a material has the `TEXCOORD_n` sets its texture slots sample.
    fn check_materials(&self, r: &mut ValidationReport) {
        let g = &self.b.gltf;
        let materials = g.materials.as_deref().unwrap_or(&[]);
        let textures = g.textures.as_ref().map_or(0, |t| t.len());
        for (i, m) in materials.iter().enumerate() {
            for issue in crate::material::check_material(m, Some(textures)) {
                let list = if issue.warning { &mut r.warnings } else { &mut r.errors };
                list.push(format!("{}: material {i} {}", issue.code, issue.msg));
            }
        }
        let mut used = vec![false; materials.len()];
        for (mi, mesh) in self.meshes.iter().enumerate() {
            for (pi, p) in mesh.primitives.iter().enumerate() {
                let Some(m) = p.material.and_then(|m| used.get_mut(m).map(|u| (m, u))) else { continue };
                *m.1 = true;
                for (slot, _, set) in crate::material::texture_slots(&materials[m.0]) {
                    if !p.attributes.contains_key(&format!("TEXCOORD_{set}")) {
                        r.errors.push(format!(
                            "MATERIAL_TEXCOORD_MISSING: material {} {slot} samples TEXCOORD_{set}, which mesh {mi} primitive {pi} does not have",
                            m.0
                        ));
                    }
                }
            }
        }
        for (i, u) in used.iter().enumerate() {
            if !u {
                r.warnings.push(format!("MATERIAL_UNUSED: material {i} is not used by any primitive"));
            }
        }
    }

    /// Samplers, textures and images (glTF 3.7.x): code values, references, image sources and
    /// embedded image headers (header only, pixels are not decoded).
    fn check_textures(&self, r: &mut ValidationReport) {
        let g = &self.b.gltf;
        let samplers = g.samplers.as_deref().unwrap_or(&[]);
        let textures = g.textures.as_deref().unwrap_or(&[]);
        let images = g.images.as_deref().unwrap_or(&[]);
        let views = g.buffer_views.as_deref().unwrap_or(&[]);

        for (i, s) in samplers.iter().enumerate() {
            if let Some(c) = s.mag_filter.filter(|&c| MagFilter::from_code(c).is_none()) {
                r.errors.push(format!("SAMPLER_MAG_FILTER_INVALID: sampler {i} magFilter {c} is not 9728 or 9729"));
            }
            if let Some(c) = s.min_filter.filter(|&c| MinFilter::from_code(c).is_none()) {
                r.errors.push(format!("SAMPLER_MIN_FILTER_INVALID: sampler {i} minFilter {c} is not 9728, 9729 or 9984-9987"));
            }
            for (axis, w) in [("wrapS", s.wrap_s), ("wrapT", s.wrap_t)] {
                if let Some(c) = w.filter(|&c| WrapMode::from_code(c).is_none()) {
                    r.errors.push(format!("SAMPLER_WRAP_INVALID: sampler {i} {axis} {c} is not 10497, 33071 or 33648"));
                }
            }
        }

        let mut sampler_used = vec![false; samplers.len()];
        let mut image_used = vec![false; images.len()];
        for (i, t) in textures.iter().enumerate() {
            match image_used.get_mut(t.source) {
                Some(u) => *u = true,
                None => r.errors.push(format!(
                    "TEXTURE_SOURCE_OUT_OF_RANGE: texture {i} references image {} ({} present)", t.source, images.len()
                )),
            }
            if let Some(si) = t.sampler {
                match sampler_used.get_mut(si) {
                    Some(u) => *u = true,
                    None => r.errors.push(format!(
                        "TEXTURE_SAMPLER_OUT_OF_RANGE: texture {i} references sampler {si} ({} present)", samplers.len()
                    )),
                }
            }
        }
        for (i, u) in sampler_used.iter().enumerate() {
            if !u {
                r.warnings.push(format!("SAMPLER_UNUSED: sampler {i} is not used by any texture"));
            }
        }
        for (i, u) in image_used.iter().enumerate() {
            if !u {
                r.warnings.push(format!("IMAGE_UNUSED: image {i} is not used by any texture"));
            }
        }

        for (i, im) in images.iter().enumerate() {
            if im.uri.is_some() == im.buffer_view.is_some() {
                r.errors.push(format!("IMAGE_SOURCE_AMBIGUOUS: image {i} needs exactly one of uri and bufferView"));
            }
            let format = match im.mime_type.as_deref() {
                Some(m) => {
                    let f = TextureFormat::from_mime_type(m);
                    if f.is_none() {
                        r.errors.push(format!("IMAGE_MIME_INVALID: image {i} mimeType {m:?} is not image/png or image/jpeg"));
                    }
                    f
                }
                None => {
                    if im.buffer_view.is_some() {
                        r.errors.push(format!("IMAGE_MIME_MISSING: image {i} uses a bufferView and needs a mimeType"));
                    }
                    None
                }
            };
            if im.uri.as_deref() == Some("") {
                r.errors.push(format!("IMAGE_URI_EMPTY: image {i} has an empty uri"));
            }
            if let Some(u) = im.uri.as_deref().filter(|u| crate::uri::is_data_uri(u)) {
                self.check_image_data_uri(i, u, format, r);
            } else if let Some(u) = im.uri.as_deref().filter(|u| !u.is_empty()) {
                if let Err(e) = crate::uri::check_relative_uri(u) {
                    r.errors.push(format!("IMAGE_URI_INVALID: image {i}: {e}"));
                }
            }
            let Some(vi) = im.buffer_view else { continue };
            let Some(v) = views.get(vi) else {
                r.errors.push(format!("IMAGE_BUFFER_VIEW_INVALID: image {i} references buffer view {vi} ({} present)", views.len()));
                continue;
            };
            if v.byte_stride.is_some() || v.target.is_some() {
                r.errors.push(format!("IMAGE_BUFFER_VIEW_INVALID: image {i} buffer view {vi} must not define byteStride or target"));
            }
            let data = v
                .byte_offset
                .checked_add(v.byte_length)
                .and_then(|end| self.b.buffer_data.get(v.buffer)?.get(v.byte_offset..end));
            let Some(data) = data else {
                r.errors.push(format!("IMAGE_BUFFER_VIEW_INVALID: image {i} buffer view {vi} does not lie inside the binary buffer"));
                continue;
            };
            let Some(format) = format else { continue }; // mime problem already reported
            if TextureFormat::from_magic(data) != Some(format) {
                r.errors.push(format!("IMAGE_DATA_MISMATCH: image {i} bytes do not start with a {} signature", format.mime_type()));
                continue;
            }
            match format.header_dimensions(data) {
                Ok((w, h)) if w == 0 || h == 0 => {
                    r.errors.push(format!("IMAGE_DIMENSIONS_INVALID: image {i} has size {w}x{h}"));
                }
                Ok(_) => {}
                Err(e) => r.errors.push(format!("IMAGE_DATA_UNDECODABLE: image {i}: {e}")),
            }
        }
    }
}

fn identity_ish(node: &crate::models::Node) -> bool {
    const I16: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    node.translation.map_or(true, |t| t == [0.0; 3])
        && node.rotation.map_or(true, |r| r == [0.0, 0.0, 0.0, 1.0])
        && node.scale.map_or(true, |s| s == [1.0; 3])
        && node.matrix.map_or(true, |m| m == I16)
}

/// Borrowed view of the document with count helpers.
struct Doc<'a> {
    b: &'a GltfBuilder,
    nodes: &'a [crate::models::Node],
    meshes: &'a [Mesh],
    accessors: &'a [Accessor],
}

impl<'a> Doc<'a> {
    fn new(b: &'a GltfBuilder) -> Self {
        Doc {
            b,
            nodes: b.gltf.nodes.as_deref().unwrap_or(&[]),
            meshes: b.gltf.meshes.as_deref().unwrap_or(&[]),
            accessors: b.gltf.accessors.as_deref().unwrap_or(&[]),
        }
    }

    fn check_asset(&self, r: &mut ValidationReport) {
        use crate::builder_extensions::parse_version;
        let a = &self.b.gltf.asset;
        let version = parse_version(&a.version);
        if version.is_none() {
            r.errors.push(format!("ASSET_VERSION_INVALID: asset version {:?} is not <major>.<minor>", a.version));
        }
        if let Some(min) = &a.min_version {
            match (parse_version(min), version) {
                (None, _) => r.errors.push(format!("ASSET_MIN_VERSION_INVALID: minVersion {min:?} is not <major>.<minor>")),
                (Some(m), Some(v)) if m > v => r.errors.push(format!(
                    "ASSET_MIN_VERSION_EXCEEDS_VERSION: minVersion {min} is greater than version {}", a.version
                )),
                _ => {}
            }
        }
        if a.copyright.as_deref() == Some("") {
            r.errors.push("ASSET_COPYRIGHT_EMPTY: asset.copyright is an empty string".to_string());
        }
        if a.generator.as_deref() == Some("") {
            r.errors.push("ASSET_GENERATOR_EMPTY: asset.generator is an empty string".to_string());
        }
    }

    /// `extensionsUsed`/`extensionsRequired` bookkeeping against the extensions found in the
    /// document (`extras` subtrees are application data and are not searched).
    fn check_extensions(&self, r: &mut ValidationReport) {
        let g = &self.b.gltf;
        let used = g.extensions_used.as_deref();
        let required = g.extensions_required.as_deref();
        for (field, list, empty_code) in [
            ("extensionsUsed", used, "EXTENSIONS_USED_EMPTY"),
            ("extensionsRequired", required, "EXTENSIONS_REQUIRED_EMPTY"),
        ] {
            let Some(list) = list else { continue };
            if list.is_empty() {
                r.errors.push(format!("{empty_code}: {field} is an empty array; omit it instead"));
            }
            for (i, n) in list.iter().enumerate() {
                if list[..i].contains(n) {
                    r.errors.push(format!("EXTENSIONS_USED_DUPLICATE: {field} lists {n} more than once"));
                }
                if !valid_extension_name(n) {
                    r.warnings.push(format!(
                        "EXTENSION_NAME_INVALID: {field} entry {n:?} is not a VENDOR_name style identifier"
                    ));
                }
            }
        }
        for n in required.unwrap_or(&[]) {
            if !used.unwrap_or(&[]).contains(n) {
                r.errors.push(format!("EXTENSIONS_REQUIRED_NOT_USED: {n} is required but not in extensionsUsed"));
            }
        }

        if required.unwrap_or(&[]).iter().any(|n| n == "KHR_materials_pbrSpecularGlossiness") {
            r.warnings.push(
                "SG_EXTENSION_REQUIRED: KHR_materials_pbrSpecularGlossiness is deprecated and should not be required; \
                 list it only in extensionsUsed so viewers can fall back to metallic-roughness"
                    .to_string(),
            );
        }

        // Serialization cannot fail for these plain data types; if it does, nothing is found.
        let mut found = std::collections::BTreeSet::new();
        if let Ok(doc) = serde_json::to_value(g) {
            collect_extensions(&doc, "", &mut found, r);
        }
        for n in &found {
            if !used.unwrap_or(&[]).contains(n) {
                r.errors.push(format!("EXTENSION_NOT_DECLARED: extension {n} is used but not listed in extensionsUsed"));
            }
        }
        for n in used.unwrap_or(&[]) {
            if !found.contains(n) {
                r.warnings.push(format!(
                    "EXTENSION_DECLARED_UNUSED: {n} is in extensionsUsed but no object carries it \
                     (fine for extensions without a payload object)"
                ));
            }
        }
    }

    fn check_image_data_uri(&self, i: usize, uri: &str, declared: Option<TextureFormat>, r: &mut ValidationReport) {
        let (mime, data) = match crate::uri::decode_data_uri(uri) {
            Ok(x) => x,
            Err(e) => return r.errors.push(format!("IMAGE_DATA_URI_INVALID: image {i}: {e}")),
        };
        let Some(format) = TextureFormat::from_mime_type(&mime) else {
            return r.errors.push(format!("IMAGE_DATA_URI_INVALID: image {i} media type {mime:?} is not image/png or image/jpeg"));
        };
        if declared.map_or(false, |d| d != format) {
            return r.errors.push(format!("IMAGE_DATA_URI_INVALID: image {i} mimeType disagrees with its data URI media type {mime:?}"));
        }
        if TextureFormat::from_magic(&data) != Some(format) {
            return r.errors.push(format!("IMAGE_DATA_MISMATCH: image {i} bytes do not start with a {} signature", format.mime_type()));
        }
        match format.header_dimensions(&data) {
            Ok((w, h)) if w == 0 || h == 0 => r.errors.push(format!("IMAGE_DIMENSIONS_INVALID: image {i} has size {w}x{h}")),
            Ok(_) => {}
            Err(e) => r.errors.push(format!("IMAGE_DATA_UNDECODABLE: image {i}: {e}")),
        }
    }

    fn check_buffers(&self, r: &mut ValidationReport) {
        let stored = &self.b.buffer_data;
        let buffers = self.b.gltf.buffers.as_deref().unwrap_or(&[]);
        if buffers.is_empty() && stored.iter().any(|d| !d.is_empty()) {
            r.errors.push("BUFFER_MISSING: binary data exists but there is no buffer".to_string());
        }
        for (i, buf) in buffers.iter().enumerate() {
            if buf.byte_length == 0 {
                r.errors.push(format!("BUFFER_EMPTY: buffer {i} has byteLength 0 (the minimum is 1)"));
            } else if let Some(d) = stored.get(i).filter(|d| d.len() != buf.byte_length) {
                r.errors.push(format!(
                    "BUFFER_LENGTH_MISMATCH: buffer {i} declares {} bytes but {} are stored", buf.byte_length, d.len()
                ));
            }
            if i == 0 && buf.uri.is_some() {
                r.errors.push("BUFFER_URI_ON_GLB_BUFFER: buffer 0 holds the builder's data and must not have a uri; \
                    choose the encoding at export (BufferEncoding)".to_string());
            } else if i > 0 {
                // Only buffer 0 can be the GLB BIN chunk; other buffers need a uri.
                match buf.uri.as_deref() {
                    None => r.errors.push(format!("BUFFER_URI_MISSING: buffer {i} has no uri; only buffer 0 can be the GLB BIN chunk")),
                    Some(u) if crate::uri::is_data_uri(u) => match crate::uri::decode_data_uri(u) {
                        Err(e) => r.errors.push(format!("BUFFER_DATA_URI_INVALID: buffer {i}: {e}")),
                        Ok((mime, _)) if mime != "application/octet-stream" && mime != "application/gltf-buffer" => {
                            r.errors.push(format!("BUFFER_DATA_URI_INVALID: buffer {i} media type {mime:?} is not application/octet-stream"));
                        }
                        Ok((_, d)) if d.len() < buf.byte_length => r.errors.push(format!(
                            "BUFFER_DATA_URI_INVALID: buffer {i} declares {} bytes but its data URI holds {}", buf.byte_length, d.len()
                        )),
                        Ok(_) => {}
                    },
                    Some(u) => {
                        if let Err(e) = crate::uri::check_relative_uri(u) {
                            r.errors.push(format!("BUFFER_URI_INVALID: buffer {i}: {e}"));
                        }
                    }
                }
            }
        }
        let views = self.b.gltf.buffer_views.as_deref().unwrap_or(&[]);
        for i in 0..buffers.len() {
            if !views.iter().any(|v| v.buffer == i) {
                r.warnings.push(format!("BUFFER_UNUSED: buffer {i} is not referenced by any buffer view"));
            } else if stored.get(i).is_none() {
                r.warnings.push(format!(
                    "BUFFER_DATA_UNVERIFIED: buffer {i} has no stored bytes, so the data of its views is only checked against the declared length"
                ));
            }
        }
        for (i, v) in self.b.gltf.buffer_views.iter().flatten().enumerate() {
            // Capacity: the stored bytes if there are any, else the declared length.
            let capacity = match buffers.get(v.buffer) {
                Some(buf) => Some(stored.get(v.buffer).map_or(buf.byte_length, Vec::len)),
                None => {
                    r.errors.push(format!(
                        "BUFFER_VIEW_BUFFER_INVALID: buffer view {i} references buffer {} ({} present)", v.buffer, buffers.len()
                    ));
                    None
                }
            };
            match (v.byte_offset.checked_add(v.byte_length), capacity) {
                (_, None) => {}
                (Some(end), Some(cap)) if end <= cap => {}
                (Some(end), Some(cap)) => r.errors.push(format!(
                    "BUFFER_VIEW_RANGE: buffer view {i} covers bytes {}..{end} but buffer {} has {cap}", v.byte_offset, v.buffer
                )),
                (None, _) => r.errors.push(format!(
                    "BUFFER_VIEW_RANGE: buffer view {i} offset {} + length {} overflows", v.byte_offset, v.byte_length
                )),
            }
            if v.byte_length == 0 {
                r.errors.push(format!("BUFFER_VIEW_LENGTH_ZERO: buffer view {i} has byteLength 0 (the minimum is 1)"));
            }
            if let Some(t) = v.target {
                if t != buffer_view_target::ARRAY_BUFFER && t != buffer_view_target::ELEMENT_ARRAY_BUFFER {
                    r.errors.push(format!(
                        "BUFFER_VIEW_TARGET_INVALID: buffer view {i} target {t} is not 34962 (ARRAY_BUFFER) or 34963 (ELEMENT_ARRAY_BUFFER)"
                    ));
                }
                if v.byte_offset % 4 != 0 {
                    r.warnings.push(format!(
                        "BUFFER_VIEW_OFFSET_ALIGNMENT: buffer view {i} has a target but byteOffset {} is not a multiple of 4; \
                         GPU uploads may need a copy", v.byte_offset
                    ));
                }
            }
            if let Some(s) = v.byte_stride {
                if !(4..=252).contains(&s) {
                    r.errors.push(format!("BUFFER_VIEW_STRIDE_RANGE: buffer view {i} byteStride {s} is outside 4..=252"));
                }
                if s % 4 != 0 {
                    r.errors.push(format!("BUFFER_VIEW_STRIDE_ALIGNMENT: buffer view {i} byteStride {s} is not a multiple of 4"));
                }
                if v.target == Some(buffer_view_target::ELEMENT_ARRAY_BUFFER) {
                    r.errors.push(format!(
                        "BUFFER_VIEW_STRIDE_ON_NON_VERTEX: buffer view {i} has byteStride but target ELEMENT_ARRAY_BUFFER"
                    ));
                }
            }
        }
    }

    fn check_accessors(&self, r: &mut ValidationReport) {
        let views = self.b.gltf.buffer_views.as_deref().unwrap_or(&[]);
        for (i, a) in self.accessors.iter().enumerate() {
            let (Some(elem), Some(comp)) = (element_size(a.component_type, &a.type_), component_size(a.component_type)) else {
                r.errors.push(format!(
                    "ACCESSOR_TYPE_INVALID: accessor {i} has component type {} and type {:?}", a.component_type, a.type_
                ));
                continue;
            };
            if a.count == 0 {
                r.errors.push(format!("ACCESSOR_COUNT_ZERO: accessor {i} has no elements"));
                continue;
            }
            if a.normalized == Some(true) && !matches!(a.component_type, 5120..=5123) {
                r.errors.push(format!(
                    "ACCESSOR_NORMALIZED_INVALID: accessor {i} is normalized but its component type {} is not an 8/16-bit integer", a.component_type
                ));
            }
            let bounds_ok = self.check_bounds(i, a, r);
            let sparse_ok = a.sparse.as_ref().map_or(true, |s| self.check_sparse(i, a, s, r));
            let Some(view_index) = a.buffer_view else {
                // Without a view the accessor reads as zeros; a byteOffset then has nothing to offset.
                if a.byte_offset.is_some() {
                    r.errors.push(format!("ACCESSOR_BUFFER_VIEW_MISSING: accessor {i} has a byteOffset but no bufferView"));
                }
                if a.sparse.is_none() {
                    r.warnings.push(format!("ACCESSOR_NO_DATA: accessor {i} has no bufferView and no sparse, so it reads as all zeros"));
                } else if sparse_ok {
                    self.check_data(i, bounds_ok, r);
                }
                continue;
            };
            let Some(view) = views.get(view_index) else {
                r.errors.push(format!("ACCESSOR_BUFFER_VIEW_INVALID: accessor {i} references buffer view {view_index}"));
                continue;
            };
            let off = a.byte_offset.unwrap_or(0);
            if off % comp != 0 {
                r.errors.push(format!(
                    "ACCESSOR_OFFSET_ALIGNMENT: accessor {i} byteOffset {off} is not a multiple of its component size {comp}"
                ));
            } else if view.byte_offset.checked_add(off).is_some_and(|abs| abs % comp != 0) {
                r.errors.push(format!(
                    "ACCESSOR_OFFSET_ALIGNMENT: accessor {i} buffer view offset {} + byteOffset {off} is not a multiple of its component size {comp}",
                    view.byte_offset
                ));
            }
            if let Some(s) = view.byte_stride {
                if s % comp != 0 {
                    r.errors.push(format!(
                        "ACCESSOR_STRIDE_ALIGNMENT: accessor {i} sits in a view with byteStride {s}, not a multiple of its component size {comp}"
                    ));
                }
                if s < elem {
                    r.errors.push(format!(
                        "BUFFER_VIEW_STRIDE_TOO_SMALL: byteStride {s} of buffer view {view_index} is smaller than the {elem}-byte elements of accessor {i}",
                    ));
                }
            }
            let stride = view.byte_stride.unwrap_or(elem);
            let end = stride
                .checked_mul(a.count - 1)
                .and_then(|span| span.checked_add(elem))
                .and_then(|span| span.checked_add(off));
            if !end.is_some_and(|e| e <= view.byte_length) {
                let need = end.map_or_else(|| "an overflowing number of".to_string(), |e| e.to_string());
                r.errors.push(format!(
                    "ACCESSOR_RANGE: accessor {i} needs {need} bytes but buffer view {view_index} has {}", view.byte_length
                ));
            } else if sparse_ok {
                self.check_data(i, bounds_ok, r);
            }
        }
    }

    /// `len` bytes at `offset` inside buffer view `view`, if the view exists and holds them.
    fn view_bytes(&self, view: usize, offset: usize, len: usize) -> Option<&[u8]> {
        let v = self.b.gltf.buffer_views.as_ref()?.get(view)?;
        let end = offset.checked_add(len)?;
        if end > v.byte_length {
            return None;
        }
        let start = v.byte_offset.checked_add(offset)?;
        self.b.buffer_data.get(v.buffer)?.get(start..start.checked_add(len)?)
    }

    /// Check `accessor.sparse`; returns whether it is sound enough to read the substituted data.
    fn check_sparse(&self, i: usize, a: &Accessor, s: &crate::models::Sparse, r: &mut ValidationReport) -> bool {
        let views = self.b.gltf.buffer_views.as_deref().unwrap_or(&[]);
        let ok = std::cell::Cell::new(true);
        let mut fail = |code: &str, msg: String| {
            r.errors.push(format!("{code}: accessor {i} sparse {msg}"));
            ok.set(false);
        };
        if s.count == 0 || s.count > a.count {
            fail("ACCESSOR_SPARSE_COUNT", format!("count {} is not in 1..={}", s.count, a.count));
        }
        let index_size: Option<usize> = match s.indices.component_type {
            5121 => Some(1),
            5123 => Some(2),
            5125 => Some(4),
            t => {
                fail("ACCESSOR_SPARSE_INDICES_TYPE", format!("indices component type {t} is not 5121, 5123 or 5125"));
                None
            }
        };
        let elem = element_size(a.component_type, &a.type_).unwrap_or(0);
        // (name, view, byteOffset, bytes needed, alignment)
        let parts = [
            ("indices", s.indices.buffer_view, s.indices.byte_offset, index_size.and_then(|z| z.checked_mul(s.count)), index_size),
            ("values", s.values.buffer_view, s.values.byte_offset, elem.checked_mul(s.count), component_size(a.component_type)),
        ];
        for (name, vi, off, need, align) in parts {
            let Some(view) = views.get(vi) else {
                fail("ACCESSOR_SPARSE_VIEW_INVALID", format!("{name} reference buffer view {vi}"));
                continue;
            };
            if view.target.is_some() || view.byte_stride.is_some() {
                fail("ACCESSOR_SPARSE_VIEW_INVALID", format!("{name} buffer view {vi} has a target or byteStride"));
            }
            let off = off.unwrap_or(0);
            if let Some(al) = align.filter(|&al| off % al != 0 || view.byte_offset.checked_add(off).is_some_and(|abs| abs % al != 0)) {
                fail("ACCESSOR_OFFSET_ALIGNMENT", format!("{name} byteOffset {off} is not aligned to {al} bytes"));
            }
            let fits = need.and_then(|n| off.checked_add(n)).is_some_and(|end| end <= view.byte_length);
            if need.is_some() && !fits {
                fail("ACCESSOR_SPARSE_RANGE", format!("{name} need {} bytes at offset {off} but buffer view {vi} has {}", need.unwrap(), view.byte_length));
            }
        }
        if !ok.get() {
            return false;
        }
        let Some(indices) = self.sparse_indices(s) else {
            fail("ACCESSOR_SPARSE_RANGE", "indices extend past the buffer data".to_string());
            return false;
        };
        if let Some(k) = (1..indices.len()).find(|&k| indices[k - 1] >= indices[k]) {
            fail("ACCESSOR_SPARSE_INDICES_ORDER", format!("indices are not strictly increasing at entry {k} ({} after {})", indices[k], indices[k - 1]));
        }
        if let Some(&bad) = indices.iter().find(|&&x| x >= a.count) {
            fail("ACCESSOR_SPARSE_INDEX_RANGE", format!("index {bad} is not below the accessor count {}", a.count));
        }
        ok.get()
    }

    /// The index list of `s`; the caller has checked type, view and range.
    fn sparse_indices(&self, s: &crate::models::Sparse) -> Option<Vec<usize>> {
        let size = component_size(s.indices.component_type)?;
        let bytes = self.view_bytes(s.indices.buffer_view, s.indices.byte_offset.unwrap_or(0), size.checked_mul(s.count)?)?;
        Some(
            bytes
                .chunks_exact(size)
                .map(|c| match size {
                    1 => c[0] as usize,
                    2 => u16::from_le_bytes([c[0], c[1]]) as usize,
                    _ => u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize,
                })
                .collect(),
        )
    }

    /// Call `f(element index, components)` for every element of the substituted array that
    /// is not shadowed: base elements the sparse data does not replace, then the sparse values.
    /// A zero base (no view) yields one zero element if any are left. `None` if unreadable.
    fn for_each_effective(&self, i: usize, f: &mut dyn FnMut(usize, Vec<f64>)) -> Option<()> {
        let a = self.accessors.get(i)?;
        let sparse = match &a.sparse {
            Some(s) => {
                let indices = self.sparse_indices(s)?;
                let elem = element_size(a.component_type, &a.type_)?;
                let bytes = self.view_bytes(s.values.buffer_view, s.values.byte_offset.unwrap_or(0), elem.checked_mul(s.count)?)?;
                let values: Vec<Vec<f64>> =
                    bytes.chunks_exact(elem).map(|c| decode_element(a.component_type, &a.type_, c)).collect::<Option<_>>()?;
                Some((indices, values))
            }
            None => None,
        };
        let replaced = |e: usize| sparse.as_ref().is_some_and(|(idx, _)| idx.binary_search(&e).is_ok());
        if a.buffer_view.is_some() {
            for (e, bytes) in self.accessor_elements(i)?.1.enumerate() {
                if !replaced(e) {
                    f(e, decode_element(a.component_type, &a.type_, bytes)?);
                }
            }
        } else if let Some((idx, _)) = &sparse {
            let gap = idx.iter().enumerate().find(|&(k, &x)| k != x).map_or(idx.len(), |(k, _)| k);
            if gap < a.count {
                let comps = type_shape(&a.type_).map(|(c, r)| c * r)?;
                f(gap, vec![0.0; comps]);
            }
        }
        for (k, v) in sparse.into_iter().flat_map(|(idx, vals)| idx.into_iter().zip(vals)) {
            f(k, v);
        }
        Some(())
    }

    /// Shape checks on `min`/`max`: length, value domain, order. Returns whether both are
    /// present and well-formed, so they can be compared with the data.
    fn check_bounds(&self, i: usize, a: &Accessor, r: &mut ValidationReport) -> bool {
        let comps = crate::accessor::type_shape(&a.type_).map_or(0, |(c, rw)| c * rw);
        let mut ok = a.min.is_some() && a.max.is_some();
        for (name, b) in [("min", &a.min), ("max", &a.max)] {
            let Some(b) = b else { continue };
            if b.len() != comps {
                r.errors.push(format!("ACCESSOR_BOUNDS_LENGTH: accessor {i} {name} has {} values, expected {comps}", b.len()));
                ok = false;
                continue;
            }
            let bad = b.iter().position(|v| match integer_range(a.component_type) {
                Some((lo, hi)) => !v.is_finite() || v.fract() != 0.0 || *v < lo || *v > hi,
                None => !v.is_finite(),
            });
            if let Some(k) = bad {
                r.errors.push(format!(
                    "ACCESSOR_BOUNDS_TYPE: accessor {i} {name}[{k}] = {} is not a valid value of component type {}", b[k], a.component_type
                ));
                ok = false;
            }
        }
        if let (true, Some(min), Some(max)) = (ok, &a.min, &a.max) {
            if let Some(k) = (0..comps).find(|&k| min[k] > max[k]) {
                r.errors.push(format!("ACCESSOR_BOUNDS_ORDER: accessor {i} min[{k}] = {} exceeds max[{k}] = {}", min[k], max[k]));
                ok = false;
            }
        }
        ok
    }

    /// Compare the data with the accessor's bounds and look for NaN / infinity.
    fn check_data(&self, i: usize, bounds_ok: bool, r: &mut ValidationReport) {
        let Some(a) = self.accessors.get(i) else { return };
        let is_float = a.component_type == 5126;
        let mut seen: Option<(Vec<f64>, Vec<f64>)> = None;
        let mut nonfinite = None;
        let readable = self.for_each_effective(i, &mut |e, vals| {
            if is_float && nonfinite.is_none() && vals.iter().any(|v| !v.is_finite()) {
                nonfinite = Some(e);
            }
            match &mut seen {
                None => seen = Some((vals.clone(), vals)),
                Some((lo, hi)) => {
                    for (k, v) in vals.iter().enumerate() {
                        lo[k] = lo[k].min(*v);
                        hi[k] = hi[k].max(*v);
                    }
                }
            }
        });
        if readable.is_none() {
            return;
        }
        if let Some(e) = nonfinite {
            r.errors.push(format!("ACCESSOR_DATA_NONFINITE: accessor {i} element {e} contains NaN or infinity"));
            return;
        }
        let (Some((lo, hi)), true, Some(min), Some(max)) = (seen, bounds_ok, &a.min, &a.max) else { return };
        let same = |stored: f64, actual: f64| if is_float { stored as f32 == actual as f32 } else { stored == actual };
        for (name, stored, actual) in [("min", min, &lo), ("max", max, &hi)] {
            if let Some(k) = (0..actual.len()).find(|&k| !same(stored[k], actual[k])) {
                r.errors.push(format!(
                    "ACCESSOR_BOUNDS_MISMATCH: accessor {i} {name}[{k}] is {} but the data's is {}", stored[k], actual[k]
                ));
            }
        }
    }

    /// Every accessor used as indices or as a vertex attribute (morph targets included) must sit
    /// in a view whose target, if any, matches that use; index data cannot be strided.
    fn check_view_usage(&self, r: &mut ValidationReport) {
        let views = self.b.gltf.buffer_views.as_deref().unwrap_or(&[]);
        let view_of = |acc: usize| {
            let vi = self.accessors.get(acc)?.buffer_view?;
            views.get(vi).map(|v| (vi, v))
        };
        for (mi, m) in self.meshes.iter().enumerate() {
            for (pi, p) in m.primitives.iter().enumerate() {
                if let Some((vi, v)) = p.indices.and_then(view_of) {
                    if v.target == Some(buffer_view_target::ARRAY_BUFFER) {
                        r.errors.push(format!(
                            "BUFFER_VIEW_TARGET_MISMATCH: mesh {mi} primitive {pi} indices use buffer view {vi}, whose target is ARRAY_BUFFER"
                        ));
                    }
                    if v.byte_stride.is_some() {
                        r.errors.push(format!(
                            "BUFFER_VIEW_STRIDE_ON_NON_VERTEX: mesh {mi} primitive {pi} indices use buffer view {vi}, which has a byteStride"
                        ));
                    }
                }
                let targets = p.targets.iter().flatten();
                for (name, &acc) in p.attributes.iter().chain(targets.flat_map(|t| t.iter())) {
                    if let Some((vi, v)) = view_of(acc) {
                        if v.target == Some(buffer_view_target::ELEMENT_ARRAY_BUFFER) {
                            r.errors.push(format!(
                                "BUFFER_VIEW_TARGET_MISMATCH: mesh {mi} primitive {pi} {name} uses buffer view {vi}, whose target is ELEMENT_ARRAY_BUFFER"
                            ));
                        }
                    }
                }
            }
        }
        // Animation samplers and inverse bind matrices are non-vertex data: no byteStride (3.6.2.4)
        let anim_accessors = self.b.gltf.animations.iter().flatten().enumerate().flat_map(|(ai, an)| {
            an.samplers.iter().flatten().enumerate().flat_map(move |(si, s)| {
                [(format!("animation {ai} sampler {si} input"), s.input), (format!("animation {ai} sampler {si} output"), s.output)]
            })
        });
        let ibm_accessors = self.b.gltf.skins.iter().flatten().enumerate()
            .filter_map(|(si, s)| s.inverse_bind_matrices.map(|a| (format!("skin {si} inverseBindMatrices"), a)));
        for (what, acc) in anim_accessors.chain(ibm_accessors) {
            if let Some((vi, v)) = view_of(acc) {
                if v.byte_stride.is_some() {
                    r.errors.push(format!(
                        "BUFFER_VIEW_STRIDE_ON_NON_VERTEX: {what} accessor {acc} uses buffer view {vi}, which has a byteStride"
                    ));
                }
            }
        }
    }

    fn check_meshes(&self, r: &mut ValidationReport) {
        let material_count = self.b.gltf.materials.as_ref().map_or(0, |m| m.len());
        for (mi, mesh) in self.meshes.iter().enumerate() {
            if mesh.primitives.is_empty() {
                r.errors.push(format!("MESH_NO_PRIMITIVES: mesh {mi} has no primitives"));
            }
            for (pi, p) in mesh.primitives.iter().enumerate() {
                for issue in check_primitive(p, self.accessors, material_count) {
                    let list = if issue.warning { &mut r.warnings } else { &mut r.errors };
                    list.push(format!("{}: mesh {mi} primitive {pi} {}", issue.code, issue.msg));
                }
                self.check_primitive_data(mi, pi, p, r);
                self.check_skin_attributes(mi, pi, p, r);
            }
        }
    }

    /// Checks that need the buffer data: index values, tangent handedness, normal length.
    /// Unreadable accessors (no view, bad range) and sparse ones (base data only) are skipped; other codes report them.
    fn check_primitive_data(&self, mi: usize, pi: usize, p: &Primitive, r: &mut ValidationReport) {
        let vertices = p.attributes.get("POSITION").or_else(|| p.attributes.values().next()).and_then(|&a| self.accessors.get(a)).map(|a| a.count);
        if let (Some(idx), Some(n)) = (p.indices, vertices) {
            if let Some((a, elems)) = self.accessor_elements(idx).filter(|(a, _)| a.sparse.is_none() && a.type_ == "SCALAR") {
                let (restart, width) = match a.component_type {
                    5121 => (u8::MAX as u64, 1),
                    5123 => (u16::MAX as u64, 2),
                    5125 => (u32::MAX as u64, 4),
                    _ => (0, 0),
                };
                let (mut bad_restart, mut bad_range) = (None, None);
                for (k, e) in elems.enumerate().filter(|_| width > 0) {
                    let mut le = [0u8; 4];
                    le[..width].copy_from_slice(&e[..width]);
                    let v = u32::from_le_bytes(le) as u64;
                    if v == restart {
                        bad_restart.get_or_insert((k, v));
                    } else if v >= n as u64 {
                        bad_range.get_or_insert((k, v));
                    }
                }
                if let Some((k, v)) = bad_restart {
                    r.errors.push(format!("PRIMITIVE_INDEX_RESTART: mesh {mi} primitive {pi} index {k} is {v}, the primitive restart value"));
                }
                if let Some((k, v)) = bad_range {
                    r.errors.push(format!("PRIMITIVE_INDEX_OUT_OF_RANGE: mesh {mi} primitive {pi} index {k} is {v} but there are only {n} vertices"));
                }
            }
        }
        if let Some(&t) = p.attributes.get("TANGENT") {
            if let Some(data) = self.read_f32(t).filter(|_| self.accessors[t].type_ == "VEC4" && self.accessors[t].sparse.is_none()) {
                if let Some(k) = data.chunks_exact(4).position(|c| c[3].abs() != 1.0) {
                    r.errors.push(format!("PRIMITIVE_TANGENT_W_INVALID: mesh {mi} primitive {pi} tangent {k} has w = {}, expected 1 or -1", data[k * 4 + 3]));
                }
            }
        }
        if let Some(&nrm) = p.attributes.get("NORMAL") {
            if let Some(data) = self.read_f32(nrm).filter(|_| self.accessors[nrm].type_ == "VEC3" && self.accessors[nrm].sparse.is_none()) {
                let off = data.chunks_exact(3).position(|c| {
                    let len = c.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>().sqrt();
                    len.is_finite() && (len - 1.0).abs() > NORMAL_UNIT_TOLERANCE
                });
                if let Some(k) = off {
                    r.warnings.push(format!("PRIMITIVE_NORMAL_NOT_UNIT: mesh {mi} primitive {pi} normal {k} is not unit length"));
                }
            }
        }
    }

    /// Returns each node's parent as derived from the document's `children` lists.
    fn check_hierarchy(&self, r: &mut ValidationReport) -> Vec<Option<usize>> {
        let n = self.nodes.len();
        let mut parents: Vec<Option<usize>> = vec![None; n];
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(m) = node.mesh {
                if m >= self.meshes.len() {
                    r.errors.push(format!("NODE_MESH_INVALID: node {i} references mesh {m} ({} meshes)", self.meshes.len()));
                }
            }
            self.check_node_transform(i, node, r);
            if node.children.as_ref().is_some_and(|c| c.is_empty()) {
                r.errors.push(format!("NODE_CHILDREN_EMPTY: node {i} has an empty children list (omit it instead)"));
            }
            if node.matrix.is_some() && (node.translation.is_some() || node.rotation.is_some() || node.scale.is_some()) {
                r.errors.push(format!("NODE_MATRIX_AND_TRS: node {i} has both a matrix and translation/rotation/scale"));
            }
            for &c in node.children.iter().flatten() {
                if c >= n {
                    r.errors.push(format!("NODE_CHILD_INVALID: node {i} lists child {c} ({n} nodes)"));
                } else if let Some(prev) = parents[c] {
                    r.errors.push(format!("NODE_PARENT_OVERRIDE: node {c} is a child of both node {prev} and node {i}"));
                } else {
                    parents[c] = Some(i);
                }
            }
        }
        for i in 0..n {
            if root_of(&parents, i).is_none() {
                r.errors.push(format!("NODE_CYCLE: node {i} is part of a cycle in the node hierarchy"));
            }
        }
        parents
    }

    fn check_node_transform(&self, i: usize, node: &crate::models::Node, r: &mut ValidationReport) {
        use crate::builder_node::{matrix_has_shear, matrix_problem, UNIT_TOLERANCE};
        if let Some(m) = &node.matrix {
            match matrix_problem(m) {
                Some((code, msg)) => r.errors.push(format!("{code}: node {i}: {msg}")),
                None if matrix_has_shear(m) => r.warnings.push(format!(
                    "NODE_MATRIX_SHEAR: node {i} matrix has shear; it is not exactly a TRS transform and viewers may differ"
                )),
                None => {}
            }
        }
        if node.translation.is_some_and(|t| t.iter().any(|v| !v.is_finite())) {
            r.errors.push(format!("NODE_TRANSLATION_NOT_FINITE: node {i} translation is not finite"));
        }
        if let Some(s) = node.scale {
            if s.iter().any(|v| !v.is_finite()) {
                r.errors.push(format!("NODE_SCALE_NOT_FINITE: node {i} scale is not finite"));
            } else if s.contains(&0.0) {
                r.warnings.push(format!("NODE_SCALE_ZERO: node {i} has a zero scale component; the node is degenerate"));
            }
        }
        if let Some(q) = node.rotation {
            if q.iter().any(|v| !v.is_finite()) {
                r.errors.push(format!("NODE_ROTATION_NOT_FINITE: node {i} rotation is not finite"));
            } else {
                let len = q.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>().sqrt();
                if (len - 1.0).abs() > UNIT_TOLERANCE {
                    r.errors.push(format!("NODE_ROTATION_NOT_UNIT: node {i} rotation {q:?} has length {len}"));
                }
            }
        }
    }

    fn check_scenes(&self, parents: &[Option<usize>], r: &mut ValidationReport) {
        let scenes = self.b.gltf.scenes.as_deref().unwrap_or(&[]);
        if let Some(s) = self.b.gltf.scene {
            if s >= scenes.len() {
                r.errors.push(format!("SCENE_DEFAULT_INVALID: default scene {s} does not exist ({} scenes)", scenes.len()));
            }
        }
        if !scenes.is_empty() && self.b.gltf.scene.is_none() {
            r.warnings.push("NO_DEFAULT_SCENE: the document has scenes but no default scene".to_string());
        }
        for (si, scene) in scenes.iter().enumerate() {
            if scene.nodes.as_ref().is_some_and(|n| n.is_empty()) {
                r.errors.push(format!("SCENE_NODES_EMPTY: scene {si} has an empty nodes list (omit it instead)"));
            }
            let mut seen = std::collections::HashSet::new();
            for &n in scene.nodes.iter().flatten() {
                if !seen.insert(n) {
                    r.errors.push(format!("SCENE_NODE_DUPLICATE: scene {si} lists node {n} more than once"));
                }
            }
            for &n in scene.nodes.iter().flatten() {
                if n >= self.nodes.len() {
                    r.errors.push(format!("SCENE_NODE_INVALID: scene {si} lists node {n} ({} nodes)", self.nodes.len()));
                } else if let Some(p) = parents[n] {
                    r.errors.push(format!("SCENE_ROOT_HAS_PARENT: scene {si} lists node {n}, which is a child of node {p}"));
                }
            }
        }
    }

    fn check_skins(&self, parents: &[Option<usize>], r: &mut ValidationReport) {
        let views = self.b.gltf.buffer_views.as_deref().unwrap_or(&[]);
        for (si, skin) in self.b.gltf.skins.iter().flatten().enumerate() {
            // Inverse bind matrices are non-vertex data: the view must not carry a target. (A
            // byteStride is already reported as BUFFER_VIEW_STRIDE_ON_NON_VERTEX.)
            if let Some(view) = skin.inverse_bind_matrices.and_then(|a| self.accessors.get(a)).and_then(|a| a.buffer_view) {
                if let Some(t) = views.get(view).and_then(|v| v.target) {
                    r.errors.push(format!(
                        "SKIN_IBM_VIEW_TARGET: skin {si} inverse bind matrices use buffer view {view}, which has target {t}; it must have none"
                    ));
                }
            }
            if skin.joints.is_empty() {
                r.errors.push(format!("SKIN_NO_JOINTS: skin {si} has no joints"));
                continue;
            }
            let mut seen = std::collections::HashSet::new();
            let mut roots = std::collections::HashSet::new();
            for &j in &skin.joints {
                if j >= self.nodes.len() {
                    r.errors.push(format!("SKIN_JOINT_INVALID: skin {si} joint node {j} does not exist ({} nodes)", self.nodes.len()));
                    continue;
                }
                if !seen.insert(j) {
                    r.errors.push(format!("SKIN_JOINT_DUPLICATE: skin {si} lists joint node {j} more than once"));
                }
                if let Some(root) = root_of(parents, j) {
                    roots.insert(root);
                }
                if let Some(s) = skin.skeleton {
                    if s < self.nodes.len() && !is_ancestor_or_self(parents, s, j) {
                        r.errors.push(format!("SKIN_SKELETON_NOT_ANCESTOR: skin {si} skeleton node {s} is not an ancestor of joint node {j}"));
                    }
                }
            }
            if roots.len() > 1 {
                r.errors.push(format!("SKIN_NO_COMMON_ROOT: skin {si} joints are spread over {} separate hierarchies", roots.len()));
            }
            if let Some(s) = skin.skeleton {
                if s >= self.nodes.len() {
                    r.errors.push(format!("SKIN_SKELETON_INVALID: skin {si} skeleton node {s} does not exist"));
                }
            }
            if let Some(acc) = skin.inverse_bind_matrices {
                match self.accessors.get(acc) {
                    None => r.errors.push(format!("SKIN_IBM_INVALID: skin {si} references accessor {acc}")),
                    Some(a) if a.type_ != "MAT4" || a.component_type != 5126 => r.errors.push(format!(
                        "SKIN_IBM_TYPE: skin {si} inverse bind matrix accessor {acc} must be FLOAT MAT4"
                    )),
                    Some(a) if a.count < skin.joints.len() => r.errors.push(format!(
                        "SKIN_IBM_COUNT: skin {si} has {} joints but accessor {acc} holds {} matrices", skin.joints.len(), a.count
                    )),
                    _ => {}
                }
            }
        }
    }

    /// Output accessor type and element count of one channel, judged by that channel's own
    /// path, target node and sampler interpolation (a sampler shared by several channels is
    /// checked once per channel).
    fn check_channel_output(
        &self,
        ai: usize,
        ci: usize,
        c: &crate::models::AnimationChannel,
        samplers: &[crate::models::AnimationSampler],
        r: &mut ValidationReport,
    ) {
        let path = c.target.path.as_str();
        let Some(expected_type) = (match path {
            "translation" | "scale" => Some("VEC3"),
            "rotation" => Some("VEC4"),
            "weights" => Some("SCALAR"),
            _ => None, // reported as ANIMATION_CHANNEL_PATH_INVALID
        }) else { return };
        let at = format!("animation {ai} channel {ci} ({path} of node {})", c.target.node);
        let Some(sampler) = samplers.get(c.sampler) else { return };
        let (Some(input), Some(output)) = (self.accessors.get(sampler.input), self.accessors.get(sampler.output)) else { return };

        // Rotation and weights may legally use normalized integer outputs; translation and scale may not.
        let anim_path = match path {
            "translation" => AnimationPath::Translation,
            "rotation" => AnimationPath::Rotation,
            "scale" => AnimationPath::Scale,
            _ => AnimationPath::Weights,
        };
        let type_ok = output.type_ == expected_type && output_component_allowed(anim_path, output.component_type, output.normalized);
        if !type_ok {
            r.errors.push(format!(
                "ANIMATION_OUTPUT_TYPE: {at}: sampler {} output accessor {} is {} with component type {}{}, expected {expected_type} {}",
                c.sampler,
                sampler.output,
                output.type_,
                output.component_type,
                if output.normalized == Some(true) { " (normalized)" } else { "" },
                if matches!(path, "rotation" | "weights") { "FLOAT or normalized integer" } else { "FLOAT" },
            ));
            return;
        }

        let interpolation = sampler.interpolation.as_deref().and_then(interpolation_from_str).unwrap_or(InterpolationType::Linear);
        if interpolation == InterpolationType::CubicSpline && input.count < 2 {
            r.errors.push(format!("ANIMATION_OUTPUT_COUNT: {at}: CUBICSPLINE needs at least two keyframes, found {}", input.count));
        }

        let width = if path == "weights" {
            let m = self.nodes.get(c.target.node).and_then(|n| n.mesh).and_then(|mi| self.meshes.get(mi)).map_or(0, |mesh| {
                mesh.primitives.iter().map(|p| p.targets.as_ref().map_or(0, |t| t.len())).find(|&n| n > 0).unwrap_or(0)
            });
            // An invalid node is reported as ANIMATION_CHANNEL_NODE_INVALID elsewhere.
            if m == 0 && self.nodes.get(c.target.node).is_some() {
                r.errors.push(format!("ANIMATION_CHANNEL_WEIGHTS_NO_MORPHS: {at}: the node has no mesh with morph targets"));
                return;
            }
            m
        } else {
            1
        };
        if width == 0 {
            return;
        }
        let expected = expected_output_count(input.count, interpolation, width);
        if output.count != expected {
            r.errors.push(format!(
                "ANIMATION_OUTPUT_COUNT: {at}: {} {interpolation} keyframes{} need {expected} output elements but output accessor {} has {}",
                input.count,
                if path == "weights" { format!(" and {width} morph targets") } else { String::new() },
                sampler.output,
                output.count
            ));
        } else if path == "rotation" {
            self.check_rotation_unit(&at, sampler.output, interpolation == InterpolationType::CubicSpline, r);
        }
    }

    /// Stored rotation quaternions (the value entries only, for cubic splines) must be unit
    /// length. Normalized integers are decoded with the glTF formulas and judged with their
    /// quantization step as tolerance (`1.05 / max`, the worst case for a quantized unit
    /// quaternion); floats use the node rotation tolerance. Reports the first offender.
    fn check_rotation_unit(&self, at: &str, output: usize, cubic: bool, r: &mut ValidationReport) {
        let Some((a, elems)) = self.accessor_elements(output).filter(|(a, _)| a.sparse.is_none() && a.type_ == "VEC4") else { return };
        let (scale, tolerance) = match (a.component_type, a.normalized == Some(true)) {
            (5126, _) => (None, crate::builder_node::UNIT_TOLERANCE),
            (5120, true) => (Some(127.0), 1.05 / 127.0),
            (5122, true) => (Some(32767.0), 1.05 / 32767.0),
            (5121, true) => (Some(255.0), 1.05 / 255.0),
            (5123, true) => (Some(65535.0), 1.05 / 65535.0),
            _ => return,
        };
        for (e, bytes) in elems.enumerate() {
            if cubic && e % 3 != 1 {
                continue;
            }
            let Some(raw) = decode_element(a.component_type, &a.type_, bytes) else { return };
            // Signed normalized: max(c / max, -1); unsigned: c / max.
            let q: Vec<f64> = raw.iter().map(|&c| scale.map_or(c, |m| (c / m).max(-1.0))).collect();
            if q.iter().any(|v| !v.is_finite()) {
                return; // reported as ACCESSOR_DATA_NONFINITE
            }
            let len = q.iter().map(|v| v * v).sum::<f64>().sqrt();
            if (len - 1.0).abs() > tolerance {
                let keyframe = if cubic { e / 3 } else { e };
                r.errors.push(format!("ANIMATION_ROTATION_NOT_UNIT: {at}: keyframe {keyframe} rotation {q:?} has length {len}"));
                return;
            }
        }
    }

    /// Cameras (glTF 3.12): projection type and objects, value ranges, node references.
    fn check_cameras(&self, r: &mut ValidationReport) {
        use crate::builder_camera::positive;
        let cameras = self.b.gltf.cameras.as_deref().unwrap_or(&[]);
        let mut used = vec![false; cameras.len()];
        for (ni, node) in self.nodes.iter().enumerate() {
            let Some(c) = node.camera else { continue };
            match used.get_mut(c) {
                Some(u) => *u = true,
                None => r.errors.push(format!(
                    "NODE_CAMERA_INVALID: node {ni} references camera {c} ({} cameras)", cameras.len()
                )),
            }
        }
        for (i, c) in cameras.iter().enumerate() {
            let e = &mut r.errors;
            match c.type_.as_str() {
                "perspective" | "orthographic" => {
                    let (this, other) = if c.type_ == "perspective" {
                        (c.perspective.is_some(), c.orthographic.is_some())
                    } else {
                        (c.orthographic.is_some(), c.perspective.is_some())
                    };
                    if !this || other {
                        e.push(format!(
                            "CAMERA_TYPE_MISMATCH: camera {i} has type {:?} but perspective is {} and orthographic is {}",
                            c.type_,
                            if c.perspective.is_some() { "present" } else { "absent" },
                            if c.orthographic.is_some() { "present" } else { "absent" },
                        ));
                    }
                }
                t => e.push(format!("CAMERA_TYPE_INVALID: camera {i} type {t:?} is not \"perspective\" or \"orthographic\"")),
            }
            // A mismatched camera is still checked through whichever projection objects exist.
            if let Some(p) = &c.perspective {
                if !positive(p.yfov) {
                    e.push(format!("CAMERA_YFOV_INVALID: camera {i} yfov {} must be finite and > 0", p.yfov));
                } else if p.yfov >= std::f32::consts::PI {
                    r.warnings.push(format!("CAMERA_YFOV_TOO_LARGE: camera {i} yfov {} is at least pi", p.yfov));
                }
                let e = &mut r.errors;
                if let Some(a) = p.aspect_ratio.filter(|&a| !positive(a)) {
                    e.push(format!("CAMERA_ASPECT_RATIO_INVALID: camera {i} aspectRatio {a} must be finite and > 0"));
                }
                if !positive(p.znear) {
                    e.push(format!("CAMERA_ZNEAR_INVALID: camera {i} znear {} must be finite and > 0", p.znear));
                }
                if let Some(z) = p.zfar.filter(|&z| !(z.is_finite() && z > 0.0 && (!p.znear.is_finite() || z > p.znear))) {
                    e.push(format!("CAMERA_ZFAR_INVALID: camera {i} zfar {z} must be finite and > znear {}", p.znear));
                }
            }
            if let Some(o) = &c.orthographic {
                for (code, what, v) in [("CAMERA_XMAG_INVALID", "xmag", o.xmag), ("CAMERA_YMAG_INVALID", "ymag", o.ymag)] {
                    if !v.is_finite() || v == 0.0 {
                        r.errors.push(format!("{code}: camera {i} {what} {v} must be finite and not 0"));
                    } else if v < 0.0 {
                        r.warnings.push(format!("CAMERA_MAG_NEGATIVE: camera {i} {what} {v} is negative and mirrors the view"));
                    }
                }
                let e = &mut r.errors;
                if !(o.znear.is_finite() && o.znear >= 0.0) {
                    e.push(format!("CAMERA_ZNEAR_INVALID: camera {i} znear {} must be finite and >= 0", o.znear));
                }
                if !(o.zfar.is_finite() && o.zfar > 0.0 && (!o.znear.is_finite() || o.zfar > o.znear)) {
                    e.push(format!("CAMERA_ZFAR_INVALID: camera {i} zfar {} must be finite and > znear {}", o.zfar, o.znear));
                }
            }
            if !used[i] {
                r.warnings.push(format!("CAMERA_UNUSED: camera {i} is not referenced by any node"));
            }
        }
    }

    fn check_skinned_nodes(&self, parents: &[Option<usize>], r: &mut ValidationReport) {
        let skins = self.b.gltf.skins.as_deref().unwrap_or(&[]);
        for (ni, node) in self.nodes.iter().enumerate() {
            let Some(si) = node.skin else {
                let skinned_mesh = node.mesh.and_then(|m| self.meshes.get(m)).filter(|m| m.primitives.iter().any(|p| p.attributes.contains_key("JOINTS_0")));
                if let (Some(mi), Some(_)) = (node.mesh, skinned_mesh) {
                    r.warnings.push(format!(
                        "SKINNED_MESH_WITHOUT_SKIN: node {ni} renders mesh {mi}, which has JOINTS_0 data, but has no skin; it is drawn unskinned"
                    ));
                }
                continue;
            };
            if parents.get(ni).copied().flatten().is_some() {
                r.warnings.push(format!(
                    "SKINNED_NODE_NOT_ROOT: node {ni} has skin {si} but is a child of node {}; parent transforms do not affect a skinned mesh",
                    parents[ni].unwrap()
                ));
            }
            let Some(skin) = skins.get(si) else {
                r.errors.push(format!("NODE_SKIN_INVALID: node {ni} references skin {si} ({} skins)", skins.len()));
                continue;
            };
            let Some(mesh) = node.mesh.and_then(|m| self.meshes.get(m)) else {
                if node.mesh.is_none() {
                    r.errors.push(format!("NODE_SKIN_WITHOUT_MESH: node {ni} has skin {si} but no mesh"));
                }
                continue;
            };
            if !identity_ish(node) {
                r.warnings.push(format!(
                    "SKINNED_NODE_LOCAL_TRANSFORM: node {ni} is skinned and has a non-identity transform, which glTF viewers ignore"
                ));
            }
            for (pi, p) in mesh.primitives.iter().enumerate() {
                if !p.attributes.contains_key("JOINTS_0") || !p.attributes.contains_key("WEIGHTS_0") {
                    r.errors.push(format!(
                        "SKIN_MESH_MISSING_ATTRIBUTES: node {ni} is skinned but mesh {} primitive {pi} lacks JOINTS_0/WEIGHTS_0",
                        node.mesh.unwrap()
                    ));
                    continue;
                }
                for n in joint_sets(p) {
                    self.check_joint_values(ni, si, skin.joints.len(), node.mesh.unwrap(), pi, p, n, r);
                }
            }
        }
    }

    /// Read `JOINTS_n` back from the buffer and check each (non-zero-weight) slot is in range.
    #[allow(clippy::too_many_arguments)]
    fn check_joint_values(&self, ni: usize, si: usize, joint_count: usize, mi: usize, pi: usize, p: &Primitive, n: usize, r: &mut ValidationReport) {
        let (Some(&ja), Some(&wa)) = (p.attributes.get(&format!("JOINTS_{n}")), p.attributes.get(&format!("WEIGHTS_{n}"))) else { return };
        let (Some(joints), weights) = (self.read_joints(ja), self.read_f32(wa)) else { return };
        for (v, slots) in joints.iter().enumerate() {
            for (k, &slot) in slots.iter().enumerate() {
                let weighted = weights.as_ref().map_or(true, |w| w.get(v * 4 + k).map_or(true, |&x| x > 0.0));
                if weighted && slot >= joint_count {
                    r.errors.push(format!(
                        "SKIN_JOINT_OUT_OF_RANGE: node {ni} (skin {si}, {joint_count} joints): mesh {mi} primitive {pi} JOINTS_{n} vertex {v} slot {k} is {slot}; \
                         JOINTS_n holds skin slots, not node indices"
                    ));
                    return;
                }
            }
        }
    }

    /// `WEIGHTS_n` of a primitive read back as `(values, units, tolerance)`: per vertex four real
    /// weights, and for normalized integer data the same weights in 1/65535 units.
    fn read_weights(&self, index: usize) -> Option<WeightData> {
        let (a, elems) = self.accessor_elements(index).filter(|(a, _)| a.sparse.is_none() && a.type_ == "VEC4")?;
        let comps = |e: &[u8], size: usize| -> [u32; 4] {
            std::array::from_fn(|k| match size {
                1 => u32::from(e[k]),
                _ => u32::from(u16::from_le_bytes([e[2 * k], e[2 * k + 1]])),
            })
        };
        match (a.component_type, a.normalized == Some(true)) {
            (5126, _) => Some(WeightData {
                values: elems.map(|e| std::array::from_fn(|k| f64::from(f32::from_le_bytes([e[4 * k], e[4 * k + 1], e[4 * k + 2], e[4 * k + 3]])))).collect(),
                units: None,
                step: 0,
            }),
            (5121, true) => {
                let raw: Vec<[u32; 4]> = elems.map(|e| comps(e, 1)).collect();
                Some(WeightData { values: raw.iter().map(|w| w.map(|x| f64::from(x) / 255.0)).collect(), units: Some(raw.iter().map(|w| w.map(|x| x * 257)).collect()), step: 257 })
            }
            (5123, true) => {
                let raw: Vec<[u32; 4]> = elems.map(|e| comps(e, 2)).collect();
                Some(WeightData { values: raw.iter().map(|w| w.map(|x| f64::from(x) / 65535.0)).collect(), units: Some(raw), step: 1 })
            }
            _ => None,
        }
    }

    /// Weight and joint data rules that hold for any primitive, skinned node or not: weights are
    /// non-negative and sum to 1 per vertex over all sets, no joint is weighted twice on a vertex,
    /// and zero-weight slots point at joint 0.
    fn check_skin_attributes(&self, mi: usize, pi: usize, p: &Primitive, r: &mut ValidationReport) {
        let sets = joint_sets(p);
        let at = format!("mesh {mi} primitive {pi}");
        let mut weights: Vec<(usize, WeightData)> = Vec::new();
        let mut joints: Vec<(usize, Vec<[usize; 4]>)> = Vec::new();
        for &n in &sets {
            if let Some(w) = p.attributes.get(&format!("WEIGHTS_{n}")).and_then(|&a| self.read_weights(a)) {
                if let Some((v, k)) = w.values.iter().enumerate().find_map(|(v, ws)| ws.iter().position(|&x| x < 0.0).map(|k| (v, k))) {
                    r.errors.push(format!("SKIN_WEIGHTS_NEGATIVE: {at} WEIGHTS_{n} vertex {v} slot {k} is {}", w.values[v][k]));
                }
                if let Some(j) = p.attributes.get(&format!("JOINTS_{n}")).and_then(|&a| self.read_joints(a)) {
                    if let Some((v, k)) = j.iter().zip(&w.values).enumerate().find_map(|(v, (js, ws))| (0..4).find(|&k| ws[k] == 0.0 && js[k] != 0).map(|k| (v, k))) {
                        r.warnings.push(format!(
                            "SKIN_JOINT_ZERO_WEIGHT_INDEX: {at} JOINTS_{n} vertex {v} slot {k} is {} but its weight is 0; unused slots should use joint 0",
                            j[v][k]
                        ));
                    }
                    joints.push((n, j));
                }
                weights.push((n, w));
            }
        }
        // Everything below needs every set readable and the same vertex count.
        if weights.len() != sets.len() || joints.len() != sets.len() {
            return;
        }
        let vertices = weights.first().map_or(0, |(_, w)| w.values.len());
        if weights.iter().any(|(_, w)| w.values.len() != vertices) || joints.iter().any(|(_, j)| j.len() != vertices) {
            return;
        }
        let all_integer = weights.iter().all(|(_, w)| w.units.is_some());
        // One rounding step of the coarsest integer type present (257 units for u8, 1 for u16).
        let step = weights.iter().map(|(_, w)| w.step).max().unwrap_or(1);
        for v in 0..vertices {
            let sum: f64 = weights.iter().flat_map(|(_, w)| w.values[v]).sum();
            // Integer data must hit 1 to within one rounding step of its coarsest type; floats within 1e-5.
            let off = if all_integer {
                let units: u64 = weights.iter().flat_map(|(_, w)| w.units.as_ref().unwrap()[v]).map(u64::from).sum();
                units.abs_diff(65535) > u64::from(step)
            } else {
                !((sum - 1.0).abs() <= 1e-5)
            };
            if off {
                r.errors.push(format!("SKIN_WEIGHTS_SUM: {at} vertex {v} weights sum to {sum}, expected 1"));
                break;
            }
        }
        for v in 0..vertices {
            let mut seen: Vec<usize> = Vec::new();
            for ((_, j), (_, w)) in joints.iter().zip(&weights) {
                for k in 0..4 {
                    if w.values[v][k] > 0.0 {
                        if seen.contains(&j[v][k]) {
                            r.errors.push(format!("SKIN_VERTEX_JOINT_DUPLICATE: {at} vertex {v} weights joint slot {} more than once", j[v][k]));
                            return;
                        }
                        seen.push(j[v][k]);
                    }
                }
            }
        }
    }

    /// Iterate the elements of accessor `index`, honouring the view's `byteStride`. Each item is
    /// one element's bytes. `None` if the accessor, its view or its range is invalid.
    fn accessor_elements(&self, index: usize) -> Option<(&Accessor, impl Iterator<Item = &[u8]>)> {
        let a = self.accessors.get(index)?;
        let view = self.b.gltf.buffer_views.as_ref()?.get(a.buffer_view?)?;
        let elem = element_size(a.component_type, &a.type_)?;
        let start = view.byte_offset.checked_add(a.byte_offset.unwrap_or(0))?;
        let stride = view.byte_stride.unwrap_or(elem);
        // A stride below the element size is invalid; with stride 0 a huge `count` would pass the
        // range check below and make the readers allocate without bound.
        if stride < elem {
            return None;
        }
        let end = stride.checked_mul(a.count.checked_sub(1)?)?.checked_add(elem)?.checked_add(start)?;
        if end > view.byte_offset.checked_add(view.byte_length)? {
            return None;
        }
        let data = self.b.buffer_data.get(view.buffer)?.get(start..end)?;
        Some((a, (0..a.count).map(move |i| &data[i * stride..i * stride + elem])))
    }

    fn read_joints(&self, index: usize) -> Option<Vec<[usize; 4]>> {
        let (a, elems) = self.accessor_elements(index)?;
        if a.type_ != "VEC4" {
            return None;
        }
        match a.component_type {
            5121 => Some(elems.map(|e| [e[0] as usize, e[1] as usize, e[2] as usize, e[3] as usize]).collect()),
            5123 => Some(elems.map(|e| std::array::from_fn(|k| u16::from_le_bytes([e[2 * k], e[2 * k + 1]]) as usize)).collect()),
            _ => None,
        }
    }

    fn read_f32(&self, index: usize) -> Option<Vec<f32>> {
        let (a, elems) = self.accessor_elements(index)?;
        (a.component_type == 5126)
            .then(|| elems.flat_map(|e| e.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))).collect())
    }

    /// Stored keyframe times: non-negative and strictly increasing. Non-finite times are left to
    /// `ACCESSOR_DATA_NONFINITE`; a wrong `min`/`max` to `ACCESSOR_BOUNDS_MISMATCH`.
    fn check_times(&self, ai: usize, si: usize, input: usize, r: &mut ValidationReport) {
        if self.accessors.get(input).is_none_or(|a| a.sparse.is_some() || a.type_ != "SCALAR") {
            return;
        }
        let Some(t) = self.read_f32(input).filter(|t| t.iter().all(|v| v.is_finite())) else { return };
        if let Some(k) = t.iter().position(|&v| v < 0.0) {
            r.errors.push(format!("ANIMATION_INPUT_NEGATIVE: animation {ai} sampler {si} input accessor {input} keyframe {k} time {} is negative", t[k]));
        }
        if let Some(k) = (1..t.len()).find(|&k| t[k] <= t[k - 1]) {
            r.errors.push(format!(
                "ANIMATION_INPUT_NOT_INCREASING: animation {ai} sampler {si} input accessor {} keyframe {k} time {} is not greater than the previous {}",
                input, t[k], t[k - 1]
            ));
        }
    }

    fn check_animations(&self, r: &mut ValidationReport) {
        for (ai, anim) in self.b.gltf.animations.iter().flatten().enumerate() {
            let samplers = anim.samplers.as_deref().unwrap_or(&[]);
            let channels = anim.channels.as_deref().unwrap_or(&[]);
            if channels.is_empty() || samplers.is_empty() {
                r.errors.push(format!("ANIMATION_EMPTY: animation {ai} needs at least one channel and one sampler"));
            }
            for (si, s) in samplers.iter().enumerate() {
                for (what, acc) in [("input", s.input), ("output", s.output)] {
                    if acc >= self.accessors.len() {
                        r.errors.push(format!("ANIMATION_SAMPLER_ACCESSOR_INVALID: animation {ai} sampler {si} {what} references accessor {acc}"));
                    }
                }
                if let Some(i) = s.interpolation.as_deref() {
                    if interpolation_from_str(i).is_none() {
                        r.errors.push(format!("ANIMATION_INTERPOLATION_INVALID: animation {ai} sampler {si} interpolation {i:?} is not LINEAR, STEP or CUBICSPLINE"));
                    }
                }
                if let Some(input) = self.accessors.get(s.input) {
                    if input.type_ != "SCALAR" || input.component_type != 5126 {
                        r.errors.push(format!(
                            "ANIMATION_INPUT_TYPE: animation {ai} sampler {si} input accessor {} must be FLOAT SCALAR keyframe times", s.input
                        ));
                    }
                    if input.min.is_none() || input.max.is_none() {
                        r.errors.push(format!("ANIMATION_INPUT_NO_BOUNDS: animation {ai} sampler {si} input accessor {} lacks min/max", s.input));
                    }
                    self.check_times(ai, si, s.input, r);
                }
            }
            let mut seen: std::collections::HashMap<(usize, &str), usize> = std::collections::HashMap::new();
            for (ci, c) in channels.iter().enumerate() {
                if let Some(first) = seen.insert((c.target.node, c.target.path.as_str()), ci) {
                    // Keep the first channel as the reference for later duplicates too.
                    seen.insert((c.target.node, c.target.path.as_str()), first);
                    r.errors.push(format!(
                        "ANIMATION_CHANNEL_DUPLICATE: animation {ai} channel {ci} animates the {} of node {}, like channel {first}", c.target.path, c.target.node
                    ));
                }
                if c.sampler >= samplers.len() {
                    r.errors.push(format!("ANIMATION_CHANNEL_SAMPLER_INVALID: animation {ai} channel {ci} references sampler {}", c.sampler));
                }
                if !ANIMATION_PATHS.contains(&c.target.path.as_str()) {
                    r.errors.push(format!("ANIMATION_CHANNEL_PATH_INVALID: animation {ai} channel {ci} path {:?}", c.target.path));
                }
                match self.nodes.get(c.target.node) {
                    None => r.errors.push(format!("ANIMATION_CHANNEL_NODE_INVALID: animation {ai} channel {ci} targets node {}", c.target.node)),
                    Some(n) if n.matrix.is_some() && c.target.path != "weights" => r.errors.push(format!(
                        "ANIMATION_CHANNEL_NODE_MATRIX: animation {ai} channel {ci} targets node {}, which has a matrix", c.target.node
                    )),
                    _ => {}
                }
                self.check_channel_output(ai, ci, c, samplers, r);
            }
        }
    }
}

/// Rough upper bound on vertex attributes a renderer can bind (the WebGL minimum is 16).
/// Used only for the `MORPH_ATTRIBUTE_BUDGET` warning.
const ATTRIBUTE_BUDGET: usize = 16;

impl<'a> Doc<'a> {
    /// Morph targets, default weights, target names and node weights.
    fn check_morphs(&self, r: &mut ValidationReport) {
        for (mi, mesh) in self.meshes.iter().enumerate() {
            let counts: Vec<usize> = mesh.primitives.iter().map(|p| p.targets.as_ref().map_or(0, |t| t.len())).collect();
            // Number of targets the mesh is expected to have: that of its first primitive with targets.
            let m = counts.iter().copied().find(|&c| c > 0).unwrap_or(0);

            for (pi, p) in mesh.primitives.iter().enumerate() {
                let Some(targets) = &p.targets else { continue };
                if targets.is_empty() {
                    r.errors.push(format!("MORPH_TARGETS_EMPTY: mesh {mi} primitive {pi} has an empty targets array"));
                    continue;
                }
                if counts[pi] != m && m > 0 {
                    r.errors.push(format!(
                        "MORPH_TARGETS_INCONSISTENT: mesh {mi} primitive {pi} has {} morph targets but another primitive has {m}", counts[pi]
                    ));
                }
                let mut budget = p.attributes.len();
                for (ti, target) in targets.iter().enumerate() {
                    budget += target.len();
                    if target.is_empty() {
                        r.errors.push(format!("MORPH_TARGET_EMPTY: mesh {mi} primitive {pi} target {ti} has no attributes"));
                    }
                    for (name, &acc) in target {
                        self.check_target_attribute(mi, pi, ti, name, acc, p, r);
                    }
                }
                if budget > ATTRIBUTE_BUDGET {
                    r.warnings.push(format!(
                        "MORPH_ATTRIBUTE_BUDGET: mesh {mi} primitive {pi} uses {budget} vertex attributes including morph targets; \
                         many renderers bind at most {ATTRIBUTE_BUDGET}"
                    ));
                }
            }

            if let Some(w) = &mesh.weights {
                if w.is_empty() {
                    r.errors.push(format!("MESH_WEIGHTS_EMPTY: mesh {mi} has an empty weights array"));
                } else if w.len() != m {
                    r.errors.push(format!("MESH_WEIGHTS_COUNT: mesh {mi} has {} weights but {m} morph targets", w.len()));
                }
                if let Some(i) = w.iter().position(|v| !v.is_finite()) {
                    r.errors.push(format!("MESH_WEIGHTS_NOT_FINITE: mesh {mi} weight {i} is not finite"));
                }
            }
            if let Some(names) = mesh.extras.as_ref().and_then(|e| e.get("targetNames")) {
                match names.as_array().filter(|a| a.iter().all(|n| n.is_string())) {
                    None => r.errors.push(format!("MESH_TARGET_NAMES_INVALID: mesh {mi} extras.targetNames must be an array of strings")),
                    Some(a) => {
                        if a.len() != m {
                            r.errors.push(format!("MESH_TARGET_NAMES_COUNT: mesh {mi} has {} target names but {m} morph targets", a.len()));
                        }
                        let mut seen = std::collections::HashSet::new();
                        if let Some(dup) = a.iter().filter_map(|n| n.as_str()).find(|n| !seen.insert(*n)) {
                            r.warnings.push(format!("MESH_TARGET_NAMES_DUPLICATE: mesh {mi} uses the target name {dup:?} more than once"));
                        }
                    }
                }
            }
        }

        for (ni, node) in self.nodes.iter().enumerate() {
            let Some(w) = &node.weights else { continue };
            let Some(mesh_index) = node.mesh else {
                r.errors.push(format!("NODE_WEIGHTS_WITHOUT_MESH: node {ni} has weights but no mesh"));
                continue;
            };
            // A dangling mesh index is reported as NODE_MESH_INVALID elsewhere.
            let Some(mesh) = self.meshes.get(mesh_index) else { continue };
            let m = mesh.primitives.iter().map(|p| p.targets.as_ref().map_or(0, |t| t.len())).find(|&c| c > 0).unwrap_or(0);
            if w.len() != m {
                r.errors.push(format!("NODE_WEIGHTS_COUNT: node {ni} has {} weights but its mesh {mesh_index} has {m} morph targets", w.len()));
            }
            if let Some(i) = w.iter().position(|v| !v.is_finite()) {
                r.errors.push(format!("NODE_WEIGHTS_NOT_FINITE: node {ni} weight {i} is not finite"));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_target_attribute(&self, mi: usize, pi: usize, ti: usize, name: &str, acc: usize, base: &Primitive, r: &mut ValidationReport) {
        let at = format!("mesh {mi} primitive {pi} target {ti} {name}");
        if !matches!(name, "POSITION" | "NORMAL" | "TANGENT") && !name.starts_with('_') {
            r.errors.push(format!("MORPH_TARGET_ATTRIBUTE_INVALID: {at} is not POSITION, NORMAL, TANGENT or an application-specific `_` attribute"));
        }
        let Some(&base_acc) = base.attributes.get(name) else {
            r.errors.push(format!("MORPH_TARGET_ATTRIBUTE_NOT_IN_BASE: {at}: the primitive has no base {name} attribute"));
            return;
        };
        let Some(a) = self.accessors.get(acc) else {
            r.errors.push(format!("MORPH_TARGET_ACCESSOR_INVALID: {at} references accessor {acc}"));
            return;
        };
        if matches!(name, "POSITION" | "NORMAL" | "TANGENT") && (a.type_ != "VEC3" || a.component_type != 5126) {
            r.errors.push(format!(
                "MORPH_TARGET_TYPE: {at} accessor {acc} must be FLOAT VEC3, found {} with component type {}{} (normalized-integer deltas need KHR_mesh_quantization, which is not supported)", a.type_, a.component_type, if a.normalized == Some(true) { " normalized" } else { "" }
            ));
        }
        if let Some(b) = self.accessors.get(base_acc) {
            if a.count != b.count {
                r.errors.push(format!(
                    "MORPH_TARGET_COUNT_MISMATCH: {at} accessor {acc} has {} elements but the base {name} accessor {base_acc} has {}", a.count, b.count
                ));
            }
        }
        if name == "POSITION" && (a.min.is_none() || a.max.is_none()) {
            r.warnings.push(format!("MORPH_TARGET_POSITION_NO_BOUNDS: {at} accessor {acc} lacks min/max"));
        }
    }
}

/// Decoded `WEIGHTS_n` data: real weights per vertex, plus 1/65535 units for normalized integers.
struct WeightData {
    values: Vec<[f64; 4]>,
    units: Option<Vec<[u32; 4]>>,
    step: u32,
}

/// Attribute set numbers `n` for which the primitive has `JOINTS_n`.
fn joint_sets(p: &Primitive) -> Vec<usize> {
    let mut v: Vec<usize> = p.attributes.keys().filter_map(|k| k.strip_prefix("JOINTS_")?.parse().ok()).collect();
    v.sort_unstable();
    v
}

/// Topmost ancestor of `node`, or `None` if the parent chain loops.
fn root_of(parents: &[Option<usize>], node: usize) -> Option<usize> {
    let mut cur = node;
    for _ in 0..=parents.len() {
        match parents[cur] {
            None => return Some(cur),
            Some(p) => cur = p,
        }
    }
    None
}

fn is_ancestor_or_self(parents: &[Option<usize>], ancestor: usize, node: usize) -> bool {
    let mut cur = node;
    for _ in 0..=parents.len() {
        if cur == ancestor {
            return true;
        }
        match parents[cur] {
            None => return false,
            Some(p) => cur = p,
        }
    }
    false
}

#[cfg(test)]
mod hardening_tests;

/// Read a non-sparse FLOAT accessor from the builder's stored bytes (`None` if it is sparse,
/// not FLOAT, or its view is invalid). Used by the builder to check stored keyframe times.
pub(crate) fn read_f32_accessor(b: &GltfBuilder, index: usize) -> Option<Vec<f32>> {
    let doc = Doc::new(b);
    doc.accessors.get(index).filter(|a| a.sparse.is_none() && a.type_ == "SCALAR")?;
    doc.read_f32(index)
}
