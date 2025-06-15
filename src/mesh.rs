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
//! use mesh_tools::mesh::MeshBuilder;
//!
//! // Create a mesh with custom data
//! let mesh = MeshBuilder::new(Some("CustomMesh".to_string()))
//!     .with_positions(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0])
//!     .with_indices(vec![0, 1, 2])
//!     .with_normals(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0])
//!     .build();
//! ```

use crate::models::{Primitive, Mesh};
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
    
    /// Build the mesh
    pub fn build(self) -> Mesh {
        let mut mesh = Mesh::default();
        mesh.name = self.name;
        
        if let Some(mut builder_fn) = self.builder {
            let primitive = builder_fn(
                &self.positions,
                &self.indices, 
                self.normals.as_deref(),
                self.texcoords.clone(),
                self.material
            );
            mesh.primitives = vec![primitive];
        } else {
            // Use default primitive building if no custom builder provided
            let primitive = crate::models::Primitive {
                attributes: std::collections::HashMap::new(),
                indices: None,
                material: self.material,
                mode: None,
            };
            mesh.primitives = vec![primitive];
        }
        
        mesh
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
