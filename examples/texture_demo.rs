use mesh_tools::GltfBuilder;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();
    
    // Create different materials
    
    // 1. Create a red material
    let red_material = builder.create_basic_material(
        Some("Red Material".to_string()),
        [1.0, 0.0, 0.0, 1.0], // Red color
    );
    
    // 2. Create a golden material (shiny metal)
    let golden_material = builder.create_metallic_material(
        Some("Golden Material".to_string()),
        [1.0, 0.84, 0.0, 1.0],  // Gold color
        0.9,                    // High metallic factor
        0.1,                    // Low roughness factor (shiny)
    );
    
    // 3. Create a checkerboard texture (black and white)
    let bw_checker_texture = builder.create_checkerboard_texture(
        512,                     // width
        512,                     // height
        64,                      // cell size
        [255, 255, 255],         // white
        [0, 0, 0],               // black
    )?;
    
    // 4. Create a checkerboard texture (blue and gray)
    let blue_checker_texture = builder.create_checkerboard_texture(
        512,                     // width
        512,                     // height
        64,                      // cell size
        [100, 149, 237],         // cornflower blue
        [200, 200, 200],         // light gray
    )?;
    
    // 5. Create a UV test texture
    let uv_test_texture = builder.create_uv_test_texture(
        512,                    // width
        512,                    // height
    )?;
    
    // Create textured materials
    let bw_checker_material = builder.create_textured_material(
        Some("Black & White Checker Material".to_string()),
        bw_checker_texture
    )?;
    
    let blue_checker_material = builder.create_textured_material(
        Some("Blue Checker Material".to_string()),
        blue_checker_texture
    )?;
    
    let uv_test_material = builder.create_textured_material(
        Some("UV Test Material".to_string()),
        uv_test_texture
    )?;
    
    // Create box meshes with different materials
    let red_box_mesh = builder.create_box_with_material(1.0, Some(red_material));
    let gold_box_mesh = builder.create_box_with_material(0.5, Some(golden_material));
    let bw_checker_box_mesh = builder.create_box_with_material(1.0, Some(bw_checker_material));
    let blue_checker_box_mesh = builder.create_box_with_material(0.75, Some(blue_checker_material));
    let uv_test_box_mesh = builder.create_box_with_material(1.2, Some(uv_test_material));
    
    // Create nodes for each box, positioned in different locations
    let red_box_node = builder.add_node(
        Some("RedBoxNode".to_string()),
        Some(red_box_mesh),
        Some([-3.0, 0.0, 0.0]),  // Left position
        None,
        None,
    );
    
    let gold_box_node = builder.add_node(
        Some("GoldBoxNode".to_string()),
        Some(gold_box_mesh),
        Some([-1.5, 0.0, 0.0]),  // Center-left position
        None,
        None,
    );
    
    let bw_checker_box_node = builder.add_node(
        Some("BWCheckerBoxNode".to_string()),
        Some(bw_checker_box_mesh),
        Some([0.0, 0.0, 0.0]),   // Center position
        None,
        None,
    );
    
    let blue_checker_box_node = builder.add_node(
        Some("BlueCheckerBoxNode".to_string()),
        Some(blue_checker_box_mesh),
        Some([1.5, 0.0, 0.0]),   // Center-right position
        None,
        None,
    );
    
    let uv_test_box_node = builder.add_node(
        Some("UVTestBoxNode".to_string()),
        Some(uv_test_box_mesh),
        Some([3.0, 0.0, 0.0]),   // Right position
        None,
        None,
    );
    
    // Create a scene with all boxes
    builder.add_scene(
        Some("Scene".to_string()),
        Some(vec![
            red_box_node, 
            gold_box_node, 
            bw_checker_box_node, 
            blue_checker_box_node,
            uv_test_box_node
        ]),
    );
    
    // Export the GLB file
    let output_path = "texture_demo.glb";
    builder.export_glb(output_path)?;
    
    println!("Successfully exported GLB file with textures: {}", output_path);
    
    Ok(())
}
