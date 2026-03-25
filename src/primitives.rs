//! # Primitive Shape Generation
//!
//! This module provides functions for generating the geometry data (vertices, indices, normals, and UVs)
//! for basic 3D primitive shapes such as planes, boxes, spheres, cylinders, and more.
//!
//! Each generation function follows the same pattern of returning a tuple with the following components:
//! - `positions`: Vertex coordinates as a vector of Point3<f32>
//! - `indices`: Triangle indices using the Triangle struct
//! - `normals`: Normal vectors as a vector of Vector3<f32>
//! - `uvs`: Texture coordinates as a vector of Vector2<f32>
//!
//! These mesh components can be directly used with the `GltfBuilder` to create complete 3D meshes.
//!
//! ## Example
//!
//! ```rust
//! use mesh_tools::primitives::generate_sphere;
//! use mesh_tools::compat::{Point3, Vector3, Vector2};
//! 
//! // Generate a sphere with radius 1.0
//! let (positions, indices, normals, uvs) = generate_sphere(1.0, 16, 8);
//! ```

use std::f32::consts::PI;
use crate::compat::{Point3, Vector2, Vector3};
use crate::builder_primitives::Triangle;

/// Generate a plane (flat surface) with subdivisions
/// 
/// # Parameters
/// * `width` - Width of the plane along X axis
/// * `depth` - Depth of the plane along Z axis 
/// * `width_segments` - Number of subdivisions along width
/// * `depth_segments` - Number of subdivisions along depth
/// 
/// # Returns
/// Tuple of (positions, indices, normals, uvs)
pub fn generate_plane(
    width: f32,
    depth: f32,
    width_segments: usize,
    depth_segments: usize,
) -> (Vec<Point3<f32>>, Vec<Triangle>, Vec<Vector3<f32>>, Vec<Vector2<f32>>) {
    let width_half = width / 2.0;
    let depth_half = depth / 2.0;
    
    let grid_x = width_segments;
    let grid_z = depth_segments;
    
    // Segment dimensions not actually used directly in this implementation
    // but kept for clarity and possible future enhancements
    let _segment_width = width / grid_x as f32;
    let _segment_depth = depth / grid_z as f32;
    
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    
    // Generate vertices, normals and uvs
    for z in 0..=grid_z {
        let z_percent = z as f32 / grid_z as f32;
        
        for x in 0..=grid_x {
            let x_percent = x as f32 / grid_x as f32;
            
            let x_pos = x_percent * width - width_half;
            let z_pos = z_percent * depth - depth_half;
            
            // Position
            positions.push(crate::compat::point3::new(x_pos, 0.0, z_pos));
            
            // Normal
            normals.push(crate::compat::vector3::new(0.0, 1.0, 0.0));
            
            // UV
            uvs.push(crate::compat::vector2::new(x_percent, 1.0 - z_percent));  // Flip Y for texture coordinates
        }
    }
    
    // Generate indices
    let vertices_per_row = grid_x + 1;
    
    for z in 0..grid_z {
        for x in 0..grid_x {
            let a = (z * vertices_per_row + x) as u32;
            let b = (z * vertices_per_row + x + 1) as u32;
            let c = ((z + 1) * vertices_per_row + x + 1) as u32;
            let d = ((z + 1) * vertices_per_row + x) as u32;
            
            // Two triangles per grid cell
            indices.push(Triangle::new(a, b, d));
            indices.push(Triangle::new(b, c, d));
        }
    }
    
    (positions, indices, normals, uvs)
}

/// Generate a sphere with subdivisions
/// 
/// # Parameters
/// * `radius` - Radius of the sphere
/// * `width_segments` - Number of horizontal subdivisions
/// * `height_segments` - Number of vertical subdivisions
/// 
/// # Returns
/// Tuple of (positions, indices, normals, uvs)
pub fn generate_sphere(
    radius: f32,
    width_segments: usize,
    height_segments: usize,
) -> (Vec<Point3<f32>>, Vec<Triangle>, Vec<Vector3<f32>>, Vec<Vector2<f32>>) {
    let width_segments = width_segments.max(3);
    let height_segments = height_segments.max(2);
    
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    
    // Generate vertices, normals and uvs
    for y in 0..=height_segments {
        let v = y as f32 / height_segments as f32;
        let phi = v * PI;
        
        for x in 0..=width_segments {
            let u = x as f32 / width_segments as f32;
            let theta = u * 2.0 * PI;
            
            // Calculate vertex position on the sphere
            let x_pos = -radius * theta.sin() * phi.sin();
            let y_pos = radius * phi.cos();
            let z_pos = radius * theta.cos() * phi.sin();
            
            // Position
            positions.push(crate::compat::point3::new(x_pos, y_pos, z_pos));
            
            // Normal (normalized position)
            let length = (x_pos * x_pos + y_pos * y_pos + z_pos * z_pos).sqrt();
            normals.push(crate::compat::vector3::new(x_pos / length, y_pos / length, z_pos / length));
            
            // UV
            uvs.push(crate::compat::vector2::new(u, 1.0 - v));  // Flip V for texture coordinates
        }
    }
    
    // Generate indices
    let vertices_per_row = width_segments + 1;
    
    for y in 0..height_segments {
        for x in 0..width_segments {
            let a = (y * vertices_per_row + x) as u32;
            let b = (y * vertices_per_row + x + 1) as u32;
            let c = ((y + 1) * vertices_per_row + x + 1) as u32;
            let d = ((y + 1) * vertices_per_row + x) as u32;
            
            // Two triangles per grid cell
            // Except at the poles where we have single triangles
            if y != 0 {
                indices.push(Triangle::new(a, b, d));
            }
            
            if y != height_segments - 1 {
                indices.push(Triangle::new(b, c, d));
            }
        }
    }
    
    (positions, indices, normals, uvs)
}

/// Generate a cylinder
/// 
/// # Parameters
/// * `radius_top` - Radius at the top of the cylinder
/// * `radius_bottom` - Radius at the bottom of the cylinder
/// * `height` - Height of the cylinder
/// * `radial_segments` - Number of subdivisions around the circumference
/// * `height_segments` - Number of subdivisions along the height
/// * `open_ended` - Whether to include top and bottom caps
/// 
/// # Returns
/// Tuple of (positions, indices, normals, uvs)
pub fn generate_cylinder(
    radius_top: f32,
    radius_bottom: f32,
    height: f32,
    radial_segments: usize,
    height_segments: usize,
    open_ended: bool,
) -> (Vec<Point3<f32>>, Vec<Triangle>, Vec<Vector3<f32>>, Vec<Vector2<f32>>) {
    let radial_segments = radial_segments.max(3);
    let height_segments = height_segments.max(1);
    
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    
    // Helper function to calculate slope normals
    let get_slope_normal = |_radius: f32, slope_factor: f32, u: f32| -> (f32, f32, f32) {
        let sin_theta = (u * 2.0 * PI).sin();
        let cos_theta = (u * 2.0 * PI).cos();
        
        let nx = cos_theta;
        let ny = slope_factor;
        let nz = sin_theta;
        
        // Normalize
        let length = (nx * nx + ny * ny + nz * nz).sqrt();
        
        (nx / length, ny / length, nz / length)
    };
    
    // Calculate slope factor for cylinder sides
    let slope_factor = if radius_top == radius_bottom {
        0.0 // No slope for a perfect cylinder
    } else {
        height / (radius_bottom - radius_top)
    };
    
    // Generate vertices for the curved surface
    for y in 0..=height_segments {
        let v = y as f32 / height_segments as f32;
        let y_pos = v * height - height / 2.0;
        let radius = radius_bottom + v * (radius_top - radius_bottom);
        
        for x in 0..=radial_segments {
            let u = x as f32 / radial_segments as f32;
            let theta = u * 2.0 * PI;
            
            let sin_theta = theta.sin();
            let cos_theta = theta.cos();
            
            // Position
            positions.push(crate::compat::point3::new(radius * cos_theta, y_pos, radius * sin_theta));
            
            // Normal
            let (nx, ny, nz) = get_slope_normal(radius, slope_factor, u);
            normals.push(crate::compat::vector3::new(nx, ny, nz));
            
            // UV
            uvs.push(crate::compat::vector2::new(u, 1.0 - v));
        }
    }
    
    // Generate indices for the curved surface
    let vertices_per_row = radial_segments + 1;
    
    for y in 0..height_segments {
        for x in 0..radial_segments {
            let a = (y * vertices_per_row + x) as u32;
            let b = (y * vertices_per_row + x + 1) as u32;
            let c = ((y + 1) * vertices_per_row + x + 1) as u32;
            let d = ((y + 1) * vertices_per_row + x) as u32;
            
            // Two triangles per grid cell
            indices.push(Triangle::new(a, d, b));
            indices.push(Triangle::new(b, d, c));
        }
    }
    
    // If not open ended, add top and bottom caps
    if !open_ended {
        let start_index = positions.len();
        
        // Top cap
        // Center vertex
        positions.push(crate::compat::point3::new(0.0, height / 2.0, 0.0));
        
        normals.push(crate::compat::vector3::new(0.0, 1.0, 0.0));
        
        uvs.push(crate::compat::vector2::new(0.5, 0.5));
        
        // Cap vertices
        for x in 0..=radial_segments {
            let u = x as f32 / radial_segments as f32;
            let theta = u * 2.0 * PI;
            
            let cos_theta = theta.cos();
            let sin_theta = theta.sin();
            
            // Position
            positions.push(crate::compat::point3::new(radius_top * cos_theta, height / 2.0, radius_top * sin_theta));
            
            // Normal
            normals.push(crate::compat::vector3::new(0.0, 1.0, 0.0));
            
            // UV
            uvs.push(crate::compat::vector2::new(cos_theta * 0.5 + 0.5, sin_theta * 0.5 + 0.5));
        }
        
        // Top cap indices
        let center_index = start_index as u32;
        
        for x in 0..radial_segments {
            indices.push(Triangle::new(
                center_index,
                center_index + (x + 2) as u32,
                center_index + (x + 1) as u32
            ));
        }
        
        // Bottom cap
        let start_index_bottom = positions.len();
        
        // Center vertex
        positions.push(crate::compat::point3::new(0.0, -height / 2.0, 0.0));
        
        normals.push(crate::compat::vector3::new(0.0, -1.0, 0.0));
        
        uvs.push(crate::compat::vector2::new(0.5, 0.5));
        
        // Cap vertices
        for x in 0..=radial_segments {
            let u = x as f32 / radial_segments as f32;
            let theta = u * 2.0 * PI;
            
            let cos_theta = theta.cos();
            let sin_theta = theta.sin();
            
            // Position
            positions.push(crate::compat::point3::new(radius_bottom * cos_theta, -height / 2.0, radius_bottom * sin_theta));
            
            // Normal
            normals.push(crate::compat::vector3::new(0.0, -1.0, 0.0));
            
            // UV
            uvs.push(crate::compat::vector2::new(cos_theta * 0.5 + 0.5, sin_theta * 0.5 + 0.5));
        }
        
        // Bottom cap indices
        let center_index = start_index_bottom as u32;
        
        for x in 0..radial_segments {
            indices.push(Triangle::new(
                center_index,
                center_index + (x + 1) as u32,
                center_index + (x + 2) as u32
            ));
        }
    }
    
    (positions, indices, normals, uvs)
}

/// Generate a cone (special case of cylinder)
/// 
/// # Parameters
/// * `radius` - Radius at the base of the cone
/// * `height` - Height of the cone
/// * `radial_segments` - Number of subdivisions around the circumference
/// * `height_segments` - Number of subdivisions along the height
/// * `open_ended` - Whether to include the base cap
/// 
/// # Returns
/// Tuple of (positions, indices, normals, uvs)
pub fn generate_cone(
    radius: f32,
    height: f32,
    radial_segments: usize,
    height_segments: usize,
    open_ended: bool,
) -> (Vec<Point3<f32>>, Vec<Triangle>, Vec<Vector3<f32>>, Vec<Vector2<f32>>) {
    generate_cylinder(0.0, radius, height, radial_segments, height_segments, open_ended)
}

/// Generate a torus (donut shape)
/// 
/// # Parameters
/// * `radius` - Distance from the center of the tube to the center of the torus
/// * `tube` - Radius of the tube
/// * `radial_segments` - Number of subdivisions around the main circle
/// * `tubular_segments` - Number of subdivisions around the tube
/// 
/// # Returns
/// Tuple of (positions, indices, normals, uvs)
pub fn generate_torus(
    radius: f32,
    tube: f32,
    radial_segments: usize,
    tubular_segments: usize,
) -> (Vec<Point3<f32>>, Vec<Triangle>, Vec<Vector3<f32>>, Vec<Vector2<f32>>) {
    let radial_segments = radial_segments.max(2);
    let tubular_segments = tubular_segments.max(3);
    
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    
    // Generate vertices
    for j in 0..=radial_segments {
        for i in 0..=tubular_segments {
            let u = i as f32 / tubular_segments as f32 * 2.0 * PI;
            let v = j as f32 / radial_segments as f32 * 2.0 * PI;
            
            // Position
            let x = (radius + tube * v.cos()) * u.cos();
            let y = (radius + tube * v.cos()) * u.sin();
            let z = tube * v.sin();
            
            positions.push(crate::compat::point3::new(x, y, z));
            
            // Normal
            let center_x = radius * u.cos();
            let center_y = radius * u.sin();
            
            let nx = x - center_x;
            let ny = y - center_y;
            let nz = z;
            
            // Normalize
            let length = (nx * nx + ny * ny + nz * nz).sqrt();
            normals.push(crate::compat::vector3::new(nx / length, ny / length, nz / length));
            
            // UV
            uvs.push(crate::compat::vector2::new(i as f32 / tubular_segments as f32, j as f32 / radial_segments as f32));
        }
    }
    
    // Generate indices
    for j in 0..radial_segments {
        for i in 0..tubular_segments {
            let a = (j * (tubular_segments + 1) + i) as u32;
            let b = (j * (tubular_segments + 1) + i + 1) as u32;
            let c = ((j + 1) * (tubular_segments + 1) + i + 1) as u32;
            let d = ((j + 1) * (tubular_segments + 1) + i) as u32;
            
            // Two triangles per cell
            indices.push(Triangle::new(a, b, d));
            indices.push(Triangle::new(b, c, d));
        }
    }
    
    (positions, indices, normals, uvs)
}

/// Generate an icosahedron (20-sided polyhedron)
/// 
/// # Parameters
/// * `radius` - Radius of the circumscribed sphere
/// 
/// # Returns
/// Tuple of (positions, indices, normals, uvs)
pub fn generate_icosahedron(radius: f32) -> (Vec<Point3<f32>>, Vec<Triangle>, Vec<Vector3<f32>>, Vec<Vector2<f32>>) {
    // Constants for icosahedron construction
    let t = (1.0 + 5.0_f32.sqrt()) / 2.0;
    
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    
    // The 12 vertices of the icosahedron
    let vertices = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ];
    
    // Normalize and scale vertices
    for vertex in &vertices {
        let length = (vertex[0] * vertex[0] + vertex[1] * vertex[1] + vertex[2] * vertex[2]).sqrt();
        let normalized = [
            vertex[0] / length * radius,
            vertex[1] / length * radius,
            vertex[2] / length * radius,
        ];
        
        positions.push(crate::compat::point3::new(normalized[0], normalized[1], normalized[2]));
        
        // Normals (same as normalized positions)
        normals.push(crate::compat::vector3::new(normalized[0] / radius, normalized[1] / radius, normalized[2] / radius));
        
        // Basic UV mapping (spherical projection)
        let u = 0.5 + (normalized[0] / radius).atan2(normalized[2] / radius) / (2.0 * PI);
        let v = 0.5 - (normalized[1] / radius).asin() / PI;
        
        uvs.push(crate::compat::vector2::new(u, v));
    }
    
    // The 20 triangles of the icosahedron
    let triangle_indices = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    
    // Add triangle indices
    for tri in &triangle_indices {
        indices.push(Triangle::new(tri[0] as u32, tri[1] as u32, tri[2] as u32));
    }
    
    (positions, indices, normals, uvs)
}
