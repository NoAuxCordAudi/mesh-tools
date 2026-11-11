//! # GltfBuilder: Main Builder Interface
//!
//! This module provides the main `GltfBuilder` struct which serves as the primary interface
//! for creating and exporting glTF/GLB files. It handles the construction of the complete
//! glTF document including scenes, nodes, meshes, materials, and binary data.
//!
//! The builder follows a fluent API pattern where methods can be chained together to
//! build up the document structure incrementally.

use byteorder::{LittleEndian, WriteBytesExt};
use serde_json;

use crate::error::{GltfError, Result};
use crate::models::*;

/// The main builder for creating and exporting glTF models
///
/// `GltfBuilder` provides methods for:
/// - Creating primitive shapes (box, sphere, plane, etc.)
/// - Creating and managing materials
/// - Creating scene hierarchies with nodes
/// - Managing buffer data for vertex attributes
/// - Exporting to glTF and GLB formats
///
/// # Safety Features
/// - Prevents NODE_PARENT_OVERRIDE errors through safe hierarchy building
/// - Automatically normalizes rotation quaternions
/// - Provides instancing helpers for common patterns
/// - Validates node relationships before building
pub struct GltfBuilder {
    /// Internal glTF document structure
    /// 
    /// ⚠️  **WARNING: Direct manipulation of this field is strongly discouraged!**
    /// 
    /// Directly modifying this structure can lead to:
    /// - NODE_PARENT_OVERRIDE errors (nodes with multiple parents)
    /// - ROTATION_NON_UNIT errors (non-normalized quaternions)  
    /// - Invalid glTF files that fail validation
    /// 
    /// **Always use the provided API methods instead:**
    /// - `add_node()`, `add_child_to_node()`, `create_mesh_instance()`
    /// - `create_mesh_grid()`, `create_safe_hierarchy()`
    /// - `normalize_quaternion()`, `quaternion_from_euler()`
    /// 
    /// This field is public only for internal crate modules. External users
    /// should treat this as read-only for inspection purposes only.
    #[doc(hidden)]  // Hide from public documentation
    pub gltf: Gltf,
    
    pub(crate) buffer_data: Vec<u8>,
    // Track node parent relationships to prevent override errors
    pub(crate) node_parents: Vec<Option<usize>>,
}

impl GltfBuilder {
    /// Create a new glTF builder
    pub fn new() -> Self {
        // Create default minimal glTF with required asset info
        let mut gltf = Gltf::default();
        
        // Asset information is required by the glTF spec
        let asset = Asset {
            version: "2.0".to_string(),
            generator: Some("Rust glTF Export Library".to_string()),
            copyright: None,
            ..Default::default()
        };
        
        gltf.asset = asset;
        
        // Initialize empty collections
        gltf.scenes = Some(Vec::new());
        gltf.nodes = Some(Vec::new());
        gltf.meshes = Some(Vec::new());
        gltf.accessors = Some(Vec::new());
        gltf.buffer_views = Some(Vec::new());
        gltf.buffers = Some(Vec::new());
        gltf.materials = Some(Vec::new());
        gltf.textures = Some(Vec::new());
        gltf.images = Some(Vec::new());
        // `animations` stays None until the first animation is added (an empty array is invalid glTF)
        gltf.samplers = Some(Vec::new());
        gltf.skins = Some(Vec::new());
        
        // Buffer 0 is created lazily by the first data write (an empty buffer is invalid glTF)
        
        GltfBuilder {
            gltf,
            buffer_data: Vec::new(),
            node_parents: Vec::new(),
        }
    }

    /// Add a scene to the glTF document
    pub fn add_scene(&mut self, name: Option<String>, nodes: Option<Vec<usize>>) -> usize {
        let scene = Scene {
            name,
            nodes,
            ..Default::default()
        };
        
        if let Some(scenes) = &mut self.gltf.scenes {
            let index = scenes.len();
            scenes.push(scene);
            
            // Set as default scene if this is the first one
            if self.gltf.scene.is_none() {
                self.gltf.scene = Some(0);
            }
            
            index
        } else {
            self.gltf.scenes = Some(vec![scene]);
            self.gltf.scene = Some(0);
            0
        }
    }

    /// Add a node to the glTF document with automatic quaternion normalization
    pub fn add_node(&mut self, name: Option<String>, mesh: Option<usize>, 
                   translation: Option<[f32; 3]>, rotation: Option<[f32; 4]>,
                   scale: Option<[f32; 3]>) -> usize {
        // Normalize rotation quaternion if provided
        let normalized_rotation = rotation.map(|q| Self::normalize_quaternion(q));
        
        let node = Node {
            name,
            mesh,
            skin: None,
            children: None,
            translation,
            rotation: normalized_rotation,
            scale,
            matrix: None,
            ..Default::default()
        };
        
        let index = if let Some(nodes) = &mut self.gltf.nodes {
            let index = nodes.len();
            nodes.push(node);
            index
        } else {
            self.gltf.nodes = Some(vec![node]);
            0
        };
        
        // Track that this node has no parent initially
        self.node_parents.push(None);
        
        index
    }

    /// Add a node with a list of children to the glTF document
    /// This method validates that children don't already have parents to prevent NODE_PARENT_OVERRIDE errors
    pub fn add_node_with_children(&mut self, name: Option<String>, mesh: Option<usize>, 
                          translation: Option<[f32; 3]>, rotation: Option<[f32; 4]>,
                          scale: Option<[f32; 3]>, children: Vec<usize>) -> Result<usize> {
        // Validate that none of the children already have parents
        for &child_index in &children {
            if child_index >= self.node_parents.len() {
                return Err(GltfError::InvalidIndex);
            }
            if self.node_parents[child_index].is_some() {
                return Err(GltfError::InvalidData(
                    format!("Node {} already has a parent. Cannot add as child to avoid NODE_PARENT_OVERRIDE error.", child_index)
                ));
            }
        }
        
        // Normalize rotation quaternion if provided
        let normalized_rotation = rotation.map(|q| Self::normalize_quaternion(q));
        
        let node = Node {
            name,
            mesh,
            skin: None,
            children: Some(children.clone()),
            translation,
            rotation: normalized_rotation,
            scale,
            matrix: None,
            ..Default::default()
        };
        
        let index = if let Some(nodes) = &mut self.gltf.nodes {
            let index = nodes.len();
            nodes.push(node);
            index
        } else {
            self.gltf.nodes = Some(vec![node]);
            0
        };
        
        // Track that this node has no parent initially
        self.node_parents.push(None);
        
        // Update parent tracking for all children
        for child_index in children {
            self.node_parents[child_index] = Some(index);
        }
        
        Ok(index)
    }

    /// Add a child to an existing node with parent validation
    /// Prevents NODE_PARENT_OVERRIDE errors by checking if child already has a parent
    pub fn add_child_to_node(&mut self, parent_index: usize, child_index: usize) -> Result<()> {
        // Validate indices
        if parent_index >= self.node_parents.len() || child_index >= self.node_parents.len() {
            return Err(GltfError::InvalidIndex);
        }
        
        // Check if child already has a parent
        if self.node_parents[child_index].is_some() {
            return Err(GltfError::InvalidData(
                format!("Node {} already has a parent. Cannot add as child to avoid NODE_PARENT_OVERRIDE error.", child_index)
            ));
        }
        
        if let Some(nodes) = &mut self.gltf.nodes {
            if parent_index < nodes.len() && child_index < nodes.len() {
                let parent = &mut nodes[parent_index];
                
                if let Some(children) = &mut parent.children {
                    if !children.contains(&child_index) {
                        children.push(child_index);
                        // Update parent tracking
                        self.node_parents[child_index] = Some(parent_index);
                    }
                } else {
                    parent.children = Some(vec![child_index]);
                    // Update parent tracking
                    self.node_parents[child_index] = Some(parent_index);
                }
                
                Ok(())
            } else {
                Err(GltfError::InvalidIndex)
            }
        } else {
            Err(GltfError::InvalidData("No nodes in document".to_string()))
        }
    }

    /// Create a parent node with multiple child nodes
    pub fn create_node_hierarchy(&mut self, parent_name: Option<String>, 
                               parent_translation: Option<[f32; 3]>,
                               parent_rotation: Option<[f32; 4]>,
                               parent_scale: Option<[f32; 3]>,
                               child_indices: Vec<usize>) -> Result<usize> {
        // Create the parent node with the children
        self.add_node_with_children(
            parent_name,
            None, // No mesh on the parent
            parent_translation,
            parent_rotation,
            parent_scale,
            child_indices
        )
    }

    /// Add a mesh to the glTF document
    pub fn add_mesh(&mut self, name: Option<String>, primitives: Vec<Primitive>) -> usize {
        let mesh = Mesh {
            name,
            primitives,
            ..Default::default()
        };
        
        if let Some(meshes) = &mut self.gltf.meshes {
            let index = meshes.len();
            meshes.push(mesh);
            index
        } else {
            self.gltf.meshes = Some(vec![mesh]);
            0
        }
    }
    
    /// Add a buffer view to the glTF document
    pub(crate) fn add_buffer_view(&mut self, byte_offset: usize, byte_length: usize, 
                          target: Option<usize>) -> usize {
        let buffer_view = BufferView {
            buffer: 0, // We only use a single buffer
            byte_offset: byte_offset,
            byte_length: byte_length,
            byte_stride: None,
            target,
            ..Default::default()
        };
        
        if let Some(buffer_views) = &mut self.gltf.buffer_views {
            let index = buffer_views.len();
            buffer_views.push(buffer_view);
            index
        } else {
            self.gltf.buffer_views = Some(vec![buffer_view]);
            0
        }
    }
    
    /// Add binary data to the buffer and return the byte offset
    pub(crate) fn add_buffer_data(&mut self, data: &[u8]) -> (usize, usize) {
        // Ensure alignment to 4-byte boundary
        while self.buffer_data.len() % 4 != 0 {
            self.buffer_data.push(0);
        }
        
        let byte_offset = self.buffer_data.len();
        let byte_length = data.len();
        
        self.buffer_data.extend_from_slice(data);
        
        // Create buffer 0 on first use and keep its length in step with the data
        let buffers = self.gltf.buffers.get_or_insert_with(Vec::new);
        if buffers.is_empty() {
            buffers.push(Buffer::default());
        }
        buffers[0].byte_length = self.buffer_data.len();
        
        (byte_offset, byte_length)
    }

    /// Set or clear the `name` of buffer `index`.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidData` if the buffer does not exist. The document is unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// assert!(b.set_buffer_name(0, Some("geometry".into())).is_err()); // no buffer yet
    /// b.create_box(1.0);
    /// b.set_buffer_name(0, Some("geometry".into())).unwrap();
    /// ```
    pub fn set_buffer_name(&mut self, index: usize, name: Option<String>) -> Result<()> {
        let n = self.gltf.buffers.as_ref().map_or(0, |v| v.len());
        let item = self.gltf.buffers.as_mut().and_then(|v| v.get_mut(index));
        item.ok_or_else(|| GltfError::InvalidData(format!("buffer {index} does not exist ({n} present)")))?.name = name;
        Ok(())
    }

    /// Set or clear the `name` of buffer view `index`.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidData` if the buffer view does not exist. The document is unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// b.create_box(1.0);
    /// b.set_buffer_view_name(0, Some("positions".into())).unwrap();
    /// assert!(b.set_buffer_view_name(999, None).is_err());
    /// ```
    pub fn set_buffer_view_name(&mut self, index: usize, name: Option<String>) -> Result<()> {
        let n = self.gltf.buffer_views.as_ref().map_or(0, |v| v.len());
        let item = self.gltf.buffer_views.as_mut().and_then(|v| v.get_mut(index));
        item.ok_or_else(|| GltfError::InvalidData(format!("buffer view {index} does not exist ({n} present)")))?.name = name;
        Ok(())
    }

    /// Set or clear the `name` of accessor `index`.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidData` if the accessor does not exist. The document is unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// b.create_box(1.0);
    /// b.set_accessor_name(0, Some("POSITION".into())).unwrap();
    /// assert!(b.set_accessor_name(999, None).is_err());
    /// ```
    pub fn set_accessor_name(&mut self, index: usize, name: Option<String>) -> Result<()> {
        let n = self.gltf.accessors.as_ref().map_or(0, |v| v.len());
        let item = self.gltf.accessors.as_mut().and_then(|v| v.get_mut(index));
        item.ok_or_else(|| GltfError::InvalidData(format!("accessor {index} does not exist ({n} present)")))?.name = name;
        Ok(())
    }

    /// Validate the document, then serialize it to GLB bytes.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidData` listing every validation error (see
    /// [`validate`](Self::validate)), or if the GLB would exceed the 4 GiB container limit.
    /// Use [`to_glb_bytes_unchecked`](Self::to_glb_bytes_unchecked) to skip validation.
    pub fn to_glb_bytes(&self) -> Result<Vec<u8>> {
        let report = self.validate();
        if !report.errors.is_empty() {
            return Err(GltfError::InvalidData(format!(
                "document failed validation with {} error(s):\n  - {}",
                report.errors.len(),
                report.errors.join("\n  - ")
            )));
        }
        self.to_glb_bytes_unchecked()
    }

    /// Serialize to GLB bytes **without** validating the document.
    ///
    /// Empty top-level arrays (which glTF forbids) are omitted, except `extensionsUsed` and
    /// `extensionsRequired`, which are kept so the problem stays visible. The result may be an invalid
    /// glTF file; this exists for deliberately producing or inspecting such documents.
    pub fn to_glb_bytes_unchecked(&self) -> Result<Vec<u8>> {
        let too_large = || GltfError::InvalidData("GLB exceeds the 4 GiB container limit".to_string());

        let mut value = serde_json::to_value(&self.gltf)?;
        if let Some(obj) = value.as_object_mut() {
            obj.retain(|k, v| {
                k == "extensionsUsed" || k == "extensionsRequired" || !matches!(v, serde_json::Value::Array(a) if a.is_empty())
            });
        }
        crate::builder_accessor::write_integer_bounds_as_integers(&mut value);
        let json = serde_json::to_string(&value)?;
        let json_pad = (4 - (json.len() % 4)) % 4;
        let json_chunk_len = u32::try_from(json.len() + json_pad).map_err(|_| too_large())?;

        let mut out = Vec::with_capacity(12 + 8 + json.len() + json_pad + 8 + self.buffer_data.len() + 3);
        out.extend_from_slice(b"glTF");
        out.write_u32::<LittleEndian>(2)?; // version
        out.write_u32::<LittleEndian>(0)?; // total length, patched below

        out.write_u32::<LittleEndian>(json_chunk_len)?;
        out.write_u32::<LittleEndian>(0x4E4F534A)?; // "JSON"
        out.extend_from_slice(json.as_bytes());
        out.resize(out.len() + json_pad, 0x20); // pad with spaces

        if !self.buffer_data.is_empty() {
            let bin_pad = (4 - (self.buffer_data.len() % 4)) % 4;
            let bin_chunk_len = u32::try_from(self.buffer_data.len() + bin_pad).map_err(|_| too_large())?;
            out.write_u32::<LittleEndian>(bin_chunk_len)?;
            out.write_u32::<LittleEndian>(0x004E4942)?; // "BIN\0"
            out.extend_from_slice(&self.buffer_data);
            out.resize(out.len() + bin_pad, 0);
        }

        let total = u32::try_from(out.len()).map_err(|_| too_large())?;
        out[8..12].copy_from_slice(&total.to_le_bytes());
        Ok(out)
    }

    /// Validate the document and export it as a GLB file.
    ///
    /// Nothing is written if validation fails. See [`to_glb_bytes`](Self::to_glb_bytes).
    pub fn export_glb(&self, path: &str) -> Result<()> {
        // Serialize fully first so a failure never leaves a truncated file behind.
        let bytes = self.to_glb_bytes()?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Export as a GLB file **without** validating. See
    /// [`to_glb_bytes_unchecked`](Self::to_glb_bytes_unchecked).
    pub fn export_glb_unchecked(&self, path: &str) -> Result<()> {
        let bytes = self.to_glb_bytes_unchecked()?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    // ========== HELPER METHODS ==========

    /// Normalize a quaternion to ensure it's a unit quaternion
    /// This prevents ROTATION_NON_UNIT glTF validation errors
    pub fn normalize_quaternion(quat: [f32; 4]) -> [f32; 4] {
        let magnitude = (quat[0]*quat[0] + quat[1]*quat[1] + quat[2]*quat[2] + quat[3]*quat[3]).sqrt();
        if magnitude > 0.0 {
            [quat[0]/magnitude, quat[1]/magnitude, quat[2]/magnitude, quat[3]/magnitude]
        } else {
            [0.0, 0.0, 0.0, 1.0] // Default identity quaternion
        }
    }

    /// Create a rotation quaternion from Euler angles (in radians)
    /// Returns a normalized quaternion ready for use in nodes
    pub fn quaternion_from_euler(x: f32, y: f32, z: f32) -> [f32; 4] {
        let (sx, cx) = (x * 0.5).sin_cos();
        let (sy, cy) = (y * 0.5).sin_cos();
        let (sz, cz) = (z * 0.5).sin_cos();

        let quat = [
            sx * cy * cz - cx * sy * sz, // x
            cx * sy * cz + sx * cy * sz, // y
            cx * cy * sz - sx * sy * cz, // z
            cx * cy * cz + sx * sy * sz, // w
        ];

        Self::normalize_quaternion(quat)
    }

    /// Create a safe node instance that references a mesh at a specific position
    /// This is the preferred way to create mesh instances to avoid parent override errors
    pub fn create_mesh_instance(&mut self, name: Option<String>, mesh_index: usize,
                               position: [f32; 3], rotation: Option<[f32; 4]>,
                               scale: Option<[f32; 3]>) -> usize {
        self.add_node(name, Some(mesh_index), Some(position), rotation, scale)
    }

    /// Create multiple instances of a mesh in a grid pattern
    /// This is a helper for common instancing patterns
    pub fn create_mesh_grid(&mut self, mesh_index: usize, name_prefix: &str,
                           rows: usize, cols: usize, spacing: f32) -> Vec<usize> {
        let mut instances = Vec::new();
        
        for row in 0..rows {
            for col in 0..cols {
                let x = (col as f32 - (cols as f32 - 1.0) / 2.0) * spacing;
                let z = (row as f32 - (rows as f32 - 1.0) / 2.0) * spacing;
                
                let instance = self.create_mesh_instance(
                    Some(format!("{}_{}", name_prefix, row * cols + col)),
                    mesh_index,
                    [x, 0.0, z],
                    None,
                    None
                );
                
                instances.push(instance);
            }
        }
        
        instances
    }

    /// Create a hierarchical group of nodes safely
    /// This method ensures no parent override errors by creating new instances
    pub fn create_safe_hierarchy(&mut self, name: Option<String>,
                                position: Option<[f32; 3]>,
                                rotation: Option<[f32; 4]>,
                                scale: Option<[f32; 3]>,
                                child_meshes: Vec<(usize, [f32; 3])>) -> Result<usize> {
        // Create child nodes first
        let mut child_nodes = Vec::new();
        
        for (i, (mesh_index, child_pos)) in child_meshes.iter().enumerate() {
            let child_name = name.as_ref().map(|n| format!("{}_child_{}", n, i));
            let child_node = self.create_mesh_instance(
                child_name,
                *mesh_index,
                *child_pos,
                None,
                None
            );
            child_nodes.push(child_node);
        }
        
        // Create parent node with children
        self.add_node_with_children(name, None, position, rotation, scale, child_nodes)
    }

    /// Validate the current node hierarchy for common glTF errors
    /// Returns a list of validation warnings/errors
    pub fn validate_hierarchy(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        
        // Check for nodes with multiple parents (should be impossible with our API now)
        let mut parent_counts = vec![0; self.node_parents.len()];
        for parent_opt in &self.node_parents {
            if let Some(parent) = parent_opt {
                if *parent < parent_counts.len() {
                    parent_counts[*parent] += 1;
                }
            }
        }
        
        // Check for orphaned nodes (nodes not in any scene)
        if let Some(scenes) = &self.gltf.scenes {
            let mut nodes_in_scenes = std::collections::HashSet::new();
            for scene in scenes {
                if let Some(scene_nodes) = &scene.nodes {
                    for &node_index in scene_nodes {
                        self.collect_node_tree(node_index, &mut nodes_in_scenes);
                    }
                }
            }
            
            if let Some(nodes) = &self.gltf.nodes {
                for (i, _) in nodes.iter().enumerate() {
                    if !nodes_in_scenes.contains(&i) {
                        warnings.push(format!("Node {} is not referenced by any scene", i));
                    }
                }
            }
        }
        
        warnings
    }
    
    /// Helper method to recursively collect all nodes in a tree
    fn collect_node_tree(&self, node_index: usize, collected: &mut std::collections::HashSet<usize>) {
        if collected.contains(&node_index) {
            return;
        }
        
        collected.insert(node_index);
        
        if let Some(nodes) = &self.gltf.nodes {
            if let Some(node) = nodes.get(node_index) {
                if let Some(children) = &node.children {
                    for &child in children {
                        self.collect_node_tree(child, collected);
                    }
                }
            }
        }
    }

    // ========== SAFE READ-ONLY ACCESS METHODS ==========

    /// Get the number of nodes in the document
    pub fn node_count(&self) -> usize {
        self.gltf.nodes.as_ref().map_or(0, |nodes| nodes.len())
    }

    /// Get the number of meshes in the document  
    pub fn mesh_count(&self) -> usize {
        self.gltf.meshes.as_ref().map_or(0, |meshes| meshes.len())
    }

    /// Get the number of materials in the document
    pub fn material_count(&self) -> usize {
        self.gltf.materials.as_ref().map_or(0, |materials| materials.len())
    }

    /// Get the number of scenes in the document
    pub fn scene_count(&self) -> usize {
        self.gltf.scenes.as_ref().map_or(0, |scenes| scenes.len())
    }

    /// Check if a node has a parent (safe way to check parent relationships)
    pub fn node_has_parent(&self, node_index: usize) -> bool {
        if node_index < self.node_parents.len() {
            self.node_parents[node_index].is_some()
        } else {
            false
        }
    }

    /// Get the parent of a node (if any)
    pub fn get_node_parent(&self, node_index: usize) -> Option<usize> {
        if node_index < self.node_parents.len() {
            self.node_parents[node_index]
        } else {
            None
        }
    }

    /// Get a read-only reference to the internal glTF structure for inspection
    /// 
    /// ⚠️  **This is for READ-ONLY inspection only!**
    /// Do not modify the returned structure - use the API methods instead.
    pub fn inspect_gltf(&self) -> &Gltf {
        &self.gltf
    }
}
