#![allow(deprecated)] // add_textured_material is kept for this demo; new code uses MaterialBuilder
use mesh_tools::GltfBuilder;
use std::error::Error;
use image::{DynamicImage, ImageBuffer, Rgba};

// Function to create a roughness gradient texture from smooth to rough
fn create_roughness_gradient(width: u32, height: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    
    for y in 0..height {
        for x in 0..width {
            // Roughness value from 0 (smooth) to 1 (rough) from left to right
            let roughness = x as f32 / width as f32;
            
            // In roughness maps, black (0) is smooth, white (255) is rough
            let value = (roughness * 255.0) as u8;
            
            img.put_pixel(x, y, Rgba([value, value, value, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

// Function to create a pattern with varying roughness levels
fn create_roughness_pattern(width: u32, height: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    let _pattern_size = width / 8;
    
    for y in 0..height {
        for x in 0..width {
            // Calculate position within the overall texture
            let pos_x = x as f32 / width as f32;
            let pos_y = y as f32 / height as f32;
            
            // Create concentric rings with alternating roughness
            let center_x = 0.5;
            let center_y = 0.5;
            let dx = pos_x - center_x;
            let dy = pos_y - center_y;
            let dist = ((dx * dx + dy * dy) * 20.0).sqrt();
            
            // Use sin wave to create rings pattern
            let roughness = (dist.sin() * 0.5 + 0.5).max(0.0).min(1.0);
            
            // Scale to 0-255 range
            let value = (roughness * 255.0) as u8;
            
            img.put_pixel(x, y, Rgba([value, value, value, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

// Function to create a weathered/worn roughness texture
fn create_weathered_texture(width: u32, height: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    
    for y in 0..height {
        for x in 0..width {
            // Generate noise patterns to simulate weathering
            let noise_x = ((x as f32 * 0.02).sin() + (y as f32 * 0.03).cos()) * 0.5 + 0.5;
            let noise_y = ((y as f32 * 0.02).sin() + (x as f32 * 0.02).cos()) * 0.5 + 0.5;
            let noise1 = (noise_x + noise_y) * 0.5;
            
            // Second noise pattern at different frequency
            let noise_x2 = ((x as f32 * 0.1).sin() + (y as f32 * 0.08).cos()) * 0.5 + 0.5;
            let noise_y2 = ((y as f32 * 0.09).sin() + (x as f32 * 0.07).cos()) * 0.5 + 0.5;
            let noise2 = (noise_x2 + noise_y2) * 0.5;
            
            // Edge wear pattern: edges are smoother (lower roughness)
            // while center areas are rougher
            let center_x = width as f32 * 0.5;
            let center_y = height as f32 * 0.5;
            let dx = (x as f32 - center_x) / center_x;
            let dy = (y as f32 - center_y) / center_y;
            let edge_factor = (dx * dx + dy * dy).sqrt();
            
            // Combine base roughness with noise and edge factor
            // Base roughness is relatively high (0.7)
            // Edges are smoother (lower roughness)
            // Noise adds variation
            let roughness = 0.7 - (edge_factor * 0.4) + (noise1 * 0.3) - (noise2 * 0.2);
            
            // Ensure it's in 0-1 range then scale to 0-255
            let value = (roughness.max(0.0).min(1.0) * 255.0) as u8;
            
            img.put_pixel(x, y, Rgba([value, value, value, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

// Function to create a scratched surface roughness texture
fn create_scratched_texture(width: u32, height: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    
    // Fill with base roughness (moderately smooth)
    let base_roughness = 60; // Smooth base surface
    for y in 0..height {
        for x in 0..width {
            img.put_pixel(x, y, Rgba([base_roughness, base_roughness, base_roughness, 255]));
        }
    }
    
    // Add random scratches (higher roughness)
    let num_scratches = 100;
    let scratch_roughness = 230; // Rough scratches
    
    for _ in 0..num_scratches {
        // Random scratch position and parameters
        let start_x = (rand_float() * width as f32) as i32;
        let start_y = (rand_float() * height as f32) as i32;
        let length = (rand_float() * 100.0 + 20.0) as i32;
        let angle = rand_float() * std::f32::consts::PI * 2.0;
        let width = (rand_float() * 2.0 + 1.0) as i32;
        
        // Draw the scratch
        for i in 0..length {
            let x = start_x + (i as f32 * angle.cos()) as i32;
            let y = start_y + (i as f32 * angle.sin()) as i32;
            
            // Make sure we're within bounds
            if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                // Draw width of scratch
                for w in -width/2..=width/2 {
                    let sx = x + w;
                    if sx >= 0 && sx < width as i32 {
                        img.put_pixel(sx as u32, y as u32, Rgba([scratch_roughness, scratch_roughness, scratch_roughness, 255]));
                    }
                }
            }
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

// Simple random number function (0-1)
fn rand_float() -> f32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos() as f32;
    (now / 1_000_000_000.0).fract()
}

fn main() -> Result<(), Box<dyn Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();
    
    // Create base color texture (neutral gray for better roughness visibility)
    let create_base_color = || -> Result<DynamicImage, Box<dyn Error>> {
        let color_value = 150; // Medium gray
        let mut img = ImageBuffer::new(512, 512);
        for y in 0..512 {
            for x in 0..512 {
                img.put_pixel(x, y, Rgba([color_value, color_value, color_value, 255]));
            }
        }
        Ok(DynamicImage::ImageRgba8(img))
    };
    
    let base_color_img = create_base_color()?;
    let base_color_texture = builder.create_texture_from_image(
        Some("BaseColor".to_string()),
        &base_color_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // Create roughness textures
    let gradient_img = create_roughness_gradient(512, 512);
    let pattern_img = create_roughness_pattern(512, 512);
    let weathered_img = create_weathered_texture(512, 512);
    let scratched_img = create_scratched_texture(512, 512);
    
    let gradient_texture = builder.create_texture_from_image(
        Some("RoughnessGradient".to_string()),
        &gradient_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    let pattern_texture = builder.create_texture_from_image(
        Some("RoughnessPattern".to_string()),
        &pattern_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    let weathered_texture = builder.create_texture_from_image(
        Some("WeatheredRoughness".to_string()),
        &weathered_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    let scratched_texture = builder.create_texture_from_image(
        Some("ScratchedRoughness".to_string()),
        &scratched_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // Create materials with various roughness textures
    
    // 1. Roughness gradient material
    let gradient_material = builder.add_textured_material(
        Some("GradientRoughnessMaterial".to_string()),
        Some(base_color_texture),    // Base color texture
        Some(gradient_texture),      // Roughness in G channel
        None,                        // No normal map
        None,                        // No occlusion texture
        None,                        // No emissive texture
        None,                        // No emissive factor
        Some(0.0),                   // Non-metallic
        None,                        // Roughness comes from texture
        None,                        // Default alpha mode
        None,                        // No alpha cutoff
        Some(true)                   // Double sided
    )?;
    
    // 2. Concentric pattern roughness material
    let pattern_material = builder.add_textured_material(
        Some("PatternRoughnessMaterial".to_string()),
        Some(base_color_texture),    // Base color texture
        Some(pattern_texture),       // Roughness in G channel
        None,                        // No normal map
        None,                        // No occlusion texture
        None,                        // No emissive texture
        None,                        // No emissive factor
        Some(0.0),                   // Non-metallic
        None,                        // Roughness comes from texture
        None,                        // Default alpha mode
        None,                        // No alpha cutoff
        None                         // Not necessarily double sided
    )?;
    
    // 3. Weathered surface material
    let weathered_material = builder.add_textured_material(
        Some("WeatheredMaterial".to_string()),
        Some(base_color_texture),    // Base color texture
        Some(weathered_texture),     // Roughness in G channel
        None,                        // No normal map
        None,                        // No occlusion texture
        None,                        // No emissive texture
        None,                        // No emissive factor
        Some(0.1),                   // Slightly metallic
        None,                        // Roughness comes from texture
        None,                        // Default alpha mode
        None,                        // No alpha cutoff
        None                         // Not necessarily double sided
    )?;
    
    // 4. Scratched surface material
    let scratched_material = builder.add_textured_material(
        Some("ScratchedMaterial".to_string()),
        Some(base_color_texture),    // Base color texture
        Some(scratched_texture),     // Roughness in G channel
        None,                        // No normal map
        None,                        // No occlusion texture
        None,                        // No emissive texture
        None,                        // No emissive factor
        Some(0.8),                   // Metallic (to show scratches better)
        None,                        // Roughness comes from texture
        None,                        // Default alpha mode
        None,                        // No alpha cutoff
        None                         // Not necessarily double sided
    )?;
    
    // Create plain materials with fixed roughness for comparison
    let smooth_material = builder.add_material(
        Some("SmoothMaterial".to_string()),
        Some([0.5, 0.5, 0.5, 1.0]),  // Base color factor
        Some(0.0),                  // Non-metallic
        Some(0.0),                  // Completely smooth (low roughness)
        None                        // Not necessarily double sided
    );
    
    let rough_material = builder.add_material(
        Some("RoughMaterial".to_string()),
        Some([0.5, 0.5, 0.5, 1.0]),  // Base color factor
        Some(0.0),                  // Non-metallic
        Some(1.0),                  // Completely rough (high roughness)
        None                        // Not necessarily double sided
    );
    
    // Create geometries for each material
    
    // Smooth and rough spheres for comparison
    let smooth_sphere = builder.create_sphere(
        0.8, 32, 16, Some(smooth_material)
    );
    
    let rough_sphere = builder.create_sphere(
        0.8, 32, 16, Some(rough_material)
    );
    
    // Gradient roughness plane
    let gradient_plane = builder.create_plane(
        2.0, 2.0, 1, 1, Some(gradient_material)
    );
    
    // Pattern roughness sphere
    let pattern_sphere = builder.create_sphere(
        1.0, 32, 16, Some(pattern_material)
    );
    
    // Weathered roughness cube
    let weathered_cube = builder.create_box_with_material(
        1.5, Some(weathered_material)
    );
    
    // Scratched roughness torus
    let scratched_torus = builder.create_torus(
        1.0,  // Radius
        0.4,  // Tube radius
        32,   // Radial segments
        48,   // Tubular segments
        Some(scratched_material)
    );
    
    // Create and position nodes
    let smooth_node = builder.add_node(
        Some("SmoothSphere".to_string()),
        Some(smooth_sphere),
        Some([-4.0, 2.0, 0.0]),  // Top left
        None,
        None,
    );
    
    let rough_node = builder.add_node(
        Some("RoughSphere".to_string()),
        Some(rough_sphere),
        Some([-1.5, 2.0, 0.0]),  // Top middle-left
        None,
        None,
    );
    
    let gradient_node = builder.add_node(
        Some("RoughnessGradientPlane".to_string()),
        Some(gradient_plane),
        Some([1.5, 2.0, 0.0]),   // Top middle-right
        // Rotate the plane to be vertical
        Some([0.7071, 0.0, 0.0, 0.7071]), // 90 degrees around X axis
        None,
    );
    
    let pattern_node = builder.add_node(
        Some("PatternSphere".to_string()),
        Some(pattern_sphere),
        Some([4.0, 2.0, 0.0]),   // Top right
        None,
        None,
    );
    
    let weathered_node = builder.add_node(
        Some("WeatheredCube".to_string()),
        Some(weathered_cube),
        Some([-2.5, -2.0, 0.0]),  // Bottom left
        None,
        None,
    );
    
    let scratched_node = builder.add_node(
        Some("ScratchedTorus".to_string()),
        Some(scratched_torus),
        Some([2.5, -2.0, 0.0]),   // Bottom right
        // Rotate the torus to show its surface better
        Some([0.7071, 0.0, 0.0, 0.7071]), // 90 degrees around X axis
        None,
    );
    
    // Create a scene with all nodes
    builder.add_scene(
        Some("RoughnessDemo".to_string()),
        Some(vec![
            smooth_node,
            rough_node,
            gradient_node,
            pattern_node,
            weathered_node,
            scratched_node,
        ]),
    );
    
    // Export the GLB file
    let output_path = "roughness_demo.glb";
    builder.export_glb(output_path)?;
    
    println!("Successfully exported GLB file with roughness examples: {}", output_path);
    println!("This example demonstrates:");
    println!("  - Top row: Basic roughness comparison and patterns");
    println!("    - Far Left: Smooth sphere (roughness=0.0)");
    println!("    - Middle Left: Rough sphere (roughness=1.0)");
    println!("    - Middle Right: Gradient plane (roughness from 0.0 to 1.0 left to right)");
    println!("    - Far Right: Sphere with concentric roughness pattern");
    println!("  - Bottom row: More complex roughness textures");
    println!("    - Left: Weathered cube with edge wear pattern");
    println!("    - Right: Metallic torus with scratch marks");
    println!();
    println!("In PBR rendering, roughness controls how light scatters when hitting a surface:");
    println!("  - Smooth surfaces (0.0) create sharp, mirror-like reflections");
    println!("  - Rough surfaces (1.0) create diffuse, scattered reflections");
    println!("  - Real-world materials have varying roughness across their surface");
    println!("  - Roughness textures provide visual detail that makes materials feel realistic");
    
    Ok(())
}
