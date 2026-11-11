//! # glTF Data Model Definitions
//!
//! This module contains the core data structures that represent a glTF 2.0 document.
//! These structures match the JSON schema defined in the [glTF 2.0 specification](https://www.khronos.org/registry/glTF/specs/2.0/glTF-2.0.html).
//!
//! The structures in this module are designed to be serialized to JSON using serde,
//! with optional fields that are skipped when None to produce valid glTF JSON.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Extension payloads keyed by extension name. Payloads are opaque JSON; they are not
/// interpreted or validated.
pub type Extensions = serde_json::Map<String, serde_json::Value>;

/// Represents a complete glTF 2.0 document
///
/// A glTF document contains all the resources and metadata needed to represent a 3D scene or model.
/// It includes scenes, nodes, meshes, materials, textures, and binary data references.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Gltf {
    pub asset: Asset,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenes: Option<Vec<Scene>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<Vec<Node>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meshes: Option<Vec<Mesh>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessors: Option<Vec<Accessor>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "bufferViews")]
    pub buffer_views: Option<Vec<BufferView>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffers: Option<Vec<Buffer>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub materials: Option<Vec<Material>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub textures: Option<Vec<Texture>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<Image>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub samplers: Option<Vec<Sampler>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skins: Option<Vec<Skin>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animations: Option<Vec<Animation>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "extensionsUsed")]
    pub extensions_used: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "extensionsRequired")]
    pub extensions_required: Option<Vec<String>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents the glTF asset information
#[derive(Serialize, Deserialize, Debug, Default)]
#[non_exhaustive]
pub struct Asset {
    pub version: String,

    /// Minimum glTF version a loader must support (`major.minor`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "minVersion")]
    pub min_version: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copyright: Option<String>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF scene
#[derive(Serialize, Deserialize, Debug, Default)]
#[non_exhaustive]
pub struct Scene {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<Vec<usize>>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF node
#[derive(Serialize, Deserialize, Debug, Default)]
#[non_exhaustive]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation: Option<[f32; 3]>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<[f32; 4]>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<[f32; 3]>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matrix: Option<[f32; 16]>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<usize>>,

    /// Morph target weights for this node's mesh. Overrides `Mesh.weights`.
    /// Its length must equal the mesh's morph target count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weights: Option<Vec<f32>>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF mesh
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Mesh {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    
    pub primitives: Vec<Primitive>,

    /// Default morph target weights; its length must equal the number of morph targets
    /// of every primitive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weights: Option<Vec<f32>>,

    /// Application-specific data. By convention `extras.targetNames` holds one name per
    /// morph target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,
}

/// Represents a glTF mesh primitive
#[derive(Serialize, Deserialize, Debug, Default)]
#[non_exhaustive]
pub struct Primitive {
    pub attributes: HashMap<String, usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indices: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<usize>,

    /// Morph targets: one attribute map per target (`POSITION`/`NORMAL`/`TANGENT` deltas
    /// relative to the base attributes, mapped to accessor indices).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<HashMap<String, usize>>>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF accessor
#[derive(Serialize, Deserialize, Debug, Default)]
#[non_exhaustive]
pub struct Accessor {
    /// Optional user-facing name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Buffer view holding the data. `None` means the accessor reads as all zeros (or is
    /// defined only by sparse data).
    #[serde(rename = "bufferView", skip_serializing_if = "Option::is_none")]
    pub buffer_view: Option<usize>,
    #[serde(rename = "componentType")]
    pub component_type: usize,
    pub count: usize,
    #[serde(rename = "type")]
    pub type_: String,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "byteOffset")]
    pub byte_offset: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<Vec<f64>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Vec<f64>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized: Option<bool>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF buffer view
#[derive(Serialize, Deserialize, Debug, Default)]
#[non_exhaustive]
pub struct BufferView {
    /// Optional user-facing name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub buffer: usize,
    #[serde(rename = "byteOffset")]
    pub byte_offset: usize,
    #[serde(rename = "byteLength")]
    pub byte_length: usize,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "byteStride")]
    pub byte_stride: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<usize>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF buffer
#[derive(Serialize, Deserialize, Debug, Default)]
#[non_exhaustive]
pub struct Buffer {
    /// Optional user-facing name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "byteLength")]
    pub byte_length: usize,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF material
#[derive(Serialize, Deserialize, Debug, Default)]
#[non_exhaustive]
pub struct Material {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "pbrMetallicRoughness")]
    pub pbr_metallic_roughness: Option<PbrMetallicRoughness>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "normalTexture")]
    pub normal_texture: Option<NormalTextureInfo>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "occlusionTexture")]
    pub occlusion_texture: Option<OcclusionTextureInfo>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "emissiveTexture")]
    pub emissive_texture: Option<TextureInfo>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "emissiveFactor")]
    pub emissive_factor: Option<[f32; 3]>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "alphaMode")]
    pub alpha_mode: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "alphaCutoff")]
    pub alpha_cutoff: Option<f32>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "doubleSided")]
    pub double_sided: Option<bool>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<MaterialExtensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF PBR material
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct PbrMetallicRoughness {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "baseColorFactor")]
    pub base_color_factor: Option<[f32; 4]>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "baseColorTexture")]
    pub base_color_texture: Option<TextureInfo>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "metallicFactor")]
    pub metallic_factor: Option<f32>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "roughnessFactor")]
    pub roughness_factor: Option<f32>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "metallicRoughnessTexture")]
    pub metallic_roughness_texture: Option<TextureInfo>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents basic texture reference information
#[derive(Serialize, Deserialize, Debug, Default)]
#[non_exhaustive]
pub struct TextureInfo {
    pub index: usize,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "texCoord")]
    pub tex_coord: Option<usize>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents normal texture reference information
#[derive(Serialize, Deserialize, Debug, Default)]
#[non_exhaustive]
pub struct NormalTextureInfo {
    pub index: usize,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "texCoord")]
    pub tex_coord: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents occlusion texture reference information
#[derive(Serialize, Deserialize, Debug, Default)]
#[non_exhaustive]
pub struct OcclusionTextureInfo {
    pub index: usize,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "texCoord")]
    pub tex_coord: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strength: Option<f32>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF texture
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Texture {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    
    pub source: usize,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampler: Option<usize>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF image
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Image {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "bufferView")]
    pub buffer_view: Option<usize>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF sampler
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Sampler {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "magFilter")]
    pub mag_filter: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "minFilter")]
    pub min_filter: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "wrapS")]
    pub wrap_s: Option<usize>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "wrapT")]
    pub wrap_t: Option<usize>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF animation
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Animation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<AnimationChannel>>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub samplers: Option<Vec<AnimationSampler>>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF animation channel
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct AnimationChannel {
    pub sampler: usize,
    
    pub target: AnimationChannelTarget,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF animation channel target
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct AnimationChannelTarget {
    pub node: usize,
    
    pub path: String,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF animation sampler
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct AnimationSampler {
    pub input: usize,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpolation: Option<String>,
    
    pub output: usize,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents material extensions for glTF
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct MaterialExtensions {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "KHR_materials_pbrSpecularGlossiness")]
    pub pbr_specular_glossiness: Option<PbrSpecularGlossiness>,

    /// Any other extension on the material, kept as opaque JSON. Must not contain
    /// `KHR_materials_pbrSpecularGlossiness`; that key is the typed field above (a duplicate
    /// would serialize twice).
    #[serde(flatten)]
    pub other: Extensions,
}

/// Represents a glTF specular-glossiness PBR material extension
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct PbrSpecularGlossiness {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "diffuseFactor")]
    pub diffuse_factor: Option<[f32; 4]>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "diffuseTexture")]
    pub diffuse_texture: Option<TextureInfo>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "specularFactor")]
    pub specular_factor: Option<[f32; 3]>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "glossinessFactor")]
    pub glossiness_factor: Option<f32>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "specularGlossinessTexture")]
    pub specular_glossiness_texture: Option<TextureInfo>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}

/// Represents a glTF skin for skeletal animation
///
/// A skin defines a set of joints and inverse bind matrices that transform
/// the geometry from model space to joint space for skeletal animation.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Skin {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    
    /// Joints define the skeleton structure by node indices
    pub joints: Vec<usize>,
    
    /// Index of the accessor containing inverse bind matrices
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "inverseBindMatrices")]
    pub inverse_bind_matrices: Option<usize>,
    
    /// Index of the skeleton node (root joint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skeleton: Option<usize>,

    /// Extension payloads keyed by extension name (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,

    /// Application-specific data (any JSON value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
}
