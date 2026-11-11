#![allow(deprecated)] // add_textured_material is kept for this demo; new code uses MaterialBuilder
use mesh_tools::GltfBuilder;
use std::error::Error;
use image::{DynamicImage, ImageBuffer, Rgba};

// Function to create a simple pulsing emissive texture
// This creates a radial gradient that can be used for a glowing orb effect
fn create_radial_emissive_texture(width: u32, height: u32, color: [u8; 3]) -> DynamicImage {
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
            
            // Create a radial gradient - brighter at center
            // For emissive, intensity is how bright it glows
            let intensity = (1.0 - n_dist * n_dist) * 0.8;
            
            // Apply color with intensity
            let r = (color[0] as f32 * intensity) as u8;
            let g = (color[1] as f32 * intensity) as u8;
            let b = (color[2] as f32 * intensity) as u8;
            
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

// Function to create a grid of glowing lines
fn create_grid_emissive_texture(width: u32, height: u32, color: [u8; 3], line_width: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    let grid_size = width.min(height) / 8;
    
    for y in 0..height {
        for x in 0..width {
            // Check if we're on a grid line
            let on_h_line = y % grid_size < line_width;
            let on_v_line = x % grid_size < line_width;
            
            // Grid lines are emissive, background is black
            if on_h_line || on_v_line {
                img.put_pixel(x, y, Rgba([color[0], color[1], color[2], 255]));
            } else {
                img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

// Function to create a "hot" texture that simulates lava or molten material
fn create_lava_emissive_texture(width: u32, height: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    
    // Define colors for the lava effect - from bright yellow to dark red
    let colors = [
        [255, 255, 0],   // Bright yellow
        [255, 200, 0],   // Orange yellow
        [255, 100, 0],   // Orange
        [255, 50, 0],    // Red-orange
        [200, 0, 0],     // Red
        [100, 0, 0],     // Dark red
    ];
    
    for y in 0..height {
        for x in 0..width {
            // Create some pseudo-random patterns based on position
            let noise_x = ((x as f32 * 0.05).sin() + (y as f32 * 0.03).cos()) * 0.5 + 0.5;
            let noise_y = ((y as f32 * 0.04).sin() + (x as f32 * 0.06).cos()) * 0.5 + 0.5;
            let noise = (noise_x + noise_y) * 0.5;
            
            // Determine which color to use based on the noise value
            // Make sure we clamp the index to be within bounds
            let color_idx = ((noise * (colors.len() as f32 - 1.0)).min((colors.len() - 1) as f32)) as usize;
            let color = colors[color_idx];
            
            // Adjust brightness slightly for more variation
            let brightness = 0.8 + (((x as f32 * 0.01).sin() + (y as f32 * 0.01).cos()) * 0.2);
            
            let r = (color[0] as f32 * brightness) as u8;
            let g = (color[1] as f32 * brightness) as u8;
            let b = (color[2] as f32 * brightness) as u8;
            
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

fn main() -> Result<(), Box<dyn Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();
    
    // Create emissive textures
    
    // 1. Blue radial glow texture
    let blue_glow_image = create_radial_emissive_texture(512, 512, [0, 100, 255]);
    let blue_glow_texture = builder.create_texture_from_image(
        Some("BlueGlowTexture".to_string()),
        &blue_glow_image,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // 2. Green grid emissive texture
    let green_grid_image = create_grid_emissive_texture(512, 512, [0, 255, 100], 4);
    let green_grid_texture = builder.create_texture_from_image(
        Some("GreenGridTexture".to_string()),
        &green_grid_image,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // 3. Lava/fire emissive texture
    let lava_image = create_lava_emissive_texture(512, 512);
    let lava_texture = builder.create_texture_from_image(
        Some("LavaTexture".to_string()),
        &lava_image,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // Create materials with emissive textures
    
    // 1. Blue glowing orb material
    let blue_glow_material = builder.add_textured_material(
        Some("BlueGlowMaterial".to_string()),
        None,                     // No base color texture
        None,                     // No metallic roughness texture
        None,                     // No normal map
        None,                     // No occlusion texture
        Some(blue_glow_texture),  // Emissive texture
        Some([1.0, 1.0, 1.0]),    // Full emissive factor (multiplies texture)
        Some(0.0),                // Non-metallic
        Some(1.0),                // Rough
        None,                     // Default alpha mode
        None,                     // No alpha cutoff
        None                      // Not necessarily double sided
    )?;
    
    // 2. Green grid emissive material
    let green_grid_material = builder.add_textured_material(
        Some("GreenGridMaterial".to_string()),
        None,                      // No base color texture
        None,                      // No metallic roughness texture
        None,                      // No normal map
        None,                      // No occlusion texture
        Some(green_grid_texture),  // Emissive texture
        Some([1.0, 1.0, 1.0]),     // Full emissive factor
        Some(0.0),                 // Non-metallic
        Some(1.0),                 // Rough
        None,                      // Default alpha mode
        None,                      // No alpha cutoff
        None                       // Not necessarily double sided
    )?;
    
    // 3. Lava material
    let lava_material = builder.add_textured_material(
        Some("LavaMaterial".to_string()),
        Some(lava_texture),       // Use lava texture as base color too
        None,                     // No metallic roughness texture
        None,                     // No normal map
        None,                     // No occlusion texture
        Some(lava_texture),       // Same texture for emissive
        Some([0.8, 0.8, 0.8]),    // Strong but not full emissive factor
        Some(0.0),                // Non-metallic
        Some(0.7),                // Roughish surface (like molten rock)
        None,                     // Default alpha mode
        None,                     // No alpha cutoff
        None                      // Not necessarily double sided
    )?;
    
    // Create meshes with emissive materials
    
    // 1. Blue glowing sphere
    let blue_sphere_mesh = builder.create_sphere(
        1.0,     // radius
        32,      // width segments
        16,      // height segments
        Some(blue_glow_material)
    );
    
    // 2. Green grid cube
    let green_cube_mesh = builder.create_box_with_material(
        1.0,
        Some(green_grid_material)
    );
    
    // 3. Lava torus
    let lava_torus_mesh = builder.create_torus(
        1.0,    // radius
        0.4,    // tube
        16,     // radial segments
        48,     // tubular segments
        Some(lava_material)
    );
    
    // Create nodes for each object
    
    // 1. Blue glowing sphere
    let blue_sphere_node = builder.add_node(
        Some("BlueGlowSphere".to_string()),
        Some(blue_sphere_mesh),
        Some([-2.5, 0.0, 0.0]),  // Left position
        None,
        None,
    );
    
    // 2. Green grid cube
    let green_cube_node = builder.add_node(
        Some("GreenGridCube".to_string()),
        Some(green_cube_mesh),
        Some([0.0, 0.0, 0.0]),   // Center position
        None,
        None,
    );
    
    // 3. Lava torus
    let lava_torus_node = builder.add_node(
        Some("LavaTorus".to_string()),
        Some(lava_torus_mesh),
        Some([2.5, 0.0, 0.0]),   // Right position
        None,
        None,
    );
    
    // Create a scene with all objects
    builder.add_scene(
        Some("EmissiveDemo".to_string()),
        Some(vec![
            blue_sphere_node,
            green_cube_node,
            lava_torus_node
        ]),
    );
    
    // Export the GLB file
    let output_path = "emissive_mapping_demo.glb";
    builder.export_glb(output_path)?;
    
    println!("Successfully exported GLB file with emissive materials: {}", output_path);
    println!("This example demonstrates:");
    println!("  - Left: Sphere with blue radial glow emissive texture");
    println!("  - Center: Cube with green grid emissive texture");
    println!("  - Right: Torus with lava/fire emissive texture");
    println!("Emissive textures make objects appear to emit light, creating effects");
    println!("like glowing objects, screens, lava, and other self-illuminated materials.");
    
    Ok(())
}
