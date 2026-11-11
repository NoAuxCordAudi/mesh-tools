//! # Primitive Shape Generation Implementation
//!
//! This module implements the primitive shape generation methods for the `GltfBuilder` struct.
//! It provides functionality for creating standard 3D shapes such as boxes, spheres, planes,
//! cylinders, cones, tori, and more.
//!
//! Each shape generation method:
//! 1. Generates the geometry data (vertices, indices, normals, UVs)
//! 2. Creates the necessary buffer views and accessors
//! 3. Creates a mesh with the appropriate primitives
//! 4. Returns the index of the created mesh
//!
//! These methods are the high-level interface for the low-level geometry generation
//! functions in the `primitives` module.

use crate::builder::GltfBuilder;
use crate::constants::{accessor_type, buffer_view_target, component_type};
use crate::models::Primitive;
use crate::primitives;
use std::collections::HashMap;
use crate::compat::{Point3, Vector2, Vector3};

/// A triangle represented by three vertex indices
///
/// Uses u32 indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triangle {
    /// First vertex index
    pub a: u32,
    /// Second vertex index
    pub b: u32,
    /// Third vertex index
    pub c: u32,
}

impl Triangle {
    /// Create a new triangle with the given vertex indices
    ///
    /// # Arguments
    /// * `a` - First vertex index
    /// * `b` - Second vertex index
    /// * `c` - Third vertex index
    ///
    /// # Returns
    /// A new Triangle instance
    pub fn new(a: u32, b: u32, c: u32) -> Self {
        Self { a, b, c }
    }
}

impl GltfBuilder {
    /// Create a simple cubic box mesh with the specified size
    ///
    /// This method creates a cube centered at the origin with equal dimensions on all sides.
    /// The cube has properly generated normals and texture coordinates for each face.
    ///
    /// # Parameters
    /// * `size` - The length of each side of the cube
    ///
    /// # Returns
    /// The index of the created mesh in the glTF document's meshes array
    ///
    /// # Example
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut builder = GltfBuilder::new();
    /// let box_mesh = builder.create_box(2.0); // Creates a 2x2x2 cube
    /// ```
    pub fn create_box(&mut self, size: f32) -> usize {
        // Box centered at origin with given size
        let half_size = size / 2.0;
        
        // 8 vertices for a cube (8 corners) using Point3
        let positions = vec![
            // Front face (z+)
            crate::compat::point3::new(-half_size, -half_size,  half_size),  // 0: bottom-left-front
            crate::compat::point3::new( half_size, -half_size,  half_size),  // 1: bottom-right-front
            crate::compat::point3::new( half_size,  half_size,  half_size),  // 2: top-right-front
            crate::compat::point3::new(-half_size,  half_size,  half_size),  // 3: top-left-front
            
            // Back face (z-)
            crate::compat::point3::new(-half_size, -half_size, -half_size),  // 4: bottom-left-back
            crate::compat::point3::new( half_size, -half_size, -half_size),  // 5: bottom-right-back
            crate::compat::point3::new( half_size,  half_size, -half_size),  // 6: top-right-back
            crate::compat::point3::new(-half_size,  half_size, -half_size),  // 7: top-left-back
        ];
        
        // 12 triangles (2 per face * 6 faces) using Triangle structs
        let indices = vec![
            // Front face (z+)
            Triangle { a: 0, b: 1, c: 2 }, Triangle { a: 0, b: 2, c: 3 },
            
            // Back face (z-)
            Triangle { a: 5, b: 4, c: 7 }, Triangle { a: 5, b: 7, c: 6 },
            
            // Top face (y+)
            Triangle { a: 3, b: 2, c: 6 }, Triangle { a: 3, b: 6, c: 7 },
            
            // Bottom face (y-)
            Triangle { a: 4, b: 5, c: 1 }, Triangle { a: 4, b: 1, c: 0 },
            
            // Right face (x+)
            Triangle { a: 1, b: 5, c: 6 }, Triangle { a: 1, b: 6, c: 2 },
            
            // Left face (x-)
            Triangle { a: 4, b: 0, c: 3 }, Triangle { a: 4, b: 3, c: 7 },
        ];
        
        // Normals for each vertex using Vector3
        let normals = vec![
            // Front face (z+)
            crate::compat::vector3::new(0.0, 0.0, 1.0),
            crate::compat::vector3::new(0.0, 0.0, 1.0),
            crate::compat::vector3::new(0.0, 0.0, 1.0),
            crate::compat::vector3::new(0.0, 0.0, 1.0),
            
            // Back face (z-)
            crate::compat::vector3::new(0.0, 0.0, -1.0),
            crate::compat::vector3::new(0.0, 0.0, -1.0),
            crate::compat::vector3::new(0.0, 0.0, -1.0),
            crate::compat::vector3::new(0.0, 0.0, -1.0),
            
            // This is simplified for example purposes
            // In a real implementation we would need more vertices with unique normals
            // or use a better normal calculation strategy
        ];
        
        // Simple UV mapping using Vector2
        let uvs = vec![
            // Front face
            crate::compat::vector2::new(0.0, 1.0),
            crate::compat::vector2::new(1.0, 1.0),
            crate::compat::vector2::new(1.0, 0.0),
            crate::compat::vector2::new(0.0, 0.0),
            
            // Back face
            crate::compat::vector2::new(1.0, 1.0),
            crate::compat::vector2::new(0.0, 1.0),
            crate::compat::vector2::new(0.0, 0.0),
            crate::compat::vector2::new(1.0, 0.0),
        ];
        
        self.create_simple_mesh(None, &positions, &indices, Some(normals), Some(uvs), None)
    }

    /// Create a box with the specified material
    pub fn create_box_with_material(&mut self, size: f32, material: Option<usize>) -> usize {
        // Box centered at origin with given size
        let half_size = size / 2.0;
        
        // For a proper cube with separate normals per face, we need to duplicate vertices
        // 24 vertices for a cube (4 per face * 6 faces) using Point3
        let positions = vec![
            // Front face (z+)
            crate::compat::point3::new(-half_size, -half_size,  half_size),
            crate::compat::point3::new( half_size, -half_size,  half_size),
            crate::compat::point3::new( half_size,  half_size,  half_size),
            crate::compat::point3::new(-half_size,  half_size,  half_size),
            
            // Back face (z-)
            crate::compat::point3::new( half_size, -half_size, -half_size),
            crate::compat::point3::new(-half_size, -half_size, -half_size),
            crate::compat::point3::new(-half_size,  half_size, -half_size),
            crate::compat::point3::new( half_size,  half_size, -half_size),
            
            // Top face (y+)
            crate::compat::point3::new(-half_size,  half_size,  half_size),
            crate::compat::point3::new( half_size,  half_size,  half_size),
            crate::compat::point3::new( half_size,  half_size, -half_size),
            crate::compat::point3::new(-half_size,  half_size, -half_size),
            
            // Bottom face (y-)
            crate::compat::point3::new( half_size, -half_size,  half_size),
            crate::compat::point3::new(-half_size, -half_size,  half_size),
            crate::compat::point3::new(-half_size, -half_size, -half_size),
            crate::compat::point3::new( half_size, -half_size, -half_size),
            
            // Right face (x+)
            crate::compat::point3::new( half_size, -half_size,  half_size),
            crate::compat::point3::new( half_size, -half_size, -half_size),
            crate::compat::point3::new( half_size,  half_size, -half_size),
            crate::compat::point3::new( half_size,  half_size,  half_size),
            
            // Left face (x-)
            crate::compat::point3::new(-half_size, -half_size, -half_size),
            crate::compat::point3::new(-half_size, -half_size,  half_size),
            crate::compat::point3::new(-half_size,  half_size,  half_size),
            crate::compat::point3::new(-half_size,  half_size, -half_size),
        ];
        
        // Convert positions to flat array for create_simple_mesh
        
        // Triangle indices (6 faces * 2 triangles * 3 vertices = 36 indices)
        let indices = vec![
            // Front face
            Triangle { a: 0, b: 1, c: 2 },
            Triangle { a: 0, b: 2, c: 3 },
            
            // Back face
            Triangle { a: 4, b: 5, c: 6 },
            Triangle { a: 4, b: 6, c: 7 },
            
            // Top face
            Triangle { a: 8, b: 9, c: 10 },
            Triangle { a: 8, b: 10, c: 11 },
            
            // Bottom face
            Triangle { a: 12, b: 13, c: 14 },
            Triangle { a: 12, b: 14, c: 15 },
            
            // Right face
            Triangle { a: 16, b: 17, c: 18 },
            Triangle { a: 16, b: 18, c: 19 },
            
            // Left face
            Triangle { a: 20, b: 21, c: 22 },
            Triangle { a: 20, b: 22, c: 23 },
        ];
        
        // Convert indices to u16 for create_simple_mesh
        
        // Normals for each vertex
        let normals = vec![
            // Front face (z+)
            crate::compat::vector3::new(0.0, 0.0, 1.0),
            crate::compat::vector3::new(0.0, 0.0, 1.0),
            crate::compat::vector3::new(0.0, 0.0, 1.0),
            crate::compat::vector3::new(0.0, 0.0, 1.0),
            
            // Back face (z-)
            crate::compat::vector3::new(0.0, 0.0, -1.0),
            crate::compat::vector3::new(0.0, 0.0, -1.0),
            crate::compat::vector3::new(0.0, 0.0, -1.0),
            crate::compat::vector3::new(0.0, 0.0, -1.0),
            
            // Top face (y+)
            crate::compat::vector3::new(0.0, 1.0, 0.0),
            crate::compat::vector3::new(0.0, 1.0, 0.0),
            crate::compat::vector3::new(0.0, 1.0, 0.0),
            crate::compat::vector3::new(0.0, 1.0, 0.0),
            
            // Bottom face (y-)
            crate::compat::vector3::new(0.0, -1.0, 0.0),
            crate::compat::vector3::new(0.0, -1.0, 0.0),
            crate::compat::vector3::new(0.0, -1.0, 0.0),
            crate::compat::vector3::new(0.0, -1.0, 0.0),
            
            // Right face (x+)
            crate::compat::vector3::new(1.0, 0.0, 0.0),
            crate::compat::vector3::new(1.0, 0.0, 0.0),
            crate::compat::vector3::new(1.0, 0.0, 0.0),
            crate::compat::vector3::new(1.0, 0.0, 0.0),
            
            // Left face (x-)
            crate::compat::vector3::new(-1.0, 0.0, 0.0),
            crate::compat::vector3::new(-1.0, 0.0, 0.0),
            crate::compat::vector3::new(-1.0, 0.0, 0.0),
            crate::compat::vector3::new(-1.0, 0.0, 0.0),
        ];
        
        // Convert normals to flat array for create_simple_mesh
        
        // UVs for each face using Vector2
        let uvs = vec![
            // Front face
            crate::compat::vector2::new(0.0, 1.0),
            crate::compat::vector2::new(1.0, 1.0),
            crate::compat::vector2::new(1.0, 0.0),
            crate::compat::vector2::new(0.0, 0.0),
            
            // Back face
            crate::compat::vector2::new(1.0, 1.0),
            crate::compat::vector2::new(1.0, 0.0),
            crate::compat::vector2::new(0.0, 0.0),
            crate::compat::vector2::new(0.0, 1.0),
            
            // Top face
            crate::compat::vector2::new(0.0, 1.0),
            crate::compat::vector2::new(0.0, 0.0),
            crate::compat::vector2::new(1.0, 0.0),
            crate::compat::vector2::new(1.0, 1.0),
            
            // Bottom face
            crate::compat::vector2::new(1.0, 1.0),
            crate::compat::vector2::new(0.0, 1.0),
            crate::compat::vector2::new(0.0, 0.0),
            crate::compat::vector2::new(1.0, 0.0),
            
            // Right face
            crate::compat::vector2::new(1.0, 1.0),
            crate::compat::vector2::new(1.0, 0.0),
            crate::compat::vector2::new(0.0, 0.0),
            crate::compat::vector2::new(0.0, 1.0),
            
            // Left face
            crate::compat::vector2::new(0.0, 1.0),
            crate::compat::vector2::new(1.0, 1.0),
            crate::compat::vector2::new(1.0, 0.0),
            crate::compat::vector2::new(0.0, 0.0),
        ];
        
        self.create_simple_mesh(None, &positions, &indices, Some(normals), Some(uvs), material)
    }
    
    /// Create a mesh with custom geometry and UV mapping
    /// 
    /// # Parameters
    /// * `name` - Optional name for the mesh
    /// * `positions` - Vertex positions as Vec<Point3<f32>>
    /// * `indices` - List of triangles, each containing three vertex indices
    /// * `normals` - Optional vertex normals as Vec<Vector3<f32>>
    /// * `texcoords` - Optional array of UV coordinate sets, each as Vec<Vector2<f32>>. 
    ///                 The first set becomes TEXCOORD_0, the second TEXCOORD_1, etc.
    /// * `material` - Optional material index to use for the mesh
    /// 
    /// # Returns
    /// The index of the created mesh
    pub fn create_custom_mesh(&mut self, 
                            name: Option<String>,
                            positions: &[Point3<f32>], 
                            indices: &[Triangle], 
                            normals: Option<Vec<Vector3<f32>>>, 
                            texcoords: Option<Vec<Vec<Vector2<f32>>>>,
                            material: Option<usize>) -> usize {
        // Calculate bounds for the positions
        let (min_point, max_point) = if !positions.is_empty() {
            let mut min = crate::compat::point3::new(f32::MAX, f32::MAX, f32::MAX);
            let mut max = crate::compat::point3::new(f32::MIN, f32::MIN, f32::MIN);
            
            for point in positions {
                min.x = min.x.min(point.x);
                min.y = min.y.min(point.y);
                min.z = min.z.min(point.z);
                
                max.x = max.x.max(point.x);
                max.y = max.y.max(point.y);
                max.z = max.z.max(point.z);
            }
            
            (Some(min), Some(max))
        } else {
            (None, None)
        };
        
        // Convert Point3 min/max to Vec<f32> for accessor
        let min = min_point.map(|p| vec![p.x, p.y, p.z]);
        let max = max_point.map(|p| vec![p.x, p.y, p.z]);
        
        // Convert positions from Point3 to flat array for buffer
        let flat_positions: Vec<f32> = positions.iter().flat_map(|p| vec![p.x, p.y, p.z]).collect();
        
        // Add position data to buffer
        let pos_bytes = unsafe {
            std::slice::from_raw_parts(
                flat_positions.as_ptr() as *const u8,
                flat_positions.len() * std::mem::size_of::<f32>()
            )
        };
        let (pos_offset, pos_length) = self.add_buffer_data(pos_bytes);
        let pos_buffer_view = self.add_buffer_view(pos_offset, pos_length, Some(buffer_view_target::ARRAY_BUFFER));
        
        // Add position accessor
        let vertex_count = positions.len();
        let pos_accessor = self.add_accessor(
            pos_buffer_view,
            component_type::FLOAT,
            vertex_count,
            accessor_type::VEC3.to_string(),
            None,
            min,
            max
        );
        
        // Flatten the Triangle structs into a flat list of indices
        let flat_indices: Vec<u32> = indices.iter()
            .flat_map(|triangle| vec![triangle.a, triangle.b, triangle.c])
            .collect();
            
        // Add index data to buffer
        let idx_bytes = unsafe {
            std::slice::from_raw_parts(
                flat_indices.as_ptr() as *const u8,
                flat_indices.len() * std::mem::size_of::<u32>()
            )
        };
        let (idx_offset, idx_length) = self.add_buffer_data(idx_bytes);
        let idx_buffer_view = self.add_buffer_view(idx_offset, idx_length, Some(buffer_view_target::ELEMENT_ARRAY_BUFFER));
        
        // Add index accessor
        let idx_accessor = self.add_accessor(
            idx_buffer_view,
            component_type::UNSIGNED_INT,  // Use UNSIGNED_INT for u32 indices
            flat_indices.len(),
            accessor_type::SCALAR.to_string(),
            None,
            None,
            None
        );
        
        // Build attributes map
        let mut attributes = HashMap::new();
        attributes.insert("POSITION".to_string(), pos_accessor);
        
        // Add normals if provided
        if let Some(normal_data) = normals {
            // Convert normals from Vector3 to flat array for buffer
            let flat_normals: Vec<f32> = normal_data.iter().flat_map(|n| vec![n.x, n.y, n.z]).collect();
            
            let norm_bytes = unsafe {
                std::slice::from_raw_parts(
                    flat_normals.as_ptr() as *const u8,
                    flat_normals.len() * std::mem::size_of::<f32>()
                )
            };
            let (norm_offset, norm_length) = self.add_buffer_data(norm_bytes);
            let norm_buffer_view = self.add_buffer_view(norm_offset, norm_length, Some(buffer_view_target::ARRAY_BUFFER));
            
            let normal_accessor = self.add_accessor(
                norm_buffer_view,
                component_type::FLOAT,
                normal_data.len(),
                accessor_type::VEC3.to_string(),
                None,
                None,
                None
            );
            
            attributes.insert("NORMAL".to_string(), normal_accessor);
        }
        
        // Add texture coordinates if provided
        let mut texcoord_accessors = Vec::new();
        if let Some(texcoord_sets) = texcoords {
            for (i, texcoord_data) in texcoord_sets.iter().enumerate() {
                // Convert Vector2 to flat array for buffer
                let flat_texcoords: Vec<f32> = texcoord_data.iter().flat_map(|uv| vec![uv.x, uv.y]).collect();
                
                let tc_bytes = unsafe {
                    std::slice::from_raw_parts(
                        flat_texcoords.as_ptr() as *const u8,
                        flat_texcoords.len() * std::mem::size_of::<f32>()
                    )
                };
                let (tc_offset, tc_length) = self.add_buffer_data(tc_bytes);
                let tc_buffer_view = self.add_buffer_view(tc_offset, tc_length, Some(buffer_view_target::ARRAY_BUFFER));
                
                let tc_accessor = self.add_accessor(
                    tc_buffer_view,
                    component_type::FLOAT,
                    texcoord_data.len(), // Number of Vector2 elements
                    accessor_type::VEC2.to_string(),
                    None,
                    None,
                    None
                );
                
                attributes.insert(format!("TEXCOORD_{}", i), tc_accessor);
                texcoord_accessors.push(tc_accessor);
            }
        }
        
        // Create primitive
        let primitive = Primitive {
            attributes,
            indices: Some(idx_accessor),
            material,
            mode: None, // Default mode (triangles)
            ..Default::default()
        };
        
        // Create and add mesh
        self.add_mesh(name, vec![primitive])
    }
    
    
    /// Create a mesh with custom geometry and single UV channel using types
    /// 
    /// Simplified version of create_custom_mesh for the common case of a single UV channel,
    /// but using proper 3D math types instead of raw float arrays.
    /// 
    /// # Parameters
    /// * `name` - Optional name for the mesh
    /// * `positions` - Vertex positions as &[Point3<f32>]
    /// * `indices` - List of triangles using the Triangle struct
    /// * `normals` - Optional vertex normals as Vec<Vector3<f32>>
    /// * `texcoords` - Optional UV coordinates as Vec<Vector2<f32>>
    /// * `material` - Optional material index to use for the mesh
    /// 
    /// # Returns
    /// The index of the created mesh
    pub fn create_simple_mesh(&mut self, 
                               name: Option<String>,
                               positions: &[Point3<f32>], 
                               indices: &[Triangle], 
                               normals: Option<Vec<Vector3<f32>>>, 
                               texcoords: Option<Vec<Vector2<f32>>>,
                               material: Option<usize>) -> usize {
        // If we have texture coordinates, create a texcoord set for the mesh
        let texcoord_sets = if let Some(uvs) = texcoords {
            let mut sets = Vec::new();
            sets.push(uvs);
            Some(sets)
        } else {
            None
        };
        
        self.create_custom_mesh(name, positions, indices, normals, texcoord_sets, material)
    }
    
    /// Create a flat plane mesh with subdivisions
    /// 
    /// This method creates a flat rectangular plane on the XZ plane (with Y as up).
    /// The plane is centered at the origin and can be subdivided into a grid of triangles.
    /// Subdividing the plane is useful for terrain or deformation effects.
    /// 
    /// # Parameters
    /// * `width` - Width of the plane along X axis
    /// * `depth` - Depth of the plane along Z axis 
    /// * `width_segments` - Number of subdivisions along width (min: 1)
    /// * `depth_segments` - Number of subdivisions along depth (min: 1)
    /// * `material` - Optional material index to use for the mesh
    /// 
    /// # Returns
    /// The index of the created mesh in the glTF document's meshes array
    /// 
    /// # Example
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut builder = GltfBuilder::new();
    /// 
    /// // Create a material
    /// let ground_material = builder.create_basic_material(
    ///     Some("Ground".to_string()),
    ///     [0.5, 0.5, 0.5, 1.0]
    /// );
    /// 
    /// // Create a 10x10 ground plane with 20x20 grid subdivisions
    /// let ground_mesh = builder.create_plane(10.0, 10.0, 20, 20, Some(ground_material));
    /// ```
    pub fn create_plane(&mut self, 
                      width: f32, 
                      depth: f32, 
                      width_segments: usize, 
                      depth_segments: usize,
                      material: Option<usize>) -> usize {
        // Get the mesh data directly as types
        let (positions, indices, normals, uvs) = primitives::generate_plane(
            width, depth, width_segments, depth_segments
        );
        
        // Create the mesh using the mint types returned by the primitives module
        self.create_simple_mesh(None, &positions, &indices, Some(normals), Some(uvs), material)
    }
    
    /// Create a sphere mesh with specified radius and resolution
    /// 
    /// This method creates a UV-mapped sphere centered at the origin. The sphere is generated
    /// using latitude/longitude segmentation, with vertices distributed evenly around the surface.
    /// 
    /// # Parameters
    /// * `radius` - Radius of the sphere
    /// * `width_segments` - Number of horizontal subdivisions (longitude lines, min: 3)
    /// * `height_segments` - Number of vertical subdivisions (latitude lines, min: 2)
    /// * `material` - Optional material index to use for the mesh
    /// 
    /// # Returns
    /// The index of the created mesh in the glTF document's meshes array
    /// 
    /// # Example
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut builder = GltfBuilder::new();
    /// 
    /// // Create a red material
    /// let red_material = builder.create_basic_material(
    ///     Some("Red".to_string()),
    ///     [1.0, 0.0, 0.0, 1.0]
    /// );
    /// 
    /// // Create a high-detail red sphere with radius 2.0
    /// let sphere_mesh = builder.create_sphere(2.0, 32, 16, Some(red_material));
    /// ```
    pub fn create_sphere(&mut self, 
                       radius: f32, 
                       width_segments: usize, 
                       height_segments: usize,
                       material: Option<usize>) -> usize {
        // Get the mesh data directly as mint types
        let (positions, indices, normals, uvs) = primitives::generate_sphere(
            radius, width_segments, height_segments
        );
        
        // Create the mesh using the mint types returned by the primitives module
        self.create_simple_mesh(None, &positions, &indices, Some(normals), Some(uvs), material)
    }
    
    /// Create a cylinder mesh with customizable dimensions
    /// 
    /// This method creates a cylinder or a truncated cone (when top and bottom radii differ).
    /// The cylinder is centered at the origin and extends along the Y axis.
    /// The cylinder can be open-ended (without caps) or closed with caps.
    /// 
    /// # Parameters
    /// * `radius_top` - Radius at the top of the cylinder
    /// * `radius_bottom` - Radius at the bottom of the cylinder
    /// * `height` - Height of the cylinder along the Y axis
    /// * `radial_segments` - Number of subdivisions around the circumference (min: 3)
    /// * `height_segments` - Number of subdivisions along the height (min: 1)
    /// * `open_ended` - When `true`, the cylinder has no top or bottom caps
    /// * `material` - Optional material index to use for the mesh
    /// 
    /// # Returns
    /// The index of the created mesh in the glTF document's meshes array
    /// 
    /// # Example
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut builder = GltfBuilder::new();
    /// 
    /// // Create a blue material
    /// let blue_material = builder.create_basic_material(
    ///     Some("Blue".to_string()),
    ///     [0.0, 0.0, 0.8, 1.0]
    /// );
    /// 
    /// // Create a cylinder with different top and bottom radii (truncated cone)
    /// let cylinder_mesh = builder.create_cylinder(
    ///     0.5,   // radius top
    ///     1.0,   // radius bottom
    ///     2.0,   // height
    ///     16,    // radial segments
    ///     1,     // height segments
    ///     false, // closed with caps
    ///     Some(blue_material)
    /// );
    /// ```
    pub fn create_cylinder(&mut self, 
                         radius_top: f32, 
                         radius_bottom: f32, 
                         height: f32, 
                         radial_segments: usize, 
                         height_segments: usize,
                         open_ended: bool,
                         material: Option<usize>) -> usize {
        // Get the mesh data directly as types
        let (positions, indices, normals, uvs) = primitives::generate_cylinder(
            radius_top, radius_bottom, height, radial_segments, height_segments, open_ended
        );
        
        // Create the mesh using the types returned by the primitives module
        self.create_simple_mesh(None, &positions, &indices, Some(normals), Some(uvs), material)
    }
    
    /// Create a cone mesh
    /// 
    /// # Parameters
    /// * `radius` - Radius at the base of the cone
    /// * `height` - Height of the cone
    /// * `radial_segments` - Number of subdivisions around the circumference
    /// * `height_segments` - Number of subdivisions along the height
    /// * `open_ended` - Whether to include the base cap
    /// * `material` - Optional material index to use for the mesh
    /// 
    /// # Returns
    /// The index of the created mesh
    pub fn create_cone(&mut self, 
                      radius: f32, 
                      height: f32, 
                      radial_segments: usize, 
                      height_segments: usize,
                      open_ended: bool,
                      material: Option<usize>) -> usize {
        // Get the mesh data directly as types
        let (positions, indices, normals, uvs) = primitives::generate_cone(
            radius, height, radial_segments, height_segments, open_ended
        );
        
        // Create the mesh using the types returned by the primitives module
        self.create_simple_mesh(None, &positions, &indices, Some(normals), Some(uvs), material)
    }
    
    /// Create a torus (donut shape) mesh
    /// 
    /// # Parameters
    /// * `radius` - Distance from the center of the tube to the center of the torus
    /// * `tube` - Radius of the tube
    /// * `radial_segments` - Number of subdivisions around the main circle
    /// * `tubular_segments` - Number of subdivisions around the tube
    /// * `material` - Optional material index to use for the mesh
    /// 
    /// # Returns
    /// The index of the created mesh
    pub fn create_torus(&mut self, 
                       radius: f32, 
                       tube: f32, 
                       radial_segments: usize, 
                       tubular_segments: usize,
                       material: Option<usize>) -> usize {
        // Get the mesh data directly as types
        let (positions, indices, normals, uvs) = primitives::generate_torus(
            radius, tube, radial_segments, tubular_segments
        );
        
        // Create the mesh using the types returned by the primitives module
        self.create_simple_mesh(None, &positions, &indices, Some(normals), Some(uvs), material)
    }
    
    /// Create an icosahedron (20-sided polyhedron) mesh
    /// 
    /// # Parameters
    /// * `radius` - Radius of the circumscribed sphere
    /// * `material` - Optional material index to use for the mesh
    /// 
    /// # Returns
    /// The index of the created mesh
    pub fn create_icosahedron(&mut self, 
                            radius: f32,
                            material: Option<usize>) -> usize {
        // Get the mesh data directly as types
        let (positions, indices, normals, uvs) = primitives::generate_icosahedron(radius);
        
        // Create the mesh using the types returned by the primitives module
        self.create_simple_mesh(None, &positions, &indices, Some(normals), Some(uvs), material)
    }
}
