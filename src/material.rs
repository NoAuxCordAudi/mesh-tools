//! # Material Creation and Management
//!
//! This module provides utilities for creating and configuring materials in the glTF format.
//! Materials in glTF use the Physically Based Rendering (PBR) model which simulates how light
//! interacts with surfaces in a physically accurate way.
//!
//! The core of this module is the `MaterialBuilder` struct which provides a builder pattern
//! for creating materials with various properties:
//!
//! - Base color (diffuse color)
//! - Metallic and roughness factors
//! - Normal, occlusion, and emissive textures
//! - Transparency settings
//! - Double-sided rendering
//!
//! ## Example
//!
//! ```rust
//! use mesh_tools::material;
//!
//! // Create a red metallic material
//! let red_metal = material::create_metallic_material(
//!     Some("RedMetal".to_string()),
//!     [1.0, 0.0, 0.0, 1.0], // Red color
//!     0.9, // High metallic
//!     0.2  // Low roughness (shiny)
//! );
//! ```

use crate::error::{GltfError, Result};
use crate::models::{
    Material, MaterialExtensions, NormalTextureInfo, OcclusionTextureInfo, PbrMetallicRoughness, PbrSpecularGlossiness,
    TextureInfo,
};

/// A reference to a texture from a material slot: the texture index and the `TEXCOORD_n` set
/// that samples it. A bare `usize` converts with set 0.
///
/// ```
/// use mesh_tools::TextureRef;
/// assert_eq!(TextureRef::from(3), TextureRef { texture: 3, tex_coord: 0 });
/// assert_eq!(TextureRef::new(3).with_tex_coord(1).tex_coord, 1);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureRef {
    /// Index into the document's textures.
    pub texture: usize,
    /// Texture coordinate set (`TEXCOORD_n`); 0 is the default and is omitted from the JSON.
    pub tex_coord: u32,
}

impl TextureRef {
    /// Reference `texture` with texture coordinate set 0.
    pub fn new(texture: usize) -> Self {
        Self { texture, tex_coord: 0 }
    }

    /// Sample with `TEXCOORD_{tex_coord}` instead of set 0.
    pub fn with_tex_coord(mut self, tex_coord: u32) -> Self {
        self.tex_coord = tex_coord;
        self
    }

    /// The model's `texCoord` field: omitted when 0 (the spec default).
    fn model_tex_coord(self) -> Option<usize> {
        (self.tex_coord != 0).then_some(self.tex_coord as usize)
    }
}

impl From<usize> for TextureRef {
    fn from(texture: usize) -> Self {
        Self::new(texture)
    }
}

/// How the alpha value of the base color is interpreted (glTF `alphaMode`).
///
/// ```
/// use mesh_tools::{material::MaterialBuilder, AlphaMode};
/// let m = MaterialBuilder::new(None).with_alpha(AlphaMode::Mask { cutoff: 0.3 }).try_build().unwrap();
/// assert_eq!(m.alpha_mode.as_deref(), Some("MASK"));
/// assert_eq!(m.alpha_cutoff, Some(0.3));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlphaMode {
    /// Alpha is ignored.
    Opaque,
    /// Fragments with alpha below `cutoff` are discarded (spec default cutoff: 0.5).
    Mask { cutoff: f32 },
    /// Alpha is composited.
    Blend,
}

impl AlphaMode {
    /// The glTF string: `OPAQUE`, `MASK` or `BLEND`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Opaque => "OPAQUE",
            Self::Mask { .. } => "MASK",
            Self::Blend => "BLEND",
        }
    }
}

/// Specular-glossiness workflow of `KHR_materials_pbrSpecularGlossiness` (deprecated upstream;
/// superseded by `KHR_materials_specular` + `KHR_materials_ior`, which this crate does not write).
///
/// Set it with [`MaterialBuilder::with_specular_glossiness`]. Absent factors take the spec
/// defaults (diffuse `[1,1,1,1]`, specular `[1,1,1]`, glossiness 1). Add a metallic-roughness
/// fallback to the same material for viewers that ignore the extension.
///
/// ```
/// use mesh_tools::{material::MaterialBuilder, SpecularGlossiness, TextureRef};
/// let m = MaterialBuilder::new(None)
///     .with_base_color([0.5, 0.5, 0.5, 1.0]) // fallback
///     .with_specular_glossiness(SpecularGlossiness {
///         specular_factor: Some([0.5, 0.25, 0.125]),
///         diffuse_texture: Some(TextureRef::new(0).with_tex_coord(1)),
///         ..Default::default()
///     })
///     .try_build()
///     .unwrap();
/// assert!(m.extensions.unwrap().pbr_specular_glossiness.is_some());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpecularGlossiness {
    /// RGBA diffuse color, each component in [0, 1].
    pub diffuse_factor: Option<[f32; 4]>,
    /// Diffuse (RGB) and opacity (A) texture.
    pub diffuse_texture: Option<TextureRef>,
    /// RGB specular color, each component in [0, 1].
    pub specular_factor: Option<[f32; 3]>,
    /// Glossiness in [0, 1] (0 rough, 1 smooth).
    pub glossiness_factor: Option<f32>,
    /// Specular (RGB) and glossiness (A) texture.
    pub specular_glossiness_texture: Option<TextureRef>,
}

impl SpecularGlossiness {
    fn to_model(self) -> PbrSpecularGlossiness {
        let mut m = PbrSpecularGlossiness::default();
        m.diffuse_factor = self.diffuse_factor;
        m.diffuse_texture = self.diffuse_texture.map(MaterialBuilder::info);
        m.specular_factor = self.specular_factor;
        m.glossiness_factor = self.glossiness_factor;
        m.specular_glossiness_texture = self.specular_glossiness_texture.map(MaterialBuilder::info);
        m
    }
}

/// The core texture slots of a material as `(slot name, index, texCoord)`.
fn core_texture_slots(m: &Material) -> Vec<(&'static str, usize, usize)> {
    let pbr = m.pbr_metallic_roughness.as_ref();
    [
        ("baseColorTexture", pbr.and_then(|p| p.base_color_texture.as_ref()).map(|t| (t.index, t.tex_coord))),
        ("metallicRoughnessTexture", pbr.and_then(|p| p.metallic_roughness_texture.as_ref()).map(|t| (t.index, t.tex_coord))),
        ("normalTexture", m.normal_texture.as_ref().map(|t| (t.index, t.tex_coord))),
        ("occlusionTexture", m.occlusion_texture.as_ref().map(|t| (t.index, t.tex_coord))),
        ("emissiveTexture", m.emissive_texture.as_ref().map(|t| (t.index, t.tex_coord))),
    ]
    .into_iter()
    .filter_map(|(name, t)| t.map(|(i, c)| (name, i, c.unwrap_or(0))))
    .collect()
}

/// The specular-glossiness texture slots as `(slot name, index, texCoord)`.
fn sg_texture_slots(m: &Material) -> Vec<(&'static str, usize, usize)> {
    let Some(sg) = m.extensions.as_ref().and_then(|e| e.pbr_specular_glossiness.as_ref()) else { return Vec::new() };
    [
        ("diffuseTexture", sg.diffuse_texture.as_ref()),
        ("specularGlossinessTexture", sg.specular_glossiness_texture.as_ref()),
    ]
    .into_iter()
    .filter_map(|(name, t)| t.map(|t| (name, t.index, t.tex_coord.unwrap_or(0))))
    .collect()
}

/// All texture slots of a material (core and specular-glossiness).
pub(crate) fn texture_slots(m: &Material) -> Vec<(&'static str, usize, usize)> {
    let mut v = core_texture_slots(m);
    v.extend(sg_texture_slots(m));
    v
}

/// One finding of [`check_material`].
pub(crate) struct MaterialIssue {
    pub code: &'static str,
    pub msg: String,
    pub warning: bool,
}

/// Every material rule that needs no other part of the document, plus the texture index check
/// when `texture_count` is known. Shared by `try_build`, `add_material_from` and `validate()`.
pub(crate) fn check_material(m: &Material, texture_count: Option<usize>) -> Vec<MaterialIssue> {
    let mut out = Vec::new();
    let mut add = |code, warning, msg: String| out.push(MaterialIssue { code, msg, warning });

    // Finite and in [0, 1]
    let unit = |what: &str, vals: &[f32], code: &'static str, add: &mut dyn FnMut(&'static str, bool, String)| {
        for v in vals {
            if !v.is_finite() {
                add("MATERIAL_FACTOR_NOT_FINITE", false, format!("{what} contains {v}"));
            } else if !(0.0..=1.0).contains(v) {
                add(code, false, format!("{what} component {v} is outside [0, 1]"));
            }
        }
    };
    if let Some(p) = &m.pbr_metallic_roughness {
        if let Some(c) = &p.base_color_factor {
            unit("baseColorFactor", c, "MATERIAL_FACTOR_RANGE", &mut add);
        }
        if let Some(v) = &p.metallic_factor {
            unit("metallicFactor", &[*v], "MATERIAL_FACTOR_RANGE", &mut add);
        }
        if let Some(v) = &p.roughness_factor {
            unit("roughnessFactor", &[*v], "MATERIAL_FACTOR_RANGE", &mut add);
        }
    }
    if let Some(e) = &m.emissive_factor {
        unit("emissiveFactor", e, "MATERIAL_FACTOR_RANGE", &mut add);
    }
    if let Some(o) = m.occlusion_texture.as_ref().and_then(|o| o.strength) {
        unit("occlusionTexture strength", &[o], "MATERIAL_OCCLUSION_STRENGTH_RANGE", &mut add);
    }
    if let Some(s) = m.normal_texture.as_ref().and_then(|n| n.scale) {
        if !s.is_finite() {
            add("MATERIAL_FACTOR_NOT_FINITE", false, format!("normalTexture scale is {s}"));
        } else if s == 0.0 {
            add("MATERIAL_NORMAL_SCALE_ZERO", true, "normalTexture scale is 0, which flattens the normal map".to_string());
        }
    }

    // Alpha
    let mode = m.alpha_mode.as_deref();
    if let Some(name) = mode.filter(|n| !matches!(*n, "OPAQUE" | "MASK" | "BLEND")) {
        add("MATERIAL_ALPHA_MODE_INVALID", false, format!("alphaMode {name:?} is not OPAQUE, MASK or BLEND"));
    }
    if let Some(c) = m.alpha_cutoff {
        if !c.is_finite() || c < 0.0 {
            add("MATERIAL_ALPHA_CUTOFF_INVALID", false, format!("alphaCutoff {c} must be finite and >= 0"));
        } else if mode != Some("MASK") {
            add("MATERIAL_ALPHA_CUTOFF_UNUSED", true, format!("alphaCutoff {c} is ignored unless alphaMode is MASK"));
        }
    }

    // Specular-glossiness factors
    if let Some(sg) = m.extensions.as_ref().and_then(|e| e.pbr_specular_glossiness.as_ref()) {
        let vals = [
            ("diffuseFactor", sg.diffuse_factor.as_ref().map(|c| &c[..])),
            ("specularFactor", sg.specular_factor.as_ref().map(|c| &c[..])),
            ("glossinessFactor", sg.glossiness_factor.as_ref().map(std::slice::from_ref)),
        ];
        for (what, v) in vals {
            if let Some(v) = v {
                unit(what, v, "SG_FACTOR_RANGE", &mut add);
            }
        }
    }

    if m.extensions.as_ref().is_some_and(|e| e.pbr_specular_glossiness.is_some()) && m.pbr_metallic_roughness.is_none() {
        add(
            "SG_WITHOUT_FALLBACK",
            true,
            "specular-glossiness without a pbrMetallicRoughness fallback; viewers that ignore the extension show the default material"
                .to_string(),
        );
    }

    // Textures
    let sg_slots = sg_texture_slots(m);
    for (slot, index, tex_coord) in core_texture_slots(m).into_iter().chain(sg_slots.iter().copied()) {
        if let Some(n) = texture_count.filter(|&n| index >= n) {
            let code = if sg_slots.iter().any(|s| s.0 == slot) { "SG_TEXTURE_INVALID" } else { "MATERIAL_TEXTURE_INVALID" };
            add(code, false, format!("{slot} references texture {index} ({n} present)"));
        }
        if tex_coord >= 2 {
            add(
                "MATERIAL_TEXCOORD_RANGE",
                true,
                format!("{slot} uses texCoord {tex_coord}; clients only have to support sets 0 and 1"),
            );
        }
    }
    out
}

impl AlphaMode {
    fn apply(self, m: &mut Material) {
        m.alpha_mode = Some(self.name().to_string());
        m.alpha_cutoff = match self {
            Self::Mask { cutoff } => Some(cutoff),
            _ => None,
        };
    }
}

/// Builder for creating and configuring glTF materials with PBR properties
pub struct MaterialBuilder {
    pub material: Material,
}

impl MaterialBuilder {
    /// Create a new material builder
    pub fn new(name: Option<String>) -> Self {
        let mut material = Material::default();
        material.name = name;
        
        Self { material }
    }
    
    /// Set base color factor
    pub fn with_base_color(mut self, color: [f32; 4]) -> Self {
        if self.material.pbr_metallic_roughness.is_none() {
            self.material.pbr_metallic_roughness = Some(PbrMetallicRoughness::default());
        }
        
        if let Some(pbr) = &mut self.material.pbr_metallic_roughness {
            pbr.base_color_factor = Some(color);
        }
        
        self
    }
    
    /// Set metallic factor
    pub fn with_metallic_factor(mut self, factor: f32) -> Self {
        if self.material.pbr_metallic_roughness.is_none() {
            self.material.pbr_metallic_roughness = Some(PbrMetallicRoughness::default());
        }
        
        if let Some(pbr) = &mut self.material.pbr_metallic_roughness {
            pbr.metallic_factor = Some(factor);
        }
        
        self
    }
    
    /// Set roughness factor
    pub fn with_roughness_factor(mut self, factor: f32) -> Self {
        if self.material.pbr_metallic_roughness.is_none() {
            self.material.pbr_metallic_roughness = Some(PbrMetallicRoughness::default());
        }
        
        if let Some(pbr) = &mut self.material.pbr_metallic_roughness {
            pbr.roughness_factor = Some(factor);
        }
        
        self
    }
    
    fn pbr_mut(&mut self) -> &mut PbrMetallicRoughness {
        self.material.pbr_metallic_roughness.get_or_insert_with(PbrMetallicRoughness::default)
    }

    fn info(t: impl Into<TextureRef>) -> TextureInfo {
        let t = t.into();
        let mut info = TextureInfo::default();
        info.index = t.texture;
        info.tex_coord = t.model_tex_coord();
        info
    }

    /// Set the base color texture. A bare `usize` samples `TEXCOORD_0`.
    pub fn with_base_color_texture(mut self, texture: impl Into<TextureRef>) -> Self {
        self.pbr_mut().base_color_texture = Some(Self::info(texture));
        self
    }

    /// Set the metallic-roughness texture (green = roughness, blue = metalness).
    pub fn with_metallic_roughness_texture(mut self, texture: impl Into<TextureRef>) -> Self {
        self.pbr_mut().metallic_roughness_texture = Some(Self::info(texture));
        self
    }

    /// Set the normal texture and its `scale` (1.0 is the default and is omitted from the JSON;
    /// 0 and negative values are legal).
    pub fn with_normal_texture(mut self, texture: impl Into<TextureRef>, scale: f32) -> Self {
        let t = texture.into();
        let mut info = NormalTextureInfo::default();
        info.index = t.texture;
        info.tex_coord = t.model_tex_coord();
        info.scale = (scale != 1.0).then_some(scale);
        self.material.normal_texture = Some(info);
        self
    }

    /// Set the occlusion texture and its `strength` (must be in [0, 1]; 1.0 is the default and
    /// is omitted from the JSON).
    pub fn with_occlusion_texture(mut self, texture: impl Into<TextureRef>, strength: f32) -> Self {
        let t = texture.into();
        let mut info = OcclusionTextureInfo::default();
        info.index = t.texture;
        info.tex_coord = t.model_tex_coord();
        info.strength = (strength != 1.0).then_some(strength);
        self.material.occlusion_texture = Some(info);
        self
    }

    /// Set the emissive texture.
    pub fn with_emissive_texture(mut self, texture: impl Into<TextureRef>) -> Self {
        self.material.emissive_texture = Some(Self::info(texture));
        self
    }

    /// Set emissive factor
    pub fn with_emissive_factor(mut self, factor: [f32; 3]) -> Self {
        self.material.emissive_factor = Some(factor);
        self
    }
    
    /// Set the alpha mode (`Mask` also sets the cutoff; the other modes clear it).
    pub fn with_alpha(mut self, mode: AlphaMode) -> Self {
        mode.apply(&mut self.material);
        self
    }

    /// Set alpha mode and cutoff from a string. Not checked until `try_build` or `validate()`.
    #[deprecated(since = "0.12.0", note = "use `with_alpha(AlphaMode)`")]
    pub fn with_alpha_mode(mut self, mode: String, cutoff: Option<f32>) -> Self {
        self.material.alpha_mode = Some(mode);
        self.material.alpha_cutoff = cutoff;
        self
    }

    /// Set the `KHR_materials_pbrSpecularGlossiness` workflow (deprecated upstream). Combine with
    /// the metallic-roughness setters to also write a fallback. `GltfBuilder::add_material_from`
    /// registers the extension in `extensionsUsed` (never `extensionsRequired`).
    pub fn with_specular_glossiness(mut self, sg: SpecularGlossiness) -> Self {
        self.material.extensions.get_or_insert_with(MaterialExtensions::default).pbr_specular_glossiness =
            Some(sg.to_model());
        self
    }

    /// Attach application-specific `extras` (any JSON value) to the material.
    pub fn with_extras(mut self, extras: serde_json::Value) -> Self {
        self.material.extras = Some(extras);
        self
    }
    
    /// Set double sided flag
    pub fn with_double_sided(mut self, double_sided: bool) -> Self {
        self.material.double_sided = Some(double_sided);
        self
    }
    
    /// Build the material without checking it (`validate()` and `add_material_from` still do).
    pub fn build(self) -> Material {
        self.material
    }

    /// Build the material after the checks that need no document context: factor ranges,
    /// finiteness, alpha mode and cutoff, occlusion strength. Texture indices are checked by
    /// [`GltfBuilder::add_material_from`](crate::GltfBuilder::add_material_from).
    ///
    /// # Errors
    /// `GltfError::InvalidMaterial` with the first failing `MATERIAL_*` code.
    ///
    /// ```
    /// use mesh_tools::{material::MaterialBuilder, GltfError};
    /// let err = MaterialBuilder::new(None).with_metallic_factor(1.5).try_build().unwrap_err();
    /// assert!(matches!(err, GltfError::InvalidMaterial { code: "MATERIAL_FACTOR_RANGE", .. }));
    /// ```
    pub fn try_build(self) -> Result<Material> {
        match check_material(&self.material, None).into_iter().find(|i| !i.warning) {
            Some(i) => Err(GltfError::InvalidMaterial { index: None, code: i.code, message: i.msg }),
            None => Ok(self.material),
        }
    }
}

/// Create a basic material with the specified color
pub fn create_basic_material(name: Option<String>, color: [f32; 4]) -> Material {
    MaterialBuilder::new(name)
        .with_base_color(color)
        .build()
}

/// Create a metallic material
pub fn create_metallic_material(
    name: Option<String>, 
    color: [f32; 4], 
    metallic: f32,
    roughness: f32
) -> Material {
    MaterialBuilder::new(name)
        .with_base_color(color)
        .with_metallic_factor(metallic)
        .with_roughness_factor(roughness)
        .build()
}

/// Create a textured material with additional options
pub fn create_textured_material(
    name: Option<String>,
    base_color_texture: Option<usize>,
    metallic_roughness_texture: Option<usize>,
    normal_texture: Option<usize>,
    occlusion_texture: Option<usize>,
    emissive_texture: Option<usize>,
    emissive_factor: Option<[f32; 3]>,
    metallic_factor: Option<f32>,
    roughness_factor: Option<f32>,
    alpha_mode: Option<String>,
    alpha_cutoff: Option<f32>,
    double_sided: Option<bool>
) -> Material {
    let mut builder = MaterialBuilder::new(name);
    
    if let Some(texture) = base_color_texture {
        builder = builder.with_base_color_texture(texture);
    }
    
    if let Some(texture) = metallic_roughness_texture {
        builder = builder.with_metallic_roughness_texture(texture);
    }
    
    if let Some(texture) = normal_texture {
        builder = builder.with_normal_texture(texture, 1.0);
    }
    
    if let Some(texture) = occlusion_texture {
        builder = builder.with_occlusion_texture(texture, 1.0);
    }
    
    if let Some(texture) = emissive_texture {
        builder = builder.with_emissive_texture(texture);
    }
    
    if let Some(factor) = emissive_factor {
        builder = builder.with_emissive_factor(factor);
    }
    
    if let Some(factor) = metallic_factor {
        builder = builder.with_metallic_factor(factor);
    }
    
    if let Some(factor) = roughness_factor {
        builder = builder.with_roughness_factor(factor);
    }
    
    if let Some(mode) = alpha_mode {
        // The string is passed through unchecked, as before; validate() reports bad values.
        builder.material.alpha_mode = Some(mode);
        builder.material.alpha_cutoff = alpha_cutoff;
    }
    
    if let Some(ds) = double_sided {
        builder = builder.with_double_sided(ds);
    }
    
    builder.build()
}
