use mesh_tools::GltfBuilder;
use std::error::Error;
use std::fs::File;
use std::path::Path;
use image::io::Reader as ImageReader;

fn main() -> Result<(), Box<dyn Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();
    
    // Load the Rustacean image
    let image_path = Path::new("examples/rustacean-flat-happy.png");
    let image = ImageReader::open(image_path)?
        .with_guessed_format()?
        .decode()?;
    
    println!("Loaded image: {}x{}", image.width(), image.height());
    
    // Create a texture from the loaded image
    let rustacean_texture = builder.create_texture_from_image(
        Some("RustaceanTexture".to_string()),
        &image,
        mesh_tools::texture::TextureFormat::PNG
    )?;
    
    // Create a material using the Rustacean texture
    let rustacean_material = builder.create_textured_material(
        Some("Rustacean Material".to_string()),
        rustacean_texture
    )?;
    
    // Create a flat plate with the Rustacean texture
    let plate_mesh = builder.create_plane(
        2.0,    // width
        2.0,    // depth
        1,      // width segments
        1,      // depth segments
        Some(rustacean_material)
    );
    
    // Create a node for the plate, positioned to be visible
    let plate_node = builder.add_node(
        Some("RustaceanPlateNode".to_string()),
        Some(plate_mesh),
        Some([0.0, 0.0, 0.0]),  // Center position
        Some([0.7071, 0.0, 0.0, 0.7071]),  // Rotate 90 degrees around X to face camera
        None,
    );
    
    // Create a scene with the plate
    builder.add_scene(
        Some("RustaceanScene".to_string()),
        Some(vec![plate_node]),
    );
    
    // Export the GLB file
    let output_path = "rustacean_plate_demo.glb";
    builder.export_glb(output_path)?;
    
    println!("Successfully exported GLB file with Rustacean plate: {}", output_path);
    
    Ok(())
}
