//! # Mesh Creation and Manipulation
//!
//! This module provides utilities for working with 3D meshes in the glTF format.
//! Meshes in glTF are composed of one or more mesh primitives, each representing
//! a drawable part of the mesh with its own geometry and material.
//!
//! The `MeshBuilder` struct provides a builder pattern for creating mesh objects
//! with various attributes:
//! - Vertex positions
//! - Vertex indices (triangulation)
//! - Normal vectors and tangents
//! - Vertex colors
//! - Primitive modes (points, lines, strips, fans)
//! - Texture coordinates (UV mapping)
//! - Material references
//! - Joint weights
//! - Morph targets, default morph weights and target names
//!
//! The module also provides helper functions for processing mesh data such as calculating
//! attribute mappings for glTF primitives.
//!
//! ## Example
//!
//! ```rust
//! use mesh_tools::{GltfBuilder, mesh::MeshBuilder};
//!
//! let mut builder = GltfBuilder::new();
//! // Uploads all attributes into the builder and returns the mesh index
//! let mesh_index = MeshBuilder::new(Some("CustomMesh".to_string()))
//!     .with_positions(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0])
//!     .with_indices(vec![0, 1, 2])
//!     .with_normals(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0])
//!     .build_into(&mut builder)
//!     .unwrap();
//! ```

use crate::builder::GltfBuilder;
use crate::error::{GltfError, Result};
use crate::builder_morph::{validate_targets, validate_weights_and_names, MorphTarget};
use crate::builder_skin::{JointIndices, SkinSet};
use crate::models::Primitive;
use crate::primitive_spec::{ColorData, Indices, PrimitiveMode, PrimitiveSpec, TexCoords};
use serde_json::Value;
use std::collections::HashMap;

/// Builder for creating and configuring 3D mesh objects
pub struct MeshBuilder {
    builder: Option<Box<dyn FnMut(&[f32], &[u16], Option<&[f32]>, Option<Vec<Vec<f32>>>, Option<usize>) -> Primitive>>,
    positions: Vec<f32>,
    indices: Vec<u16>,
    indices_u32: Option<Vec<u32>>,
    normals: Option<Vec<f32>>,
    tangents: Option<Vec<f32>>,
    colors: Vec<ColorData>,
    mode: Option<PrimitiveMode>,
    primitive_extras: Option<Value>,
    mesh_extras: Option<Value>,
    texcoords: Option<Vec<Vec<f32>>>,
    joints: Option<Vec<[u16; 4]>>,
    weights: Option<Vec<[f32; 4]>>,
    joint_count: Option<usize>,
    morph_targets: Option<Vec<MorphTarget>>,
    default_weights: Option<Vec<f32>>,
    target_names: Option<Vec<String>>,
    material: Option<usize>,
    name: Option<String>,
}

impl MeshBuilder {
    /// Create a new mesh builder
    pub fn new(name: Option<String>) -> Self {
        Self {
            builder: None,
            positions: Vec::new(),
            indices: Vec::new(),
            indices_u32: None,
            normals: None,
            tangents: None,
            colors: Vec::new(),
            mode: None,
            primitive_extras: None,
            mesh_extras: None,
            texcoords: None,
            joints: None,
            weights: None,
            joint_count: None,
            morph_targets: None,
            default_weights: None,
            target_names: None,
            material: None,
            name,
        }
    }
    
    /// Set the positions for the mesh
    pub fn with_positions(mut self, positions: Vec<f32>) -> Self {
        self.positions = positions;
        self
    }
    
    /// Set the indices for the mesh
    ///
    /// Written as `u16`, or `u32` if an index is 65535 or more (which glTF forbids as a `u16`).
    /// Replaces indices set with [`with_indices_u32`](Self::with_indices_u32).
    pub fn with_indices(mut self, indices: Vec<u16>) -> Self {
        self.indices = indices;
        self.indices_u32 = None;
        self
    }

    /// Set the indices as `u32`, written as `u32` exactly (use for more than 65535 vertices).
    /// Replaces indices set with [`with_indices`](Self::with_indices).
    pub fn with_indices_u32(mut self, indices: Vec<u32>) -> Self {
        self.indices_u32 = Some(indices);
        self.indices = Vec::new();
        self
    }

    /// Set the tangents: 4 floats per vertex (`x, y, z, w` with `w` = 1 or -1).
    pub fn with_tangents(mut self, tangents: Vec<f32>) -> Self {
        self.tangents = Some(tangents);
        self
    }

    /// Set the vertex color sets; set `n` becomes `COLOR_n`.
    pub fn with_colors(mut self, colors: Vec<ColorData>) -> Self {
        self.colors = colors;
        self
    }

    /// Set the primitive mode (default: triangles). The index (or vertex) count is checked
    /// against it.
    pub fn with_mode(mut self, mode: PrimitiveMode) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Set `extras` of the primitive (any JSON value).
    pub fn with_primitive_extras(mut self, extras: Value) -> Self {
        self.primitive_extras = Some(extras);
        self
    }

    /// Set `extras` of the mesh. With morph target names it must be an object without a
    /// `targetNames` key; the names are merged in.
    pub fn with_mesh_extras(mut self, extras: Value) -> Self {
        self.mesh_extras = Some(extras);
        self
    }
    
    /// Set the normals for the mesh
    pub fn with_normals(mut self, normals: Vec<f32>) -> Self {
        self.normals = Some(normals);
        self
    }
    
    /// Set a single set of texture coordinates for the mesh
    pub fn with_texcoords(mut self, texcoords: Vec<f32>) -> Self {
        let mut texcoord_sets = Vec::new();
        texcoord_sets.push(texcoords);
        self.texcoords = Some(texcoord_sets);
        self
    }
    
    /// Set multiple sets of texture coordinates for the mesh
    pub fn with_multiple_texcoords(mut self, texcoord_sets: Vec<Vec<f32>>) -> Self {
        self.texcoords = Some(texcoord_sets);
        self
    }
    
    /// Set the material index for the mesh
    pub fn with_material(mut self, material: usize) -> Self {
        self.material = Some(material);
        self
    }
    
    /// Set the primitive builder function
    #[deprecated(since = "0.10.0", note = "the closure bypasses validation; use `GltfBuilder::create_primitive` with `add_mesh_checked`")]
    pub fn with_primitive_builder(
        mut self,
        builder: Box<dyn FnMut(&[f32], &[u16], Option<&[f32]>, Option<Vec<Vec<f32>>>, Option<usize>) -> Primitive>,
    ) -> Self {
        self.builder = Some(builder);
        self
    }
    
    /// Set the joints for the mesh
    pub fn with_joints(mut self, joints: Vec<[u16; 4]>) -> Self {
        self.joints = Some(joints);
        self
    }
    
    /// Set the weights for the mesh
    pub fn with_weights(mut self, weights: Vec<[f32; 4]>) -> Self {
        self.weights = Some(weights);
        self
    }
    
    /// Set the number of joints in the skin this mesh will be used with.
    ///
    /// When set, every joint slot with a non-zero weight is checked against it. When not
    /// set, only the `u16` range is enforced (the skin isn't known yet).
    pub fn with_joint_count(mut self, joint_count: usize) -> Self {
        self.joint_count = Some(joint_count);
        self
    }

    /// Set the morph targets of the mesh (see [`MorphTarget`]).
    pub fn with_morph_targets(mut self, targets: Vec<MorphTarget>) -> Self {
        self.morph_targets = Some(targets);
        self
    }

    /// Set the default morph target weights (one per target). Requires morph targets.
    pub fn with_default_weights(mut self, weights: Vec<f32>) -> Self {
        self.default_weights = Some(weights);
        self
    }

    /// Set the morph target names (one per target), written to `mesh.extras.targetNames`.
    /// Requires morph targets.
    pub fn with_target_names(mut self, names: Vec<String>) -> Self {
        self.target_names = Some(names);
        self
    }

    /// Upload the mesh into `gltf` and return its mesh index.
    ///
    /// All attributes are uploaded: positions (with bounds), optional normals and tangents, any
    /// number of texcoord sets (`TEXCOORD_n`) and color sets (`COLOR_n`), indices (empty means
    /// non-indexed geometry), and, if set, `JOINTS_0`/`WEIGHTS_0`. Morph targets are written as
    /// `primitive.targets`, default weights as `mesh.weights` and target names as
    /// `mesh.extras.targetNames` (see [`GltfBuilder::attach_morph_targets`] and
    /// [`GltfBuilder::add_morphable_mesh`]). The mesh has one primitive; use
    /// [`GltfBuilder::create_primitive`] and [`GltfBuilder::add_mesh_checked`] for several.
    ///
    /// If a custom primitive builder was set with `with_primitive_builder` (deprecated), it is
    /// called to produce the primitive instead, and neither joints/weights/joint count, morph
    /// data, nor tangents/colors/mode/`u32` indices/primitive extras can be combined with it.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if joints are set without weights or the reverse, if a
    /// custom primitive builder is combined with the data above, if default weights or
    /// target names are set without morph targets, if mesh extras are not an object (or contain
    /// `targetNames`) while target names are set, or for any validation failure documented on
    /// [`GltfBuilder::create_primitive`] (counts, mode, index range, tangents, material) and
    /// [`GltfBuilder::attach_morph_targets`] (target count, delta lengths, finite values, normal
    /// deltas need base normals). All data is validated before anything is written, so the
    /// document is unchanged on error.
    pub fn build_into(self, gltf: &mut GltfBuilder) -> Result<usize> {
        let has_skin_data = self.joints.is_some() || self.weights.is_some() || self.joint_count.is_some();
        let has_morph_data = self.morph_targets.is_some() || self.default_weights.is_some() || self.target_names.is_some();

        if let Some(mut builder_fn) = self.builder {
            if has_skin_data {
                return Err(GltfError::InvalidInput(
                    "a custom primitive builder cannot be combined with joints, weights or joint count".to_string(),
                ));
            }
            if has_morph_data {
                return Err(GltfError::InvalidInput(
                    "a custom primitive builder cannot be combined with morph targets, default weights or target names".to_string(),
                ));
            }
            if self.tangents.is_some() || !self.colors.is_empty() || self.mode.is_some() || self.indices_u32.is_some() || self.primitive_extras.is_some() {
                return Err(GltfError::InvalidInput(
                    "a custom primitive builder cannot be combined with tangents, colors, mode, u32 indices or primitive extras".to_string(),
                ));
            }
            let primitive = builder_fn(
                &self.positions,
                &self.indices,
                self.normals.as_deref(),
                self.texcoords,
                self.material,
            );
            let index = gltf.add_mesh(self.name, vec![primitive]);
            if let Some(e) = self.mesh_extras {
                gltf.set_extras(crate::ExtensionTarget::Mesh(index), e)?;
            }
            return Ok(index);
        }

        let skin_sets = match (self.joints, self.weights) {
            (Some(joints), Some(weights)) => vec![SkinSet { joints: JointIndices::U16(joints), weights }],
            (None, None) => Vec::new(),
            (Some(_), None) => {
                return Err(GltfError::InvalidInput("joints were set but weights were not".to_string()));
            }
            (None, Some(_)) => {
                return Err(GltfError::InvalidInput("weights were set but joints were not".to_string()));
            }
        };
        if skin_sets.is_empty() && self.joint_count.is_some() {
            return Err(GltfError::InvalidInput("joint count was set but there are no joints/weights".to_string()));
        }

        // Validate all morph data and extras before writing the base primitive, so a failure
        // leaves the document unchanged.
        match &self.morph_targets {
            Some(targets) => {
                validate_targets(targets, self.positions.len() / 3, self.normals.is_some())?;
                validate_weights_and_names(targets.len(), self.default_weights.as_deref(), self.target_names.as_deref())?;
                if let (Some(extras), Some(_)) = (&self.mesh_extras, &self.target_names) {
                    match extras.as_object() {
                        None => {
                            return Err(GltfError::InvalidInput("mesh extras must be an object when target names are set".to_string()));
                        }
                        Some(o) if o.contains_key("targetNames") => {
                            return Err(GltfError::InvalidInput("mesh extras contain targetNames, which the target names set here would clash with".to_string()));
                        }
                        Some(_) => {}
                    }
                }
            }
            None if self.default_weights.is_some() || self.target_names.is_some() => {
                return Err(GltfError::InvalidInput(
                    "default weights or target names were set but there are no morph targets".to_string(),
                ));
            }
            None => {}
        }

        let indices = match self.indices_u32 {
            Some(i) if !i.is_empty() => Some(Indices::U32(i)),
            _ if !self.indices.is_empty() => Some(Indices::auto(self.indices.iter().map(|&i| u32::from(i)).collect())),
            _ => None,
        };
        let mut primitive = gltf.create_primitive(PrimitiveSpec {
            positions: self.positions,
            indices,
            normals: self.normals,
            tangents: self.tangents,
            texcoords: self.texcoords.into_iter().flatten().map(TexCoords::F32).collect(),
            colors: self.colors,
            skin_sets,
            joint_count: self.joint_count,
            material: self.material,
            mode: self.mode.unwrap_or_default(),
            extras: self.primitive_extras,
        })?;
        match self.morph_targets {
            Some(targets) => {
                gltf.attach_morph_targets(&mut primitive, &targets)?;
                let index = gltf.add_morphable_mesh(self.name, vec![primitive], self.default_weights, self.target_names)?;
                if let Some(e) = self.mesh_extras {
                    gltf.set_extras(crate::ExtensionTarget::Mesh(index), e)?;
                }
                Ok(index)
            }
            None => gltf.add_mesh_checked(self.name, vec![primitive], self.mesh_extras),
        }
    }
}

/// Calculate min and max bounds of positions
pub fn calculate_bounds(positions: &[f32]) -> (Vec<f32>, Vec<f32>) {
    if positions.is_empty() {
        return (vec![], vec![]);
    }
    
    // Assuming positions are in format [x1, y1, z1, x2, y2, z2, ...]
    let components_per_vertex = 3;
    
    let mut min = vec![f32::MAX; components_per_vertex];
    let mut max = vec![f32::MIN; components_per_vertex];
    
    for i in (0..positions.len()).step_by(components_per_vertex) {
        for j in 0..components_per_vertex {
            if i + j < positions.len() {
                min[j] = min[j].min(positions[i + j]);
                max[j] = max[j].max(positions[i + j]);
            }
        }
    }
    
    (min, max)
}

/// Create attribute mapping for mesh primitive
pub fn create_attribute_mapping(
    position_accessor: usize,
    normal_accessor: Option<usize>,
    texcoord_accessors: Option<Vec<usize>>,
) -> HashMap<String, usize> {
    let mut attributes = HashMap::new();
    
    // Position attribute is always required
    attributes.insert("POSITION".to_string(), position_accessor);
    
    // Add normal attribute if provided
    if let Some(normal_index) = normal_accessor {
        attributes.insert("NORMAL".to_string(), normal_index);
    }
    
    // Add texture coordinate attributes if provided
    if let Some(texcoord_indices) = texcoord_accessors {
        for (i, texcoord_index) in texcoord_indices.iter().enumerate() {
            attributes.insert(format!("TEXCOORD_{}", i), *texcoord_index);
        }
    }
    
    attributes
}
