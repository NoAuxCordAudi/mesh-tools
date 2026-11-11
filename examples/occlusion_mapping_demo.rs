#![allow(deprecated)] // add_textured_material is kept for this demo; new code uses MaterialBuilder
use mesh_tools::GltfBuilder;
use std::error::Error;
use image::{DynamicImage, ImageBuffer, Rgba};

// Function to create a simple ambient occlusion (AO) texture
// This simulates shadows/darkening in crevices and tight areas
fn create_ambient_occlusion_texture(width: u32, height: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let max_dist = (center_x.min(center_y) * 0.9) as f32;
    
    for y in 0..height {
        for x in 0..width {
            // Calculate distance from center
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();
            
            // Normalize the distance to range 0-1
            let n_dist = (dist / max_dist).min(1.0);
            
            // Create a radial gradient AO texture
            // Edges of the circle will be darker (more occluded)
            let occlusion_value = n_dist * 0.8;
            
            // Convert to brightness (1.0 - occlusion) and map to grayscale
            // In AO maps, white means no occlusion, black means fully occluded
            let brightness = ((1.0 - occlusion_value) * 255.0) as u8;
            
            // Create grayscale pixel (R=G=B)
            img.put_pixel(x, y, Rgba([brightness, brightness, brightness, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

// Function to create a grid pattern occlusion texture
fn create_grid_occlusion_texture(width: u32, height: u32, line_width: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    let grid_size = width.min(height) / 8;
    
    for y in 0..height {
        for x in 0..width {
            // Check if we're on a grid line
            let on_h_line = y % grid_size < line_width;
            let on_v_line = x % grid_size < line_width;
            
            // Grid lines are darker (more occluded)
            let brightness = if on_h_line || on_v_line {
                // 50% occluded on grid lines
                128
            } else {
                // Fully unoccluded elsewhere
                255
            };
            
            img.put_pixel(x, y, Rgba([brightness, brightness, brightness, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

// Function to create a crevice occlusion map for a cube
// Simulates darkening near edges and corners
fn create_edge_occlusion_texture(width: u32, height: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    let section_width = width / 4;
    let section_height = height / 3;
    
    for y in 0..height {
        for x in 0..width {
            // Determine which face of the cube unwrap we're in
            // (We don't actually use this information but would in a more complex unwrap)
            let _section_x = x / section_width;
            let _section_y = y / section_height;
            
            // Calculate position within section (0.0 - 1.0)
            let local_x = (x % section_width) as f32 / section_width as f32;
            let local_y = (y % section_height) as f32 / section_height as f32;
            
            // Distance from edges
            let edge_dist_x = local_x.min(1.0 - local_x);
            let edge_dist_y = local_y.min(1.0 - local_y);
            let min_dist = edge_dist_x.min(edge_dist_y);
            
            // Apply edge darkening (occlusion)
            // Closer to edges = darker (more occluded)
            let edge_factor = (min_dist * 5.0).min(1.0);
            
            // Convert to brightness (white = no occlusion)
            let brightness = (edge_factor * 255.0) as u8;
            
            img.put_pixel(x, y, Rgba([brightness, brightness, brightness, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

fn main() -> Result<(), Box<dyn Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();
    
    // Create base color textures for better visualization
    let checkerboard_texture = builder.create_checkerboard_texture(
        512,                     // width
        512,                     // height
        64,                      // cell size
        [220, 220, 220],         // light gray
        [180, 180, 180],         // darker gray
    )?;
    
    // Create ambient occlusion textures
    
    // 1. Radial gradient occlusion (for sphere)
    let radial_ao_image = create_ambient_occlusion_texture(512, 512);
    let radial_ao_texture = builder.create_texture_from_image(
        Some("RadialAOTexture".to_string()),
        &radial_ao_image,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // 2. Grid pattern occlusion (for cube)
    let grid_ao_image = create_grid_occlusion_texture(512, 512, 8);
    let grid_ao_texture = builder.create_texture_from_image(
        Some("GridAOTexture".to_string()),
        &grid_ao_image,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // 3. Edge occlusion (for cube edges)
    let edge_ao_image = create_edge_occlusion_texture(512, 512);
    let edge_ao_texture = builder.create_texture_from_image(
        Some("EdgeAOTexture".to_string()),
        &edge_ao_image,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // Create materials with occlusion maps
    
    // 1. Material with radial occlusion map
    let radial_ao_material = builder.add_textured_material(
        Some("RadialAOMaterial".to_string()),
        Some(checkerboard_texture), // Base color texture
        None,                      // No metallic roughness texture
        None,                      // No normal map
        Some(radial_ao_texture),   // Occlusion texture
        None,                      // No emissive texture
        None,                      // No emissive factor
        Some(0.0),                 // Non-metallic
        Some(0.5),                 // Medium roughness
        None,                      // Default alpha mode
        None,                      // No alpha cutoff
        None                       // Not necessarily double sided
    )?;
    
    // 2. Material with grid occlusion map
    let grid_ao_material = builder.add_textured_material(
        Some("GridAOMaterial".to_string()),
        Some(checkerboard_texture), // Base color texture
        None,                      // No metallic roughness texture
        None,                      // No normal map
        Some(grid_ao_texture),     // Occlusion texture
        None,                      // No emissive texture
        None,                      // No emissive factor
        Some(0.0),                 // Non-metallic
        Some(0.5),                 // Medium roughness
        None,                      // Default alpha mode
        None,                      // No alpha cutoff
        None                       // Not necessarily double sided
    )?;
    
    // 3. Material with edge occlusion map
    let edge_ao_material = builder.add_textured_material(
        Some("EdgeAOMaterial".to_string()),
        Some(checkerboard_texture), // Base color texture
        None,                      // No metallic roughness texture
        None,                      // No normal map
        Some(edge_ao_texture),     // Occlusion texture
        None,                      // No emissive texture
        None,                      // No emissive factor
        Some(0.0),                 // Non-metallic
        Some(0.5),                 // Medium roughness
        None,                      // Default alpha mode
        None,                      // No alpha cutoff
        None                       // Not necessarily double sided
    )?;
    
    // 4. Control material with no occlusion map for comparison
    let control_material = builder.create_textured_material(
        Some("ControlMaterial".to_string()),
        checkerboard_texture
    )?;
    
    // Create meshes for demonstration
    
    // 1. Sphere with radial occlusion
    let sphere_mesh = builder.create_sphere(
        1.0,     // radius
        32,      // width segments
        16,      // height segments
        Some(radial_ao_material)
    );
    
    // 2. Cube with grid occlusion
    let grid_cube_mesh = builder.create_box_with_material(
        1.0,
        Some(grid_ao_material)
    );
    
    // 3. Cube with edge occlusion
    let edge_cube_mesh = builder.create_box_with_material(
        1.0,
        Some(edge_ao_material)
    );
    
    // 4. Control sphere with no occlusion
    let control_sphere_mesh = builder.create_sphere(
        1.0,     // radius
        32,      // width segments
        16,      // height segments
        Some(control_material)
    );
    
    // Create nodes for each object
    
    // 1. Sphere with radial occlusion
    let sphere_node = builder.add_node(
        Some("SphereWithRadialAO".to_string()),
        Some(sphere_mesh),
        Some([-2.5, 0.0, 0.0]),  // Left position
        None,
        None,
    );
    
    // 2. Cube with grid occlusion
    let grid_cube_node = builder.add_node(
        Some("CubeWithGridAO".to_string()),
        Some(grid_cube_mesh),
        Some([0.0, 0.0, 0.0]),   // Center position
        None,
        None,
    );
    
    // 3. Cube with edge occlusion
    let edge_cube_node = builder.add_node(
        Some("CubeWithEdgeAO".to_string()),
        Some(edge_cube_mesh),
        Some([2.5, 0.0, 0.0]),   // Right position 
        None,
        None,
    );
    
    // 4. Control sphere without occlusion 
    let control_sphere_node = builder.add_node(
        Some("ControlSphereNoAO".to_string()),
        Some(control_sphere_mesh),
        Some([-2.5, -3.0, 0.0]),  // Bottom left
        None,
        None,
    );
    
    // Create a scene with all objects
    builder.add_scene(
        Some("OcclusionMappingDemo".to_string()),
        Some(vec![
            sphere_node,
            grid_cube_node,
            edge_cube_node,
            control_sphere_node
        ]),
    );
    
    // Export the GLB file
    let output_path = "occlusion_mapping_demo.glb";
    builder.export_glb(output_path)?;
    
    println!("Successfully exported GLB file with occlusion mapping: {}", output_path);
    println!("This example demonstrates:");
    println!("  - Left: Sphere with radial gradient occlusion (darker at edges)");
    println!("  - Center: Cube with grid pattern occlusion");
    println!("  - Right: Cube with edge-darkening occlusion");
    println!("  - Bottom Left: Control sphere with no occlusion for comparison");
    println!("The occlusion maps simulate how ambient light is blocked in different areas,");
    println!("adding depth and definition to the objects.");
    
    Ok(())
}
