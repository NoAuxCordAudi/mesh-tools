#![allow(deprecated)] // add_textured_material is kept for this demo; new code uses MaterialBuilder
use mesh_tools::GltfBuilder;
use std::error::Error;
use image::{DynamicImage, ImageBuffer, Rgba};

// Function to create a metalness gradient texture
// This creates a texture with metalness varying from 0.0 to 1.0 (left to right)
fn create_metalness_gradient(width: u32, height: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    
    for y in 0..height {
        for x in 0..width {
            // Metalness value from 0 to 1 (left to right)
            let metalness = x as f32 / width as f32;
            
            // In metalness maps, white (255) is fully metallic, black (0) is non-metallic
            let value = (metalness * 255.0) as u8;
            
            img.put_pixel(x, y, Rgba([value, value, value, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

// Function to create a pattern of metal and non-metal regions
fn create_metal_pattern(width: u32, height: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    let pattern_size = width / 8;
    
    for y in 0..height {
        for x in 0..width {
            // Create a checkerboard pattern of metal and non-metal
            let pattern_x = x / pattern_size;
            let pattern_y = y / pattern_size;
            
            // Alternate between metallic and non-metallic
            let is_metal = (pattern_x + pattern_y) % 2 == 0;
            
            // 255 for metallic areas, 0 for non-metallic
            let value = if is_metal { 255 } else { 0 };
            
            img.put_pixel(x, y, Rgba([value, value, value, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

// Function to create a fingerprint/smudge pattern on metal
// This simulates a metal surface with varying levels of fingerprints and smudges
fn create_fingerprint_metal(width: u32, height: u32) -> DynamicImage {
    let mut img = ImageBuffer::new(width, height);
    
    for y in 0..height {
        for x in 0..width {
            // Generate some noise patterns to simulate fingerprints/smudges
            let noise_x = ((x as f32 * 0.05).sin() + (y as f32 * 0.06).cos()) * 0.5 + 0.5;
            let noise_y = ((y as f32 * 0.04).sin() + (x as f32 * 0.03).cos()) * 0.5 + 0.5;
            let noise = (noise_x * noise_y) * 0.8;
            
            // Base metalness is high (around 0.8-1.0) but reduced in smudged areas
            let metalness = 0.8 + (0.2 * ((x as f32 * 0.01).cos() * (y as f32 * 0.01).sin()));
            let with_fingerprints = metalness - (noise * 0.5); // Fingerprints reduce metalness
            
            // Scale to 0-255 range
            let value = (with_fingerprints.max(0.0).min(1.0) * 255.0) as u8;
            
            img.put_pixel(x, y, Rgba([value, value, value, 255]));
        }
    }
    
    DynamicImage::ImageRgba8(img)
}

fn main() -> Result<(), Box<dyn Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();
    
    // Create base color textures for gold, copper, silver, aluminum and blue steel
    let gold_color = [255, 215, 0];     // RGB for gold
    let copper_color = [184, 115, 51];  // RGB for copper
    let silver_color = [192, 192, 192]; // RGB for silver
    let aluminum_color = [169, 169, 183]; // RGB for aluminum
    let blue_steel_color = [70, 130, 180]; // RGB for blue steel
    
    // Create a uniform base color texture for each metal type
    let create_color_texture = |color: [u8; 3]| -> Result<DynamicImage, Box<dyn Error>> {
        let mut img = ImageBuffer::new(512, 512);
        for y in 0..512 {
            for x in 0..512 {
                img.put_pixel(x, y, Rgba([color[0], color[1], color[2], 255]));
            }
        }
        Ok(DynamicImage::ImageRgba8(img))
    };
    
    // Create a base color texture for each metal
    let gold_base_img = create_color_texture(gold_color)?;
    let copper_base_img = create_color_texture(copper_color)?;
    let silver_base_img = create_color_texture(silver_color)?;
    let aluminum_base_img = create_color_texture(aluminum_color)?;
    let blue_steel_base_img = create_color_texture(blue_steel_color)?;
    
    // Convert images to textures
    let gold_base_texture = builder.create_texture_from_image(
        Some("GoldBaseColor".to_string()),
        &gold_base_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    let copper_base_texture = builder.create_texture_from_image(
        Some("CopperBaseColor".to_string()),
        &copper_base_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    let silver_base_texture = builder.create_texture_from_image(
        Some("SilverBaseColor".to_string()),
        &silver_base_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    let aluminum_base_texture = builder.create_texture_from_image(
        Some("AluminumBaseColor".to_string()),
        &aluminum_base_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    let blue_steel_base_texture = builder.create_texture_from_image(
        Some("BlueSteelBaseColor".to_string()),
        &blue_steel_base_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // Create metalness textures
    let gradient_metal_img = create_metalness_gradient(512, 512);
    let pattern_metal_img = create_metal_pattern(512, 512);
    let fingerprint_metal_img = create_fingerprint_metal(512, 512);
    
    let gradient_metal_texture = builder.create_texture_from_image(
        Some("MetalnessGradient".to_string()),
        &gradient_metal_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    let pattern_metal_texture = builder.create_texture_from_image(
        Some("MetalnessPattern".to_string()),
        &pattern_metal_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    let fingerprint_metal_texture = builder.create_texture_from_image(
        Some("FingerprintMetalness".to_string()),
        &fingerprint_metal_img,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // 1. Solid metals with fixed metalness values
    let gold_material = builder.add_textured_material(
        Some("GoldMaterial".to_string()),
        Some(gold_base_texture),  // Gold color
        None,                     // No metallic-roughness texture
        None,                     // No normal map
        None,                     // No occlusion texture
        None,                     // No emissive texture
        None,                     // No emissive factor
        Some(1.0),                // Fully metallic
        Some(0.1),                // Very smooth (low roughness)
        None,                     // Default alpha mode
        None,                     // No alpha cutoff
        None                      // Not necessarily double sided
    )?;
    
    let copper_material = builder.add_textured_material(
        Some("CopperMaterial".to_string()),
        Some(copper_base_texture), // Copper color
        None,                      // No metallic-roughness texture
        None,                      // No normal map
        None,                      // No occlusion texture
        None,                      // No emissive texture
        None,                      // No emissive factor
        Some(1.0),                 // Fully metallic
        Some(0.2),                 // Fairly smooth
        None,                      // Default alpha mode
        None,                      // No alpha cutoff
        None                       // Not necessarily double sided
    )?;
    
    let brushed_aluminum_material = builder.add_textured_material(
        Some("BrushedAluminumMaterial".to_string()),
        Some(aluminum_base_texture), // Aluminum color
        None,                        // No metallic-roughness texture
        None,                        // No normal map
        None,                        // No occlusion texture
        None,                        // No emissive texture
        None,                        // No emissive factor
        Some(0.9),                   // High but not fully metallic (slight oxide layer)
        Some(0.3),                   // Moderate roughness (brushed finish)
        None,                        // Default alpha mode
        None,                        // No alpha cutoff
        None                         // Not necessarily double sided
    )?;
    
    // 2. Material with metalness gradient texture
    let gradient_material = builder.add_textured_material(
        Some("GradientMetalMaterial".to_string()),
        Some(silver_base_texture),      // Silver base color
        Some(gradient_metal_texture),   // Metalness gradient in R channel
        None,                           // No normal map
        None,                           // No occlusion texture
        None,                           // No emissive texture
        None,                           // No emissive factor
        None,                           // Metalness comes from texture
        Some(0.25),                     // Moderate roughness
        None,                           // Default alpha mode
        None,                           // No alpha cutoff
        Some(true)                      // Make it double sided
    )?;
    
    // 3. Material with patterned metalness
    let pattern_material = builder.add_textured_material(
        Some("PatternMetalMaterial".to_string()),
        Some(blue_steel_base_texture),  // Blue steel base color
        Some(pattern_metal_texture),    // Patterned metalness in R channel
        None,                           // No normal map
        None,                           // No occlusion texture
        None,                           // No emissive texture
        None,                           // No emissive factor
        None,                           // Metalness comes from texture
        Some(0.4),                      // Medium roughness
        None,                           // Default alpha mode
        None,                           // No alpha cutoff
        None                            // Not necessarily double sided
    )?;
    
    // 4. Material with fingerprint metalness
    let fingerprint_material = builder.add_textured_material(
        Some("FingerprintMetalMaterial".to_string()),
        Some(silver_base_texture),        // Silver base color
        Some(fingerprint_metal_texture),  // Fingerprint metalness in R channel
        None,                             // No normal map
        None,                             // No occlusion texture
        None,                             // No emissive texture
        None,                             // No emissive factor
        None,                             // Metalness comes from texture
        Some(0.1),                        // Low roughness (high polish)
        None,                             // Default alpha mode
        None,                             // No alpha cutoff
        None                              // Not necessarily double sided
    )?;
    
    // Create geometry for each material
    
    // Create spheres for solid metals
    let gold_sphere = builder.create_sphere(
        1.0, 32, 16, Some(gold_material)
    );
    
    let copper_sphere = builder.create_sphere(
        1.0, 32, 16, Some(copper_material)
    );
    
    let aluminum_sphere = builder.create_sphere(
        1.0, 32, 16, Some(brushed_aluminum_material)
    );
    
    // Create geometries for textured metalness
    let gradient_plane = builder.create_plane(
        2.0, 2.0, 1, 1, Some(gradient_material)
    );
    
    let pattern_cube = builder.create_box_with_material(
        1.5, Some(pattern_material)
    );
    
    let fingerprint_torus = builder.create_torus(
        1.0,  // Radius
        0.4,  // Tube radius
        32,   // Radial segments
        48,   // Tubular segments
        Some(fingerprint_material)
    );
    
    // Create and position nodes
    let gold_node = builder.add_node(
        Some("GoldSphere".to_string()),
        Some(gold_sphere),
        Some([-4.0, 2.0, 0.0]),  // Top left
        None,
        None,
    );
    
    let copper_node = builder.add_node(
        Some("CopperSphere".to_string()),
        Some(copper_sphere),
        Some([0.0, 2.0, 0.0]),   // Top middle
        None,
        None,
    );
    
    let aluminum_node = builder.add_node(
        Some("AluminumSphere".to_string()),
        Some(aluminum_sphere),
        Some([4.0, 2.0, 0.0]),   // Top right
        None,
        None,
    );
    
    let gradient_node = builder.add_node(
        Some("MetalGradientPlane".to_string()),
        Some(gradient_plane),
        Some([-4.0, -2.0, 0.0]), // Bottom left
        // Rotate the plane to be vertical and face the camera
        Some([0.0, 0.0, 0.0, 1.0]),
        None,
    );
    
    let pattern_node = builder.add_node(
        Some("PatternMetalCube".to_string()),
        Some(pattern_cube),
        Some([0.0, -2.0, 0.0]),  // Bottom middle
        None,
        None,
    );
    
    let fingerprint_node = builder.add_node(
        Some("FingerprintMetalTorus".to_string()),
        Some(fingerprint_torus),
        Some([4.0, -2.0, 0.0]),  // Bottom right
        // Rotate the torus to show its surface better
        Some([0.7071, 0.0, 0.0, 0.7071]), // 90 degrees around X axis
        None,
    );
    
    // Create a scene with all nodes
    builder.add_scene(
        Some("MetalnessDemo".to_string()),
        Some(vec![
            gold_node,
            copper_node,
            aluminum_node,
            gradient_node,
            pattern_node,
            fingerprint_node,
        ]),
    );
    
    // Export the GLB file
    let output_path = "metalness_demo.glb";
    builder.export_glb(output_path)?;
    
    println!("Successfully exported GLB file with metalness examples: {}", output_path);
    println!("This example demonstrates:");
    println!("  - Top row: Solid metal spheres with different pure metalness values");
    println!("    - Left: Gold (metalness=1.0, roughness=0.1)");
    println!("    - Middle: Copper (metalness=1.0, roughness=0.2)");
    println!("    - Right: Brushed Aluminum (metalness=0.9, roughness=0.3)");
    println!("  - Bottom row: Objects with metalness textures");
    println!("    - Left: Gradient plane (metalness varies from 0.0 to 1.0 left to right)");
    println!("    - Middle: Patterned cube (checkerboard of metallic and non-metallic areas)");
    println!("    - Right: Torus with fingerprint/smudge patterns on metal surface");
    println!();
    println!("In PBR rendering, metalness controls how the material interacts with light:");
    println!("  - Metallic surfaces (1.0) reflect light as specular reflections and use base color as specular color");
    println!("  - Non-metallic surfaces (0.0) have diffuse reflection and grayscale specular reflection");
    println!("  - Mixed surfaces have a blend of these properties based on the metalness value");
    
    Ok(())
}
