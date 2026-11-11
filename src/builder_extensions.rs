//! Extensions, extras, `extensionsUsed`/`extensionsRequired` bookkeeping and asset metadata.

use serde_json::Value;

use crate::builder::GltfBuilder;
use crate::error::{GltfError, Result};
use crate::models::{Extensions, PbrSpecularGlossiness};

/// The glTF object an extension payload or `extras` value is attached to.
///
/// Indices are checked when the target is used; an out-of-range index is an error.
/// Texture-info structs (`baseColorTexture` etc.) are not addressable yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtensionTarget {
    /// The document root.
    Root,
    /// `asset`.
    Asset,
    Scene(usize),
    Node(usize),
    Mesh(usize),
    Primitive { mesh: usize, primitive: usize },
    Material(usize),
    /// `materials[i].pbrMetallicRoughness`; the material must have one.
    PbrMetallicRoughness(usize),
    Texture(usize),
    Image(usize),
    Sampler(usize),
    Accessor(usize),
    BufferView(usize),
    Buffer(usize),
    Skin(usize),
    Animation(usize),
    AnimationChannel { animation: usize, channel: usize },
    AnimationSampler { animation: usize, sampler: usize },
    Camera(usize),
    /// `cameras[i].perspective`; the camera must have one.
    CameraPerspective(usize),
    /// `cameras[i].orthographic`; the camera must have one.
    CameraOrthographic(usize),
}

fn invalid(msg: String) -> GltfError {
    GltfError::InvalidInput(msg)
}

/// Mutable access to the `extensions` and `extras` slots of one object. Materials keep
/// their extensions in a typed struct, so they are handled separately.
enum Slots<'a> {
    Plain(&'a mut Option<Extensions>, &'a mut Option<Value>),
    Material(&'a mut crate::models::Material),
}

macro_rules! item {
    ($opt:expr, $i:expr, $kind:literal) => {{
        let len = $opt.as_ref().map_or(0, |v| v.len());
        $opt.as_mut().and_then(|v| v.get_mut($i)).ok_or_else(|| {
            invalid(format!(concat!($kind, " {} does not exist ({} present)"), $i, len))
        })?
    }};
}

macro_rules! plain {
    ($o:expr) => {{
        let o = $o;
        Slots::Plain(&mut o.extensions, &mut o.extras)
    }};
}

impl GltfBuilder {
    fn slots(&mut self, target: ExtensionTarget) -> Result<Slots<'_>> {
        use ExtensionTarget as T;
        let g = &mut self.gltf;
        Ok(match target {
            T::Root => Slots::Plain(&mut g.extensions, &mut g.extras),
            T::Asset => plain!(&mut g.asset),
            T::Scene(i) => plain!(item!(g.scenes, i, "scene")),
            T::Node(i) => plain!(item!(g.nodes, i, "node")),
            T::Mesh(i) => plain!(item!(g.meshes, i, "mesh")),
            T::Primitive { mesh, primitive } => {
                let m = item!(g.meshes, mesh, "mesh");
                let n = m.primitives.len();
                let p = m.primitives.get_mut(primitive).ok_or_else(|| {
                    invalid(format!("primitive {primitive} of mesh {mesh} does not exist ({n} present)"))
                })?;
                plain!(p)
            }
            T::Material(i) => Slots::Material(item!(g.materials, i, "material")),
            T::PbrMetallicRoughness(i) => {
                let m = item!(g.materials, i, "material");
                let pbr = m
                    .pbr_metallic_roughness
                    .as_mut()
                    .ok_or_else(|| invalid(format!("material {i} has no pbrMetallicRoughness")))?;
                plain!(pbr)
            }
            T::Texture(i) => plain!(item!(g.textures, i, "texture")),
            T::Image(i) => plain!(item!(g.images, i, "image")),
            T::Sampler(i) => plain!(item!(g.samplers, i, "sampler")),
            T::Accessor(i) => plain!(item!(g.accessors, i, "accessor")),
            T::BufferView(i) => plain!(item!(g.buffer_views, i, "buffer view")),
            T::Buffer(i) => plain!(item!(g.buffers, i, "buffer")),
            T::Skin(i) => plain!(item!(g.skins, i, "skin")),
            T::Animation(i) => plain!(item!(g.animations, i, "animation")),
            T::Camera(i) => plain!(item!(g.cameras, i, "camera")),
            T::CameraPerspective(i) => {
                let c = item!(g.cameras, i, "camera");
                plain!(c.perspective.as_mut().ok_or_else(|| invalid(format!("camera {i} has no perspective")))?)
            }
            T::CameraOrthographic(i) => {
                let c = item!(g.cameras, i, "camera");
                plain!(c.orthographic.as_mut().ok_or_else(|| invalid(format!("camera {i} has no orthographic")))?)
            }
            T::AnimationChannel { animation, channel } => {
                plain!(item!(item!(g.animations, animation, "animation").channels, channel, "channel"))
            }
            T::AnimationSampler { animation, sampler } => {
                plain!(item!(item!(g.animations, animation, "animation").samplers, sampler, "animation sampler"))
            }
        })
    }

    /// Set (or replace) the payload of extension `name` on `target`, and add the name to
    /// `extensionsUsed`.
    ///
    /// The payload is opaque JSON and must be an object; it is not otherwise validated.
    /// `KHR_materials_pbrSpecularGlossiness` on a material must deserialize into the typed
    /// specular-glossiness struct. Use [`require_extension`](Self::require_extension) as
    /// well if loaders must understand the extension.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` for an empty name, a non-object payload, or a target that does
    /// not exist. The document is unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::{GltfBuilder, ExtensionTarget};
    /// use serde_json::json;
    ///
    /// let mut b = GltfBuilder::new();
    /// let node = b.add_node(None, None, None, None, None);
    /// b.set_extension(ExtensionTarget::Node(node), "VENDOR_note", json!({ "a": 1 })).unwrap();
    /// assert!(b.gltf.extensions_used.as_ref().unwrap().contains(&"VENDOR_note".to_string()));
    ///
    /// // Errors leave the document unchanged.
    /// assert!(b.set_extension(ExtensionTarget::Node(9), "VENDOR_note", json!({})).is_err());
    /// assert!(b.set_extension(ExtensionTarget::Node(node), "VENDOR_note", json!(3)).is_err());
    /// ```
    pub fn set_extension(&mut self, target: ExtensionTarget, name: &str, value: Value) -> Result<()> {
        check_name(name)?;
        if !value.is_object() {
            return Err(invalid(format!("payload of extension {name} must be a JSON object")));
        }
        match self.slots(target)? {
            Slots::Plain(ext, _) => {
                ext.get_or_insert_with(Default::default).insert(name.to_string(), value);
            }
            Slots::Material(m) => {
                if name == SPEC_GLOSS {
                    // Parse before touching the material so a bad payload changes nothing.
                    let sg: PbrSpecularGlossiness = serde_json::from_value(value)
                        .map_err(|e| invalid(format!("invalid {SPEC_GLOSS} payload: {e}")))?;
                    m.extensions.get_or_insert_with(Default::default).pbr_specular_glossiness = Some(sg);
                } else {
                    m.extensions.get_or_insert_with(Default::default).other.insert(name.to_string(), value);
                }
            }
        }
        self.register_extension(name);
        Ok(())
    }

    /// Set (replace) the `extras` of `target`. Any JSON value is allowed.
    ///
    /// On a mesh that already has morph target names, the names are kept under
    /// `extras.targetNames`: `value` must then be an object without a `targetNames` key and is
    /// merged with them. (See also [`set_mesh_target_names`](Self::set_mesh_target_names).)
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the target does not exist, or on the `targetNames` clash
    /// described above. The document is unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::{GltfBuilder, ExtensionTarget};
    /// use serde_json::json;
    ///
    /// let mut b = GltfBuilder::new();
    /// b.set_extras(ExtensionTarget::Root, json!([1, 2, 3])).unwrap();
    /// assert_eq!(b.gltf.extras, Some(json!([1, 2, 3])));
    /// assert!(b.set_extras(ExtensionTarget::Mesh(0), json!({})).is_err());
    /// ```
    pub fn set_extras(&mut self, target: ExtensionTarget, value: Value) -> Result<()> {
        let is_mesh = matches!(target, ExtensionTarget::Mesh(_));
        let extras = match self.slots(target)? {
            Slots::Plain(_, extras) => extras,
            Slots::Material(m) => &mut m.extras,
        };
        let kept = if is_mesh { extras.as_ref().and_then(|e| e.get(TARGET_NAMES)).cloned() } else { None };
        match kept {
            None => *extras = Some(value),
            Some(names) => {
                let Value::Object(mut obj) = value else {
                    return Err(invalid("mesh extras must be an object: the mesh has morph target names".to_string()));
                };
                if obj.contains_key(TARGET_NAMES) {
                    return Err(invalid(format!(
                        "extras contain {TARGET_NAMES}, which the mesh already has; use set_mesh_target_names"
                    )));
                }
                obj.insert(TARGET_NAMES.to_string(), names);
                *extras = Some(Value::Object(obj));
            }
        }
        Ok(())
    }

    /// Add `name` to `extensionsUsed` (idempotent). Use this for extensions that have no
    /// payload object, such as `KHR_mesh_quantization`; [`validate`](Self::validate) warns
    /// about declared extensions it finds no payload for.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` for an empty name.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// b.declare_extension_used("KHR_mesh_quantization").unwrap();
    /// b.declare_extension_used("KHR_mesh_quantization").unwrap(); // no duplicate
    /// assert_eq!(b.gltf.extensions_used.as_ref().unwrap().len(), 1);
    /// assert!(b.declare_extension_used("").is_err());
    /// ```
    pub fn declare_extension_used(&mut self, name: &str) -> Result<()> {
        check_name(name)?;
        self.register_extension(name);
        Ok(())
    }

    /// Add `name` to `extensionsRequired` (and to `extensionsUsed`, which must be a superset).
    /// Idempotent. A loader that does not support a required extension must refuse the file.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` for an empty name.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// b.require_extension("KHR_draco_mesh_compression").unwrap();
    /// assert_eq!(b.gltf.extensions_required, b.gltf.extensions_used);
    /// assert!(b.require_extension("").is_err());
    /// ```
    pub fn require_extension(&mut self, name: &str) -> Result<()> {
        check_name(name)?;
        self.register_extension(name);
        push_unique(&mut self.gltf.extensions_required, name);
        Ok(())
    }

    /// Add `name` to `extensionsUsed` unless already present.
    pub(crate) fn register_extension(&mut self, name: &str) {
        push_unique(&mut self.gltf.extensions_used, name);
    }

    /// Set `mesh.extras.targetNames` on an existing morphable mesh, replacing earlier names
    /// and keeping the other extras. Complements `add_morphable_mesh`, which sets them at
    /// creation.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the mesh does not exist, has no morph targets, `names.len()`
    /// differs from the target count, or its extras are not a JSON object. The document is
    /// unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// let plain = b.create_box(1.0); // no morph targets
    /// assert!(b.set_mesh_target_names(plain, vec!["a".into()]).is_err());
    /// ```
    pub fn set_mesh_target_names(&mut self, mesh: usize, names: Vec<String>) -> Result<()> {
        let count = self.gltf.meshes.as_ref().map_or(0, |m| m.len());
        let m = self
            .gltf
            .meshes
            .as_mut()
            .and_then(|m| m.get_mut(mesh))
            .ok_or_else(|| invalid(format!("mesh {mesh} does not exist ({count} present)")))?;
        let n = crate::builder_morph::morph_target_count(&m.primitives)?;
        crate::builder_morph::validate_weights_and_names(n, None, Some(&names))?;
        merge_target_names(&mut m.extras, names)
    }

    /// Set `asset.copyright`.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` for an empty string.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// b.set_copyright("(c) Example").unwrap();
    /// assert_eq!(b.gltf.asset.copyright.as_deref(), Some("(c) Example"));
    /// assert!(b.set_copyright("").is_err());
    /// ```
    pub fn set_copyright(&mut self, copyright: &str) -> Result<()> {
        if copyright.is_empty() {
            return Err(invalid("copyright must not be empty".to_string()));
        }
        self.gltf.asset.copyright = Some(copyright.to_string());
        Ok(())
    }

    /// Set `asset.generator` (default `"Rust glTF Export Library"`).
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` for an empty string (use
    /// [`clear_generator`](Self::clear_generator) to omit it).
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// b.set_generator("my tool 1.0").unwrap();
    /// b.clear_generator();
    /// assert!(b.gltf.asset.generator.is_none());
    /// assert!(b.set_generator("").is_err());
    /// ```
    pub fn set_generator(&mut self, generator: &str) -> Result<()> {
        if generator.is_empty() {
            return Err(invalid("generator must not be empty; use clear_generator".to_string()));
        }
        self.gltf.asset.generator = Some(generator.to_string());
        Ok(())
    }

    /// Omit `asset.generator` from the output.
    pub fn clear_generator(&mut self) {
        self.gltf.asset.generator = None;
    }

    /// Set `asset.minVersion` (`major.minor`, not greater than the asset version `2.0`).
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the string is not `<digits>.<digits>` or exceeds the asset
    /// version.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// b.set_min_version("2.0").unwrap();
    /// assert!(b.set_min_version("2.1").is_err()); // newer than the asset
    /// assert!(b.set_min_version("two").is_err());
    /// ```
    pub fn set_min_version(&mut self, min_version: &str) -> Result<()> {
        let min = parse_version(min_version)
            .ok_or_else(|| invalid(format!("minVersion {min_version:?} is not <major>.<minor>")))?;
        let version = parse_version(&self.gltf.asset.version)
            .ok_or_else(|| invalid(format!("asset version {:?} is not <major>.<minor>", self.gltf.asset.version)))?;
        if min > version {
            return Err(invalid(format!(
                "minVersion {min_version} exceeds asset version {}",
                self.gltf.asset.version
            )));
        }
        self.gltf.asset.min_version = Some(min_version.to_string());
        Ok(())
    }
}

pub(crate) const SPEC_GLOSS: &str = "KHR_materials_pbrSpecularGlossiness";
const TARGET_NAMES: &str = "targetNames";

fn check_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(invalid("extension name must not be empty".to_string()));
    }
    Ok(())
}

fn push_unique(list: &mut Option<Vec<String>>, name: &str) {
    let v = list.get_or_insert_with(Vec::new);
    if !v.iter().any(|n| n == name) {
        v.push(name.to_string());
    }
}

/// Parse `<digits>.<digits>` into `(major, minor)`.
pub(crate) fn parse_version(s: &str) -> Option<(u64, u64)> {
    let (a, b) = s.split_once('.')?;
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit());
    if !digits(a) || !digits(b) {
        return None;
    }
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// Store `names` under `extras.targetNames`, keeping other keys. Errors (leaving `extras`
/// untouched) if `extras` is not an object.
pub(crate) fn merge_target_names(extras: &mut Option<Value>, names: Vec<String>) -> Result<()> {
    let names = Value::from(names);
    match extras {
        None => *extras = Some(serde_json::json!({ TARGET_NAMES: names })),
        Some(Value::Object(o)) => {
            o.insert(TARGET_NAMES.to_string(), names);
        }
        Some(_) => return Err(invalid("mesh extras are not an object, so targetNames cannot be stored".to_string())),
    }
    Ok(())
}
