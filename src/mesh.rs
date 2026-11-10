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

    /// Upload the mesh into `gltf` and return its mesh index.
    ///
    /// All attributes are uploaded: positions (with bounds), optional normals, any number
    /// of texcoord sets (`TEXCOORD_n`), indices (`u16` here; empty means non-indexed
    /// geometry), and, if set, `JOINTS_0`/`WEIGHTS_0`.
    ///
    /// If a custom primitive builder was set with
    /// [`with_primitive_builder`](Self::with_primitive_builder), it is called to produce the
    /// primitive instead, and joints/weights/joint count cannot be combined with it.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if joints are set without weights or the reverse, if a
    /// custom primitive builder is combined with skinning data, or for any validation
    /// failure documented on
    /// [`GltfBuilder::create_skinned_primitive`](crate::GltfBuilder::create_skinned_primitive)
    /// (counts, index range, weights). The document is unchanged on error.
    pub fn build_into(self, gltf: &mut GltfBuilder) -> Result<usize> {
        let has_skin_data = self.joints.is_some() || self.weights.is_some() || self.joint_count.is_some();

        if let Some(mut builder_fn) = self.builder {
            if has_skin_data {
                return Err(GltfError::InvalidInput(
                    "a custom primitive builder cannot be combined with joints, weights or joint count".to_string(),
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

        let indices: Vec<u32> = self.indices.iter().map(|&i| u32::from(i)).collect();
        let texcoord_sets: Vec<&[f32]> = self.texcoords.iter().flatten().map(|t| t.as_slice()).collect();
        let primitive = gltf.create_primitive_checked(
            &self.positions,
            if indices.is_empty() { None } else { Some(&indices) },
            self.normals.as_deref(),
            &texcoord_sets,
            &skin_sets,
            self.joint_count.unwrap_or(u16::MAX as usize + 1),
            self.material,
        )?;
        Ok(gltf.add_mesh(self.name, vec![primitive]))
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
