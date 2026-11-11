use mesh_tools::{GltfBuilder, Triangle};
use mesh_tools::compat::{Point3, Vector2, Vector3};
use std::error::Error;

// Helper function to calculate cross product for mint Vector3
fn cross(a: Vector3<f32>, b: Vector3<f32>) -> Vector3<f32> {
    mesh_tools::compat::vector3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();
    
    // Create materials for our terrain
    let grass_material = builder.create_basic_material(
        Some("Grass".to_string()),
        [0.4, 0.8, 0.3, 1.0], // Green
    );
    
    let mountain_material = builder.create_basic_material(
        Some("Mountain".to_string()),
        [0.6, 0.6, 0.6, 1.0], // Gray
    );
    
    let snow_material = builder.create_metallic_material(
        Some("Snow".to_string()),
        [1.0, 1.0, 1.0, 1.0], // White
        0.1, // Low metallic factor
        0.8, // High roughness factor
    );
    
    let water_material = builder.create_metallic_material(
        Some("Water".to_string()),
        [0.0, 0.3, 0.8, 0.8], // Blue with transparency
        0.2, // Low metallic
        0.1, // Low roughness (shiny)
    );
    
    let sand_material = builder.create_basic_material(
        Some("Sand".to_string()),
        [0.76, 0.7, 0.5, 1.0], // Sandy color
    );
    
    // Generate procedural terrain mesh
    generate_terrain_mesh(&mut builder, 64, 64, 10.0, 2.0,
                         grass_material, mountain_material, snow_material, 
                         water_material, sand_material)?;
    
    // Add a camera viewpoint
    let viewpoint = builder.add_node(
        Some("Viewpoint".to_string()),
        None, // No mesh for viewpoint
        Some([0.0, 15.0, 40.0]), // Position for a good view
        Some([-0.4, 0.0, 0.0, 0.92]), // Rotation quaternion looking down at scene
        None,
    );
    
    // Set the default scene to include our viewpoint
    builder.add_scene(
        Some("Terrain Scene".to_string()),
        Some(vec![viewpoint]),
    );
    
    // Export the GLB file
    let output_path = "terrain_demo.glb";
    builder.export_glb(output_path)?;
    
    println!("Successfully exported procedural terrain: {}", output_path);
    println!("");
    println!("This example demonstrates procedural terrain generation with:");
    println!("1. Height map based on mathematical noise functions");
    println!("2. Material variation based on elevation and slope");
    println!("3. Multiple biomes (water, sand, grass, mountains, snow)");
    println!("4. Realistic terrain features (mountains, valleys, plateaus)");
    
    Ok(())
}

/// Generate a terrain mesh with materials based on height and slope
fn generate_terrain_mesh(
    builder: &mut GltfBuilder,
    width_segments: usize,
    depth_segments: usize,
    width: f32,
    height_scale: f32,
    grass_material: usize,
    mountain_material: usize,
    snow_material: usize,
    water_material: usize,
    sand_material: usize,
) -> Result<(), Box<dyn Error>> {
    // Height map dimensions
    let grid_width = width_segments + 1;
    let grid_depth = depth_segments + 1;
    
    // Generate height map
    let height_map = generate_height_map(grid_width, grid_depth, height_scale);
    
    // Calculate vertices, normals, and indices using nalgebra types
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut raw_indices = Vec::new(); // We'll convert these to Triangle structs later
    let mut texcoords = Vec::new();
    
    // Create vertices and texture coordinates
    for z in 0..grid_depth {
        for x in 0..grid_width {
            let u = x as f32 / (grid_width - 1) as f32;
            let v = z as f32 / (grid_depth - 1) as f32;
            
            let x_pos = u * width - width / 2.0;
            let z_pos = v * width - width / 2.0;
            let y_pos = height_map[z * grid_width + x];
            
            // Add position using Point3
            positions.push(mesh_tools::compat::point3::new(x_pos, y_pos, z_pos));
            
            // Add UV coordinates using Vector2
            texcoords.push(mesh_tools::compat::vector2::new(u, v));
        }
    }
    
    // Create indices for triangles
    let mut indices = Vec::new(); // This will hold our Triangle structs
    
    for z in 0..depth_segments {
        for x in 0..width_segments {
            let tl = z * grid_width + x;
            let tr = tl + 1;
            let bl = (z + 1) * grid_width + x;
            let br = bl + 1;
            
            // First triangle as Triangle struct
            indices.push(Triangle {
                a: tl as u32,
                b: bl as u32,
                c: tr as u32
            });
            
            // Second triangle as Triangle struct
            indices.push(Triangle {
                a: tr as u32,
                b: bl as u32,
                c: br as u32
            });
            
            // Also keep raw indices for the normals calculation and material sorting
            raw_indices.push(tl as u16);
            raw_indices.push(bl as u16);
            raw_indices.push(tr as u16);
            
            raw_indices.push(tr as u16);
            raw_indices.push(bl as u16);
            raw_indices.push(br as u16);
        }
    }
    
    // Calculate normals using the nalgebra types
    normals = calculate_normals(&positions, &raw_indices);
    
    // Create separate meshes based on height and slope for different materials
    let water_threshold = 0.0;
    let sand_threshold = 0.2;
    let grass_threshold = 2.0;
    let mountain_threshold = 5.0;
    
    // Arrays to store indices for different material regions
    let mut water_triangles = Vec::new();
    let mut sand_triangles = Vec::new();
    let mut grass_triangles = Vec::new();
    let mut mountain_triangles = Vec::new();
    let mut snow_triangles = Vec::new();
    
    // Assign triangles to appropriate materials based on height
    for (i, triangle) in raw_indices.chunks_exact(3).enumerate() {
        let idx1 = triangle[0] as usize;
        let idx2 = triangle[1] as usize;
        let idx3 = triangle[2] as usize;
        
        let y1 = positions[idx1].y;
        let y2 = positions[idx2].y;
        let y3 = positions[idx3].y;
        
        // Calculate average height of the triangle
        let avg_height = (y1 + y2 + y3) / 3.0;
        
        // Calculate the slope of the triangle
        let normal_idx = idx1; // Use normal of first vertex
        let normal_y = normals[normal_idx].y;
        let slope = 1.0 - normal_y; // 0 for flat, 1 for vertical
        
        // Create a Triangle struct from the indices
        let triangle = Triangle::new(
            triangle[0] as u32,
            triangle[1] as u32,
            triangle[2] as u32
        );
        
        // Categorize triangle based on height and slope
        if avg_height < water_threshold {
            water_triangles.push(triangle);
        } else if avg_height < sand_threshold {
            sand_triangles.push(triangle);
        } else if avg_height < grass_threshold || slope < 0.3 {
            grass_triangles.push(triangle);
        } else if avg_height < mountain_threshold {
            mountain_triangles.push(triangle);
        } else {
            snow_triangles.push(triangle);
        }
    }
    
    // Create separate meshes for each terrain type using nalgebra types
    let water_mesh = if !water_triangles.is_empty() {
        // Convert the Vector2 texcoords to a Vec<Vec<Vector2>> for multiple UV sets
        let texcoord_sets = Some(vec![texcoords.clone()]);
        
        Some(builder.create_custom_mesh(
            Some("Water".to_string()),
            &positions,
            &water_triangles,
            Some(normals.clone()),
            texcoord_sets,
            Some(water_material),
        ))
    } else {
        None
    };
    
    let sand_mesh = if !sand_triangles.is_empty() {
        // Convert the Vector2 texcoords to a Vec<Vec<Vector2>> for multiple UV sets
        let texcoord_sets = Some(vec![texcoords.clone()]);
        
        Some(builder.create_custom_mesh(
            Some("Sand".to_string()),
            &positions,
            &sand_triangles,
            Some(normals.clone()),
            texcoord_sets,
            Some(sand_material),
        ))
    } else {
        None
    };
    
    let grass_mesh = if !grass_triangles.is_empty() {
        // Convert the Vector2 texcoords to a Vec<Vec<Vector2>> for multiple UV sets
        let texcoord_sets = Some(vec![texcoords.clone()]);
        
        Some(builder.create_custom_mesh(
            Some("Grass".to_string()),
            &positions,
            &grass_triangles,
            Some(normals.clone()),
            texcoord_sets,
            Some(grass_material),
        ))
    } else {
        None
    };
    
    let mountain_mesh = if !mountain_triangles.is_empty() {
        // Convert the Vector2 texcoords to a Vec<Vec<Vector2>> for multiple UV sets
        let texcoord_sets = Some(vec![texcoords.clone()]);
        
        Some(builder.create_custom_mesh(
            Some("Mountain".to_string()),
            &positions,
            &mountain_triangles,
            Some(normals.clone()),
            texcoord_sets,
            Some(mountain_material),
        ))
    } else {
        None
    };
    
    let snow_mesh = if !snow_triangles.is_empty() {
        // Convert the Vector2 texcoords to a Vec<Vec<Vector2>> for multiple UV sets
        let texcoord_sets = Some(vec![texcoords.clone()]);
        
        Some(builder.create_custom_mesh(
            Some("Snow".to_string()),
            &positions,
            &snow_triangles,
            Some(normals.clone()),
            texcoord_sets,
            Some(snow_material),
        ))
    } else {
        None
    };
    
    // Create nodes for each terrain type
    let mut terrain_nodes = Vec::new();
    
    if let Some(mesh) = water_mesh {
        let node = builder.add_node(
            Some("WaterNode".to_string()),
            Some(mesh),
            None,
            None,
            None,
        );
        terrain_nodes.push(node);
    }
    
    if let Some(mesh) = sand_mesh {
        let node = builder.add_node(
            Some("SandNode".to_string()),
            Some(mesh),
            None,
            None,
            None,
        );
        terrain_nodes.push(node);
    }
    
    if let Some(mesh) = grass_mesh {
        let node = builder.add_node(
            Some("GrassNode".to_string()),
            Some(mesh),
            None,
            None,
            None,
        );
        terrain_nodes.push(node);
    }
    
    if let Some(mesh) = mountain_mesh {
        let node = builder.add_node(
            Some("MountainNode".to_string()),
            Some(mesh),
            None,
            None,
            None,
        );
        terrain_nodes.push(node);
    }
    
    if let Some(mesh) = snow_mesh {
        let node = builder.add_node(
            Some("SnowNode".to_string()),
            Some(mesh),
            None,
            None,
            None,
        );
        terrain_nodes.push(node);
    }
    
    // Create a parent node for all terrain parts
    let terrain_parent = builder.create_node_hierarchy(
        Some("Terrain".to_string()),
        None,
        None,
        None,
        terrain_nodes,
    )?;
    
    // Add the terrain to the scene
    builder.add_scene(
        Some("Main Scene".to_string()),
        Some(vec![terrain_parent]),
    );
    
    Ok(())
}

/// Generate a height map using a combination of noise functions
fn generate_height_map(width: usize, depth: usize, height_scale: f32) -> Vec<f32> {
    let mut height_map = vec![0.0; width * depth];
    
    // Noise parameters
    let mountain_scale = 1.0 / 20.0; // Large features
    let hill_scale = 1.0 / 10.0;     // Medium features
    let roughness_scale = 1.0 / 2.0; // Small features
    
    for z in 0..depth {
        for x in 0..width {
            let nx = x as f32 / width as f32;
            let nz = z as f32 / depth as f32;
            
            // Create interesting terrain features with combined noise functions
            
            // Large mountain ranges
            let mountains = simplex_noise_2d(nx * mountain_scale, nz * mountain_scale);
            
            // Medium hills
            let hills = simplex_noise_2d(nx * hill_scale + 100.0, nz * hill_scale + 100.0) * 0.5;
            
            // Small roughness features
            let roughness = simplex_noise_2d(nx * roughness_scale + 200.0, nz * roughness_scale + 200.0) * 0.25;
            
            // Combine different features with varied weights
            let combined = mountains + hills + roughness;
            
            // Apply island falloff to make terrain recede at the edges
            let dx = nx - 0.5;
            let dz = nz - 0.5;
            let distance_from_center = (dx * dx + dz * dz).sqrt() * 2.0;
            let falloff = 1.0 - smoothstep(0.0, 1.0, distance_from_center);
            
            // Combine noise with falloff and apply height scale
            let height = combined * falloff * height_scale;
            
            height_map[z * width + x] = height;
        }
    }
    
    height_map
}

/// Simple implementation of a 2D simplex-like noise function
/// For proper terrain, a real noise library would be better
fn simplex_noise_2d(x: f32, y: f32) -> f32 {
    // This is a simplified noise function that gives terrain-like results
    // It uses a combination of sine waves at different frequencies
    
    let noise1 = (x * 1.0 + y * 2.3).sin();
    let noise2 = (x * 2.1 + y * 0.9).sin() * 0.5;
    let noise3 = (x * 3.6 + y * 3.1).sin() * 0.25;
    let noise4 = (x * 7.2 + y * 8.8).sin() * 0.125;
    
    // Combine the noise values
    (noise1 + noise2 + noise3 + noise4) * 0.5 + 0.5 // Scale to [0, 1]
}

/// Calculate smooth step function
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).max(0.0).min(1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Calculate vertex normals based on triangle faces using nalgebra types
fn calculate_normals(positions: &[Point3<f32>], indices: &[u16]) -> Vec<Vector3<f32>> {
    let vertex_count = positions.len();
    let mut normals = vec![mesh_tools::compat::vector3::new(0.0, 0.0, 0.0); vertex_count];
    
    // For each triangle
    for i in (0..indices.len()).step_by(3) {
        let i1 = indices[i] as usize;
        let i2 = indices[i + 1] as usize;
        let i3 = indices[i + 2] as usize;
        
        // Get vertices of this triangle
        let v1 = positions[i1];
        let v2 = positions[i2];
        let v3 = positions[i3];
        
        // Calculate vectors along two edges of the triangle
        let edge1 = mesh_tools::compat::vector3::new(v2.x - v1.x, v2.y - v1.y, v2.z - v1.z);
        let edge2 = mesh_tools::compat::vector3::new(v3.x - v1.x, v3.y - v1.y, v3.z - v1.z);
        
        // Calculate cross product to get face normal
        let normal = cross(edge1, edge2);
        
        // Accumulate the normal on each vertex of the triangle
        normals[i1].x += normal.x;
        normals[i1].y += normal.y;
        normals[i1].z += normal.z;
        
        normals[i2].x += normal.x;
        normals[i2].y += normal.y;
        normals[i2].z += normal.z;
        
        normals[i3].x += normal.x;
        normals[i3].y += normal.y;
        normals[i3].z += normal.z;
    }
    
    // Normalize all the normals
    for normal in &mut normals {
        let length = (normal.x * normal.x + normal.y * normal.y + normal.z * normal.z).sqrt();
        if length > 0.0 {
            normal.x /= length;
            normal.y /= length;
            normal.z /= length;
        }
    }
    
    normals
}
