//! # Skin Builder Support
//!
//! This module provides skin-related functionality for the GltfBuilder.
//! Skins are used for skeletal animation in glTF, where a mesh is deformed
//! by a hierarchy of joints (bones).

use crate::builder::GltfBuilder;
use crate::models::{Skin, Node};
use crate::error::Result;
use mint::{ColumnMatrix4};

impl GltfBuilder {
    /// Add a skin to the glTF document
    ///
    /// A skin contains a list of joint node indices and their inverse bind matrices.
    /// The inverse bind matrices transform vertices from model space to joint space.
    ///
    /// # Arguments
    /// 
    /// * `name` - Optional name for the skin
    /// * `joints` - Vector of node indices that represent the joints/bones
    /// * `inverse_bind_matrices_accessor` - Index of accessor containing 4x4 inverse bind matrices
    /// * `skeleton` - Optional index of the skeleton root node
    ///
    /// # Returns
    ///
    /// The index of the created skin in the glTF document
    pub fn add_skin(
        &mut self,
        name: Option<String>,
        joints: Vec<usize>,
        inverse_bind_matrices_accessor: Option<usize>,
        skeleton: Option<usize>,
    ) -> usize {
        let skin = Skin {
            name,
            joints,
            inverse_bind_matrices: inverse_bind_matrices_accessor,
            skeleton,
        };

        if let Some(ref mut skins) = self.gltf.skins {
            skins.push(skin);
            skins.len() - 1
        } else {
            // This shouldn't happen if new() is called properly
            self.gltf.skins = Some(vec![skin]);
            0
        }
    }

    /// Add a joint (bone) node to the glTF document
    ///
    /// Joint nodes are typically used in skeletal animation systems.
    /// They can have transforms and be organized in hierarchies.
    ///
    /// # Arguments
    ///
    /// * `name` - Optional name for the joint
    /// * `translation` - Optional translation relative to parent
    /// * `rotation` - Optional rotation quaternion [x, y, z, w]
    /// * `scale` - Optional scale factors
    ///
    /// # Returns
    ///
    /// The index of the created joint node
    pub fn add_joint_node(
        &mut self,
        name: Option<String>,
        translation: Option<[f32; 3]>,
        rotation: Option<[f32; 4]>,
        scale: Option<[f32; 3]>,
    ) -> usize {
        self.add_node(name, None, translation, rotation, scale)
    }

    /// Create inverse bind matrices accessor for a skin
    ///
    /// Inverse bind matrices are used to transform vertices from model space
    /// to the local space of each joint. Each matrix is 4x4 and stored as
    /// 16 consecutive floats in column-major order.
    ///
    /// # Arguments
    ///
    /// * `matrices` - Vector of 4x4 matrices (one per joint)
    ///
    /// # Returns
    ///
    /// The index of the created accessor
    pub fn create_inverse_bind_matrices_accessor(&mut self, matrices: &[ColumnMatrix4<f32>]) -> usize {
        let mut matrix_data = Vec::new();
        
        // Convert matrices to flat f32 array in column-major order
        for matrix in matrices {
            // glTF uses column-major order for matrices
            let cols = [matrix.x, matrix.y, matrix.z, matrix.w];
            for col in &cols {
                matrix_data.push(col.x);
                matrix_data.push(col.y);
                matrix_data.push(col.z);
                matrix_data.push(col.w);
            }
        }

        // Convert to bytes
        let mut bytes = Vec::new();
        for &value in &matrix_data {
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        // Add buffer data
        let (byte_offset, byte_length) = self.add_buffer_data(&bytes);

        // Create buffer view
        let buffer_view_index = self.add_buffer_view(byte_offset, byte_length, None);

        // Create accessor
        self.add_accessor(
            buffer_view_index,
            5126, // FLOAT
            matrices.len(),
            "MAT4".to_string(),
            None,
            None,
            None,
        )
    }

    /// Add a node with an associated skin
    ///
    /// This creates a node that references a mesh and applies skinning using the specified skin.
    ///
    /// # Arguments
    ///
    /// * `name` - Optional name for the node
    /// * `mesh` - Index of the mesh to render
    /// * `skin` - Index of the skin to apply for deformation
    /// * `translation` - Optional translation
    /// * `rotation` - Optional rotation quaternion [x, y, z, w]
    /// * `scale` - Optional scale factors
    ///
    /// # Returns
    ///
    /// The index of the created node
    pub fn add_skinned_node(
        &mut self,
        name: Option<String>,
        mesh: usize,
        skin: usize,
        translation: Option<[f32; 3]>,
        rotation: Option<[f32; 4]>,
        scale: Option<[f32; 3]>,
    ) -> usize {
        let node = Node {
            name,
            mesh: Some(mesh),
            skin: Some(skin),
            children: None,
            translation,
            rotation,
            scale,
            matrix: None,
        };

        if let Some(ref mut nodes) = self.gltf.nodes {
            nodes.push(node);
            nodes.len() - 1
        } else {
            self.gltf.nodes = Some(vec![node]);
            0
        }
    }

    /// Create a simple linear bone chain
    ///
    /// This utility method creates a simple chain of joints connected linearly.
    /// Useful for creating simple skeletal structures like arms, legs, or spines.
    ///
    /// # Arguments
    ///
    /// * `joint_names` - Names for each joint in the chain
    /// * `joint_positions` - Positions for each joint relative to parent
    ///
    /// # Returns
    ///
    /// Vector of joint node indices
    pub fn create_bone_chain(
        &mut self,
        joint_names: &[String],
        joint_positions: &[[f32; 3]],
    ) -> Result<Vec<usize>> {
        if joint_names.len() != joint_positions.len() {
            return Err(crate::error::GltfError::InvalidInput(
                "Joint names and positions must have the same length".to_string()
            ));
        }

        let mut joint_indices = Vec::new();

        for (i, (name, position)) in joint_names.iter().zip(joint_positions.iter()).enumerate() {
            let joint_index = self.add_joint_node(
                Some(name.clone()),
                Some(*position),
                None, // No rotation
                None, // No scale
            );

            // Add this joint as a child of the previous joint (except for the first one)
            if i > 0 {
                self.add_child_to_node(joint_indices[i - 1], joint_index)?;
            }

            joint_indices.push(joint_index);
        }

        Ok(joint_indices)
    }

    /// Create a skinned mesh primitive with joint weights
    ///
    /// This method creates a mesh primitive that includes joint and weight attributes
    /// for skeletal animation.
    ///
    /// # Arguments
    ///
    /// * `name` - Optional name for the mesh
    /// * `positions` - Vertex positions (3 floats per vertex)
    /// * `indices` - Triangle indices
    /// * `normals` - Optional vertex normals
    /// * `texcoords` - Optional texture coordinates
    /// * `joints` - Joint indices for each vertex (4 joints per vertex)
    /// * `weights` - Joint weights for each vertex (4 weights per vertex, should sum to 1.0)
    /// * `material` - Optional material index
    ///
    /// # Returns
    ///
    /// The index of the created mesh
    pub fn create_skinned_mesh(
        &mut self,
        name: Option<String>,
        positions: &[f32],
        indices: &[u16],
        normals: Option<&[f32]>,
        texcoords: Option<&[f32]>,
        joints: &[[u16; 4]],
        weights: &[[f32; 4]],
        material: Option<usize>,
    ) -> Result<usize> {
        use crate::models::Primitive;
        use std::collections::HashMap;

        // Validate input data
        let vertex_count = positions.len() / 3;
        if joints.len() != vertex_count || weights.len() != vertex_count {
            return Err(crate::error::GltfError::InvalidInput(
                "Joints and weights must match vertex count".to_string()
            ));
        }

        // Create position accessor
        let mut pos_bytes = Vec::new();
        for &value in positions {
            pos_bytes.extend_from_slice(&value.to_le_bytes());
        }
        let (pos_offset, pos_length) = self.add_buffer_data(&pos_bytes);
        let pos_buffer_view = self.add_buffer_view(pos_offset, pos_length, Some(34962)); // ARRAY_BUFFER
        let pos_accessor = self.add_accessor(
            pos_buffer_view,
            5126, // FLOAT
            vertex_count,
            "VEC3".to_string(),
            None,
            None,
            None,
        );

        // Create indices accessor
        let mut idx_bytes = Vec::new();
        for &value in indices {
            idx_bytes.extend_from_slice(&value.to_le_bytes());
        }
        let (idx_offset, idx_length) = self.add_buffer_data(&idx_bytes);
        let idx_buffer_view = self.add_buffer_view(idx_offset, idx_length, Some(34963)); // ELEMENT_ARRAY_BUFFER
        let idx_accessor = self.add_accessor(
            idx_buffer_view,
            5123, // UNSIGNED_SHORT
            indices.len(),
            "SCALAR".to_string(),
            None,
            None,
            None,
        );

        // Create joints accessor (as UNSIGNED_SHORT)
        let mut joints_bytes = Vec::new();
        for joint_set in joints {
            for &joint in joint_set {
                joints_bytes.extend_from_slice(&joint.to_le_bytes());
            }
        }
        let (joints_offset, joints_length) = self.add_buffer_data(&joints_bytes);
        let joints_buffer_view = self.add_buffer_view(joints_offset, joints_length, Some(34962)); // ARRAY_BUFFER
        let joints_accessor = self.add_accessor(
            joints_buffer_view,
            5123, // UNSIGNED_SHORT
            vertex_count,
            "VEC4".to_string(),
            None,
            None,
            None,
        );

        // Create weights accessor
        let mut weights_bytes = Vec::new();
        for weight_set in weights {
            for &weight in weight_set {
                weights_bytes.extend_from_slice(&weight.to_le_bytes());
            }
        }
        let (weights_offset, weights_length) = self.add_buffer_data(&weights_bytes);
        let weights_buffer_view = self.add_buffer_view(weights_offset, weights_length, Some(34962)); // ARRAY_BUFFER
        let weights_accessor = self.add_accessor(
            weights_buffer_view,
            5126, // FLOAT
            vertex_count,
            "VEC4".to_string(),
            None,
            None,
            None,
        );

        // Create normals accessor if provided
        let normal_accessor = if let Some(normals) = normals {
            let mut norm_bytes = Vec::new();
            for &value in normals {
                norm_bytes.extend_from_slice(&value.to_le_bytes());
            }
            let (norm_offset, norm_length) = self.add_buffer_data(&norm_bytes);
            let norm_buffer_view = self.add_buffer_view(norm_offset, norm_length, Some(34962)); // ARRAY_BUFFER
            Some(self.add_accessor(
                norm_buffer_view,
                5126, // FLOAT
                vertex_count,
                "VEC3".to_string(),
                None,
                None,
                None,
            ))
        } else {
            None
        };

        // Create texcoord accessor if provided
        let texcoord_accessor = if let Some(texcoords) = texcoords {
            let mut tex_bytes = Vec::new();
            for &value in texcoords {
                tex_bytes.extend_from_slice(&value.to_le_bytes());
            }
            let (tex_offset, tex_length) = self.add_buffer_data(&tex_bytes);
            let tex_buffer_view = self.add_buffer_view(tex_offset, tex_length, Some(34962)); // ARRAY_BUFFER
            Some(self.add_accessor(
                tex_buffer_view,
                5126, // FLOAT
                vertex_count,
                "VEC2".to_string(),
                None,
                None,
                None,
            ))
        } else {
            None
        };

        // Build attributes
        let mut attributes = HashMap::new();
        attributes.insert("POSITION".to_string(), pos_accessor);
        if let Some(normal_acc) = normal_accessor {
            attributes.insert("NORMAL".to_string(), normal_acc);
        }
        if let Some(tex_acc) = texcoord_accessor {
            attributes.insert("TEXCOORD_0".to_string(), tex_acc);
        }
        attributes.insert("JOINTS_0".to_string(), joints_accessor);
        attributes.insert("WEIGHTS_0".to_string(), weights_accessor);

        // Create primitive
        let primitive = Primitive {
            attributes,
            indices: Some(idx_accessor),
            material,
            mode: Some(4), // TRIANGLES
        };

        // Create mesh
        let mesh = crate::models::Mesh {
            name: name.clone(),
            primitives: vec![primitive],
        };

        // Add mesh to document
        if let Some(ref mut meshes) = self.gltf.meshes {
            meshes.push(mesh);
            Ok(meshes.len() - 1)
        } else {
            self.gltf.meshes = Some(vec![mesh]);
            Ok(0)
        }
    }
}
