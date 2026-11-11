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
//! - Normal vectors
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
use std::collections::HashMap;

/// Builder for creating and configuring 3D mesh objects
pub struct MeshBuilder {
    builder: Option<Box<dyn FnMut(&[f32], &[u16], Option<&[f32]>, Option<Vec<Vec<f32>>>, Option<usize>) -> Primitive>>,
    positions: Vec<f32>,
    indices: Vec<u16>,
    normals: Option<Vec<f32>>,
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
            normals: None,
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
    pub fn with_indices(mut self, indices: Vec<u16>) -> Self {
        self.indices = indices;
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
    /// All attributes are uploaded: positions (with bounds), optional normals, any number
    /// of texcoord sets (`TEXCOORD_n`), indices (`u16` here; empty means non-indexed
    /// geometry), and, if set, `JOINTS_0`/`WEIGHTS_0`. Morph targets are written as
    /// `primitive.targets`, default weights as `mesh.weights` and target names as
    /// `mesh.extras.targetNames` (see [`GltfBuilder::attach_morph_targets`] and
    /// [`GltfBuilder::add_morphable_mesh`]).
    ///
    /// If a custom primitive builder was set with
    /// [`with_primitive_builder`](Self::with_primitive_builder), it is called to produce the
    /// primitive instead, and neither joints/weights/joint count nor morph targets/default
    /// weights/target names can be combined with it.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if joints are set without weights or the reverse, if a
    /// custom primitive builder is combined with skinning or morph data, if default weights or
    /// target names are set without morph targets, or for any validation failure documented on
    /// [`GltfBuilder::create_skinned_primitive`](crate::GltfBuilder::create_skinned_primitive)
    /// (counts, index range, weights) or [`GltfBuilder::attach_morph_targets`] (target count,
    /// delta lengths, finite values, no tangent deltas, normal deltas need base normals). The
    /// default weights and target names must each match the number of morph targets and the
    /// weights must be finite. All morph data is validated before anything is written, so the
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
            let primitive = builder_fn(
                &self.positions,
                &self.indices,
                self.normals.as_deref(),
                self.texcoords,
                self.material,
            );
            return Ok(gltf.add_mesh(self.name, vec![primitive]));
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

        // Validate all morph data before writing the base primitive, so a failure leaves the
        // document unchanged.
        match &self.morph_targets {
            Some(targets) => {
                validate_targets(targets, self.positions.len() / 3, self.normals.is_some())?;
                validate_weights_and_names(targets.len(), self.default_weights.as_deref(), self.target_names.as_deref())?;
            }
            None if self.default_weights.is_some() || self.target_names.is_some() => {
                return Err(GltfError::InvalidInput(
                    "default weights or target names were set but there are no morph targets".to_string(),
                ));
            }
            None => {}
        }

        let indices: Vec<u32> = self.indices.iter().map(|&i| u32::from(i)).collect();
        let texcoord_sets: Vec<&[f32]> = self.texcoords.iter().flatten().map(|t| t.as_slice()).collect();
        let mut primitive = gltf.create_primitive_checked(
            &self.positions,
            if indices.is_empty() { None } else { Some(&indices) },
            self.normals.as_deref(),
            &texcoord_sets,
            &skin_sets,
            self.joint_count.unwrap_or(u16::MAX as usize + 1),
            self.material,
        )?;
        match self.morph_targets {
            Some(targets) => {
                gltf.attach_morph_targets(&mut primitive, &targets)?;
                gltf.add_morphable_mesh(self.name, vec![primitive], self.default_weights, self.target_names)
            }
            None => Ok(gltf.add_mesh(self.name, vec![primitive])),
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
