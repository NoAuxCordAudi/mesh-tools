//! # GltfBuilder: Main Builder Interface
//!
//! This module provides the main `GltfBuilder` struct which serves as the primary interface
//! for creating and exporting glTF/GLB files. It handles the construction of the complete
//! glTF document including scenes, nodes, meshes, materials, and binary data.
//!
//! The builder follows a fluent API pattern where methods can be chained together to
//! build up the document structure incrementally.

use std::fs::File;
use std::io::{self, Write, Seek};
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
pub struct GltfBuilder {
    pub gltf: Gltf,
    pub buffer_data: Vec<u8>,
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
        gltf.animations = Some(Vec::new());
        gltf.samplers = Some(Vec::new());
        gltf.skins = Some(Vec::new());
        
        // Create initial buffer in buffer list
        let buffer = Buffer {
            byte_length: 0, // Will be updated during export
            uri: None,     // Will be embedded in GLB
        };
        
        if let Some(buffers) = &mut gltf.buffers {
            buffers.push(buffer);
        }
        
        GltfBuilder {
            gltf,
            buffer_data: Vec::new(),
        }
    }

    /// Add a scene to the glTF document
    pub fn add_scene(&mut self, name: Option<String>, nodes: Option<Vec<usize>>) -> usize {
        let scene = Scene {
            name,
            nodes,
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

    /// Add a node to the glTF document
    pub fn add_node(&mut self, name: Option<String>, mesh: Option<usize>, 
                   translation: Option<[f32; 3]>, rotation: Option<[f32; 4]>,
                   scale: Option<[f32; 3]>) -> usize {
        let node = Node {
            name,
            mesh,
            skin: None,
            children: None,
            translation,
            rotation,
            scale,
            matrix: None,
        };
        
        if let Some(nodes) = &mut self.gltf.nodes {
            let index = nodes.len();
            nodes.push(node);
            index
        } else {
            self.gltf.nodes = Some(vec![node]);
            0
        }
    }

    /// Add a node with a list of children to the glTF document
    pub fn add_node_with_children(&mut self, name: Option<String>, mesh: Option<usize>, 
                          translation: Option<[f32; 3]>, rotation: Option<[f32; 4]>,
                          scale: Option<[f32; 3]>, children: Vec<usize>) -> usize {
        let node = Node {
            name,
            mesh,
            skin: None,
            children: Some(children),
            translation,
            rotation,
            scale,
            matrix: None,
        };
        
        if let Some(nodes) = &mut self.gltf.nodes {
            let index = nodes.len();
            nodes.push(node);
            index
        } else {
            self.gltf.nodes = Some(vec![node]);
            0
        }
    }

    /// Add a child to an existing node
    pub fn add_child_to_node(&mut self, parent_index: usize, child_index: usize) -> Result<()> {
        if let Some(nodes) = &mut self.gltf.nodes {
            if parent_index < nodes.len() && child_index < nodes.len() {
                let parent = &mut nodes[parent_index];
                
                if let Some(children) = &mut parent.children {
                    if !children.contains(&child_index) {
                        children.push(child_index);
                    }
                } else {
                    parent.children = Some(vec![child_index]);
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
                               child_indices: Vec<usize>) -> usize {
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
    
    /// Add an accessor to the glTF document
    pub(crate) fn add_accessor(&mut self, buffer_view: usize, component_type: usize, 
                       count: usize, type_: String, byte_offset: Option<usize>,
                       min: Option<Vec<f32>>, max: Option<Vec<f32>>) -> usize {
        let accessor = Accessor {
            buffer_view: buffer_view,
            component_type: component_type,
            count,
            type_,
            byte_offset: byte_offset,
            min,
            max,
            normalized: None,
        };
        
        if let Some(accessors) = &mut self.gltf.accessors {
            let index = accessors.len();
            accessors.push(accessor);
            index
        } else {
            self.gltf.accessors = Some(vec![accessor]);
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
        
        // Update the buffer size in the glTF model
        if let Some(buffers) = &mut self.gltf.buffers {
            if !buffers.is_empty() {
                buffers[0].byte_length = self.buffer_data.len();
            }
        }
        
        (byte_offset, byte_length)
    }

    /// Export the glTF as a GLB file
    pub fn export_glb(&self, path: &str) -> Result<()> {
        let mut file = File::create(path)?;
        
        // Write GLB header (magic, version, length)
        file.write_all(b"glTF")?;
        file.write_u32::<LittleEndian>(2)?; // version
        
        // We'll write the total length later, once we know it
        let length_pos = file.stream_position()?;
        file.write_u32::<LittleEndian>(0)?; // placeholder total length
        
        // JSON chunk
        let json = serde_json::to_string(&self.gltf)?;
        let json_len = json.len();
        let json_pad = (4 - (json_len % 4)) % 4; // Padding to 4-byte boundary
        
        file.write_u32::<LittleEndian>((json_len + json_pad) as u32)?; // chunk length
        file.write_u32::<LittleEndian>(0x4E4F534A)?; // chunk type "JSON"
        file.write_all(json.as_bytes())?;
        
        // Add padding
        for _ in 0..json_pad {
            file.write_u8(0x20)?; // Space character for padding
        }
        
        // BIN chunk
        if !self.buffer_data.is_empty() {
            let bin_len = self.buffer_data.len();
            let bin_pad = (4 - (bin_len % 4)) % 4; // Padding to 4-byte boundary
            
            file.write_u32::<LittleEndian>((bin_len + bin_pad) as u32)?; // chunk length
            file.write_u32::<LittleEndian>(0x004E4942)?; // chunk type "BIN"
            file.write_all(&self.buffer_data)?;
            
            // Add padding
            for _ in 0..bin_pad {
                file.write_u8(0)?;
            }
        }
        
        // Go back and write the total length
        let current_pos = file.stream_position()?;
        file.seek(io::SeekFrom::Start(length_pos))?;
        file.write_u32::<LittleEndian>(current_pos as u32)?;
        
        Ok(())
    }
}
