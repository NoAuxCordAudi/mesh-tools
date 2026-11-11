#![allow(deprecated)] // add_textured_material is kept for this demo; new code uses MaterialBuilder
use mesh_tools::GltfBuilder;
use std::error::Error;
use std::f32::consts::PI;
use image::{DynamicImage, ImageBuffer, Rgba};

// Function to create a simple normal map that simulates a curved surface
fn create_bump_normal_map(width: u32, height: u32, bump_height: f32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let max_dist = (center_x.min(center_y) * 0.8) as f32;
    
    for y in 0..height {
        for x in 0..width {
            // Calculate distance from center
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();
            
            // Normalize the distance to range 0-1
            let n_dist = (dist / max_dist).min(1.0);
            
            // Create a dome shape with cosine
            let angle = (1.0 - n_dist) * PI / 2.0;
            let z_factor = angle.cos() * bump_height;
            
            // Calculate normal vector
            // For a dome shape, the normal is based on the gradient of the height field
            let nx: f32;
            let ny: f32;
            let nz: f32;
            
            if dist > 0.001 {
                // Direction from center to pixel
                let dir_x = dx / dist;
                let dir_y = dy / dist;
                
                // Normal calculation
                let strength = (1.0 - n_dist) * bump_height;
                nx = dir_x * strength;
                ny = dir_y * strength;
                nz = z_factor;
            } else {
                // At the exact center, normal points straight up
                nx = 0.0;
                ny = 0.0;
                nz = 1.0;
            }
            
            // Normalize the normal vector
            let len = (nx*nx + ny*ny + nz*nz).sqrt();
            
            // Convert normal from -1,1 range to 0,1 range for RGB encoding
            // Normal map convention: RGB = (normal.x + 1, normal.y + 1, normal.z + 1) / 2
            let r = ((nx / len + 1.0) * 127.5) as u8;
            let g = ((ny / len + 1.0) * 127.5) as u8;
            let b = ((nz / len + 1.0) * 127.5) as u8;
            
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

// Function to create a wave pattern normal map
fn create_wave_normal_map(width: u32, height: u32, amplitude: f32, frequency: f32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    
    for y in 0..height {
        for x in 0..width {
            // Normalized coordinates (0 to 1)
            let nx = x as f32 / width as f32;
            let ny = y as f32 / height as f32;
            
            // Create wave pattern
            let angle_x = nx * 2.0 * PI * frequency;
            let angle_y = ny * 2.0 * PI * frequency;
            
            // Calculate normals based on the derivative of the wave function
            // z = amplitude * sin(frequency * x) * sin(frequency * y)
            let dx = -amplitude * angle_x.cos() * angle_y.sin() * (2.0 * PI * frequency / width as f32);
            let dy = -amplitude * angle_x.sin() * angle_y.cos() * (2.0 * PI * frequency / height as f32);
            let dz = 1.0;
            
            // Normalize the normal vector
            let len = (dx*dx + dy*dy + dz*dz).sqrt();
            
            // Convert normal from -1,1 range to 0,1 range for RGB encoding
            // Normal map convention: RGB = (normal.x + 1, normal.y + 1, normal.z + 1) / 2
            let r = ((dx / len + 1.0) * 127.5) as u8;
            let g = ((dy / len + 1.0) * 127.5) as u8;
            let b = ((dz / len + 1.0) * 127.5) as u8;
            
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

fn main() -> Result<(), Box<dyn Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();
    
    // Create textures for normal maps
    
    // 1. Create a bump normal map (simulates a dome on a flat surface)
    let bump_normal_image = create_bump_normal_map(512, 512, 0.8);
    let bump_normal_texture = builder.create_texture_from_image(
        Some("BumpNormalMap".to_string()),
        &bump_normal_image,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // 2. Create a wave normal map (simulates wavy surface)
    let wave_normal_image = create_wave_normal_map(512, 512, 0.5, 4.0);
    let wave_normal_texture = builder.create_texture_from_image(
        Some("WaveNormalMap".to_string()),
        &wave_normal_image,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // Create UV test texture for base color to help visualize UV coordinates
    let uv_test_texture = builder.create_uv_test_texture(512, 512)?;
    
    // Create materials with normal maps
    
    // 1. Material with the bump normal map
    let bump_material = builder.add_textured_material(
        Some("BumpMaterial".to_string()),
        Some(uv_test_texture),  // Base color texture (UV grid)
        None,                  // No metallic roughness texture
        Some(bump_normal_texture), // Normal map
        None,                  // No occlusion texture
        None,                  // No emissive texture
        None,                  // No emissive factor
        Some(0.5),             // Moderate metallic factor
        Some(0.7),             // Moderate roughness
        None,                  // Default alpha mode
        None,                  // No alpha cutoff
        None                   // Not necessarily double sided
    )?;
    
    // 2. Material with the wave normal map
    let wave_material = builder.add_textured_material(
        Some("WaveMaterial".to_string()),
        Some(uv_test_texture),  // Base color texture (UV grid)
        None,                  // No metallic roughness texture
        Some(wave_normal_texture), // Normal map
        None,                  // No occlusion texture
        None,                  // No emissive texture
        None,                  // No emissive factor
        Some(0.5),             // Moderate metallic factor
        Some(0.7),             // Moderate roughness
        None,                  // Default alpha mode
        None,                  // No alpha cutoff
        None                   // Not necessarily double sided
    )?;
    
    // 3. Material without normal map for comparison
    let flat_material = builder.create_textured_material(
        Some("FlatMaterial".to_string()),
        uv_test_texture
    )?;
    
    // Create plane meshes using the materials
    
    // 1. Flat reference plane with no normal map
    let flat_plane_mesh = builder.create_plane(
        1.5, 1.5,     // width and depth
        10, 10,       // width and depth segments
        Some(flat_material)
    );
    
    // 2. Plane with bump normal map
    let bump_plane_mesh = builder.create_plane(
        1.5, 1.5,     // width and depth
        10, 10,       // width and depth segments
        Some(bump_material)
    );
    
    // 3. Plane with wave normal map
    let wave_plane_mesh = builder.create_plane(
        1.5, 1.5,     // width and depth
        10, 10,       // width and depth segments
        Some(wave_material)
    );
    
    // Create nodes for each plane, positioned in different locations
    
    // 1. Flat reference plane (centered)
    let flat_plane_node = builder.add_node(
        Some("FlatPlaneNode".to_string()),
        Some(flat_plane_mesh),
        Some([-2.0, 0.0, 0.0]),  // Left position
        Some([0.7071, 0.0, 0.0, 0.7071]),  // Rotate 90 degrees around X to face camera
        None,
    );
    
    // 2. Bump normal mapped plane
    let bump_plane_node = builder.add_node(
        Some("BumpPlaneNode".to_string()),
        Some(bump_plane_mesh),
        Some([0.0, 0.0, 0.0]),   // Center position
        Some([0.7071, 0.0, 0.0, 0.7071]),  // Rotate 90 degrees around X to face camera
        None,
    );
    
    // 3. Wave normal mapped plane
    let wave_plane_node = builder.add_node(
        Some("WavePlaneNode".to_string()),
        Some(wave_plane_mesh),
        Some([2.0, 0.0, 0.0]),   // Right position
        Some([0.7071, 0.0, 0.0, 0.7071]),  // Rotate 90 degrees around X to face camera
        None,
    );
    
    // Add a simple directional light to help visualize the normal mapping effect
    // (Note: While glTF doesn't directly support lights in the format,
    // we'll create a node that could be interpreted as a light by viewers)
    let light_node = builder.add_node(
        Some("DirectionalLight".to_string()),
        None,
        Some([0.0, 2.0, 2.0]),  // Position above and in front
        None,
        None,
    );
    
    // Create a scene with all planes and the light
    builder.add_scene(
        Some("NormalMappingDemo".to_string()),
        Some(vec![
            flat_plane_node,
            bump_plane_node,
            wave_plane_node,
            light_node
        ]),
    );
    
    // Export the GLB file
    let output_path = "normal_mapping_demo.glb";
    builder.export_glb(output_path)?;
    
    println!("Successfully exported normal mapping demo: {}", output_path);
    println!("This example demonstrates:");
    println!("  - Left: Flat plane with no normal map");
    println!("  - Center: Flat plane with dome/bump normal map");
    println!("  - Right: Flat plane with wave pattern normal map");
    println!("When viewed in a glTF viewer, the normal-mapped planes appear to have");
    println!("3D surface details despite being completely flat geometrically.");
    
    Ok(())
}
