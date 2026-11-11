//! # mesh-tools: A Rust library for exporting glTF/GLB files
//!
//! This library provides a high-level API for creating and exporting 3D models in the
//! [glTF](https://www.khronos.org/gltf/) (GL Transmission Format) standard, which is a 
//! JSON-based format for 3D scenes and models, widely used for efficient transmission and loading 
//! of 3D content.
//!
//! ## Key Features
//! 
//! - Create and manipulate 3D geometry (vertices, normals, UVs, indices)
//! - Generate primitive shapes (boxes, spheres, planes, cylinders, etc.)
//! - Define materials with physically-based rendering (PBR) properties
//! - Support for textures and image data
//! - Create complex hierarchical scenes with node parent-child relationships
//! - Skinning and keyframe animation (translation, rotation, scale; linear, step and cubic spline)
//! - Morph targets (blend shapes) with default weights, target names and weights animation
//! - Validated export to GLB (single binary file); see [`GltfBuilder::validate`]
//! - Lightweight math types via the mint crate
//!
//! ## Math Types
//!
//! This library uses the lightweight [mint](https://crates.io/crates/mint) crate for mathematical types.
//! A compatibility layer is provided in the `compat` module to make working with these types easy:
//!
//! ```rust
//! use mesh_tools::compat::{Point3, Vector2, Vector3};
//!
//! // Use constructor functions
//! let position = mesh_tools::compat::point3::new(1.0, 2.0, 3.0);
//! let normal = mesh_tools::compat::vector3::new(0.0, 1.0, 0.0);
//! 
//! // Vector operations
//! let a = mesh_tools::compat::vector3::new(1.0, 0.0, 0.0);
//! let b = mesh_tools::compat::vector3::new(0.0, 1.0, 0.0);
//! let cross_product = mesh_tools::compat::cross(a, b);
//! ```
//!
//! ## Basic Usage
//!
//! ```rust
//! use mesh_tools::GltfBuilder;
//!
//! // Create a new glTF builder
//! let mut builder = GltfBuilder::new();
//!
//! // Create a simple box mesh
//! let box_mesh = builder.create_box(1.0);
//!
//! // Add a node referencing the mesh
//! let node = builder.add_node(
//!     Some("Box".to_string()),
//!     Some(box_mesh),
//!     None, // Default position
//!     None, // Default rotation
//!     None, // Default scale
//! );
//!
//! // Create a scene containing the node
//! builder.add_scene(Some("Main Scene".to_string()), Some(vec![node]));
//!
//! // Export to GLB format
//! builder.export_glb("output.glb").unwrap();
//! ```
//!
//! See the examples directory for more complex usage scenarios.

// Public modules
pub mod texture;     // Texture and image handling
pub mod primitives;  // Geometry generation primitives
pub mod error;       // Error types and results
pub mod models;      // glTF data model definitions
pub mod constants;   // glTF format constants
pub mod compat;      // Compatibility layer for mint math types
pub mod material;    // Material creation and management
pub mod mesh;        // Mesh creation and manipulation
pub mod builder;     // Main GltfBuilder implementation
pub mod accessor;    // Typed accessor writer types
mod sampler;          // Typed sampler settings

// Internal implementation modules
mod builder_primitives;       // Implementations for primitive shape creation
mod builder_texture;          // Implementations for texture handling
mod builder_material;         // Implementations for material handling
mod builder_material_specular; // Implementations for specular material handling
mod interleave;               // Interleaved vertex attributes
mod builder_accessor;         // The single accessor writer
mod builder_animation;        // Implementations for animation handling
mod builder_skin;             // Implementations for skin handling
mod builder_morph;            // Implementations for morph target handling
mod uri;                      // Percent-encoding and data: URIs
mod builder_export;           // Buffer encodings and .gltf export
mod builder_node;             // Scenes, node matrices and transforms
mod builder_camera;           // Cameras and node camera references
mod builder_extensions;       // Extensions, extras and asset metadata
mod builder_mesh;             // Primitive and mesh creation
mod primitive_spec;           // Primitive modes, specs and helpers
pub mod validate;             // Document validation

// Re-exports
pub use error::{GltfError, Result};
pub use models::*;
pub use builder::GltfBuilder;
pub use builder_export::{BufferEncoding, GltfParts};
pub use validate::ValidationReport;
pub use builder_primitives::Triangle;
pub use accessor::{AccessorSpec, AccessorType, Bounds, BufferViewTarget, Component, ComponentType, SparseSpec};
pub use interleave::InterleavedAttribute;
pub use sampler::{MagFilter, MinFilter, SamplerDesc, WrapMode};

// Constants re-exports
pub use constants::accessor_type;
pub use constants::buffer_view_target;
pub use constants::component_type;
pub use constants::sampler_filter;
pub use constants::sampler_wrap;
pub use constants::alpha_mode;
pub use constants::primitive_mode;
pub use builder_animation::{AnimationPath, CubicKeyframe, InterpolationType, NormalizedComponent};
pub use builder_skin::{limit_influences, normalize_weights, quantize_u16, quantize_u8, JointIndices, SkinSet, SkinWeights};
pub use builder_morph::{MorphDelta, MorphTarget};
pub use material::{AlphaMode, SpecularGlossiness, TextureRef};
pub use builder_extensions::{ExtensionTarget, TextureSlot};
pub use primitive_spec::{fan_to_triangles, strip_to_triangles, ColorData, Indices, PrimitiveMode, PrimitiveSpec, TexCoords};

// Compile and run every Rust code block in docs/SKINNING.md as a doctest, so the guide
// cannot drift from the API.
#[cfg(doctest)]
#[doc = include_str!("../docs/SKINNING.md")]
struct SkinningGuideDoctests;

// Likewise for docs/ANIMATION.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/ANIMATION.md")]
struct AnimationGuideDoctests;

// Likewise for docs/MORPH_TARGETS.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/MORPH_TARGETS.md")]
struct MorphTargetsGuideDoctests;

// Likewise for docs/VALIDATION.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/VALIDATION.md")]
struct ValidationGuideDoctests;

// Likewise for docs/EXTENSIONS.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/EXTENSIONS.md")]
struct ExtensionsGuideDoctests;

// Likewise for docs/ACCESSORS.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/ACCESSORS.md")]
struct AccessorsGuideDoctests;

// Likewise for docs/MESHES.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/MESHES.md")]
struct MeshesGuideDoctests;

// Likewise for docs/PRIMITIVE_MODES.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/PRIMITIVE_MODES.md")]
struct PrimitiveModesGuideDoctests;

// Likewise for docs/MATERIALS.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/MATERIALS.md")]
struct MaterialsGuideDoctests;

// Likewise for docs/BUFFERS.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/BUFFERS.md")]
struct BuffersGuideDoctests;

// Likewise for docs/TEXTURES.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/TEXTURES.md")]
struct TexturesGuideDoctests;

// Likewise for docs/SCENES.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/SCENES.md")]
struct ScenesGuideDoctests;

// Likewise for docs/NODES.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/NODES.md")]
struct NodesGuideDoctests;

// Likewise for docs/CAMERAS.md.
#[cfg(doctest)]
#[doc = include_str!("../docs/CAMERAS.md")]
struct CamerasGuideDoctests;
