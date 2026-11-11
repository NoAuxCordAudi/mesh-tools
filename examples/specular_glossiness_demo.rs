use mesh_tools::GltfBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();
    
    // Set up the scene with multiple objects showcasing specular materials
    let mut objects = Vec::new();
    
    // Position objects in a grid for comparison
    let positions = [
        [-2.5, 0.0, -2.5],  // Metallic-roughness (for comparison)
        [0.0, 0.0, -2.5],   // Specular glossiness (blue plastic)
        [2.5, 0.0, -2.5],   // Specular glossiness (gold)
        [-2.5, 0.0, 0.0],   // Specular glossiness (red plastic)
        [0.0, 0.0, 0.0],    // Specular glossiness (green plastic)
        [2.5, 0.0, 0.0],    // Specular glossiness (chrome)
        [-2.5, 0.0, 2.5],   // Specular glossiness (white ceramic)
        [0.0, 0.0, 2.5],    // Specular glossiness (black rubber)
        [2.5, 0.0, 2.5],    // Specular glossiness (copper)
    ];
    
    // Create a standard metallic-roughness material for comparison
    let metallic_material = builder.create_metallic_material(
        Some("Standard Metal".to_string()),
        [0.8, 0.8, 0.8, 1.0], // Silver-like base color
        1.0,  // Fully metallic
        0.2,  // Fairly smooth
    );
    
    // Create a range of specular materials with different properties
    
    // Blue plastic material (low specular intensity, high glossiness)
    let blue_plastic_material = builder.create_specular_material(
        Some("Blue Plastic".to_string()),
        [0.1, 0.1, 0.8, 1.0],  // Blue diffuse color
        [0.2, 0.2, 0.2],       // Low specular intensity
        0.9,                   // High glossiness
    )?;
    
    // Gold material (yellow color with golden specular)
    let gold_material = builder.create_specular_material(
        Some("Gold".to_string()),
        [1.0, 0.8, 0.2, 1.0],  // Gold diffuse color
        [1.0, 0.9, 0.4],       // Gold specular color
        0.85,                  // High glossiness
    )?;
    
    // Red plastic material (medium specular)
    let red_plastic_material = builder.create_specular_material(
        Some("Red Plastic".to_string()),
        [0.9, 0.1, 0.1, 1.0],  // Red diffuse color
        [0.3, 0.3, 0.3],       // Medium specular intensity
        0.8,                   // High glossiness
    )?;
    
    // Green plastic material (high specular)
    let green_plastic_material = builder.create_specular_material(
        Some("Green Plastic".to_string()),
        [0.1, 0.8, 0.1, 1.0],  // Green diffuse color
        [0.5, 0.5, 0.5],       // High specular intensity
        0.7,                   // Medium-high glossiness
    )?;
    
    // Chrome-like material (high specular, high glossiness)
    let chrome_material = builder.create_specular_material(
        Some("Chrome".to_string()),
        [0.8, 0.8, 0.8, 1.0],  // Light gray diffuse
        [0.95, 0.95, 0.95],    // Very bright specular
        0.95,                  // Very high glossiness
    )?;
    
    // White ceramic material (medium specular, medium glossiness)
    let white_ceramic_material = builder.create_specular_material(
        Some("White Ceramic".to_string()),
        [0.95, 0.95, 0.95, 1.0], // White diffuse
        [0.4, 0.4, 0.4],       // Medium specular
        0.6,                   // Medium glossiness
    )?;
    
    // Black rubber material (very low specular, low glossiness)
    let black_rubber_material = builder.create_specular_material(
        Some("Black Rubber".to_string()),
        [0.1, 0.1, 0.1, 1.0],  // Black diffuse
        [0.05, 0.05, 0.05],    // Very low specular
        0.2,                   // Low glossiness (rough)
    )?;
    
    // Copper material (reddish metal)
    let copper_material = builder.create_specular_material(
        Some("Copper".to_string()),
        [0.95, 0.5, 0.3, 1.0], // Copper diffuse color
        [0.98, 0.65, 0.45],    // Copper specular color
        0.85,                  // High glossiness
    )?;
    
    // Collection of materials in the same order as positions
    let materials = [
        metallic_material,
        blue_plastic_material,
        gold_material,
        red_plastic_material,
        green_plastic_material,
        chrome_material,
        white_ceramic_material,
        black_rubber_material,
        copper_material,
    ];
    
    // Create spheres with different materials
    for i in 0..positions.len() {
        // Create a sphere mesh
        let sphere_mesh = builder.create_sphere(
            1.0,           // Radius of 1.0
            36,            // 36 segments for smooth sphere
            18,            // 18 rings for smooth sphere
            Some(materials[i]),  // Apply the corresponding material
        );
        
        // Position the sphere
        let node = builder.add_node(
            Some(format!("Sphere {}", i)),
            Some(sphere_mesh),
            Some([positions[i][0], positions[i][1], positions[i][2]]),
            None, // No rotation
            None, // Default scale
        );
        
        objects.push(node);
    }
    
    // Add ground plane
    let ground_material = builder.create_basic_material(
        Some("Ground".to_string()),
        [0.5, 0.5, 0.5, 1.0]
    );
    
    let ground_mesh = builder.create_plane(
        10.0,           // Width
        10.0,           // Depth
        1,              // Width segments
        1,              // Depth segments
        Some(ground_material) // Use the ground material
    );
    
    let ground_node = builder.add_node(
        Some("Ground".to_string()),
        Some(ground_mesh),
        Some([0.0, -1.0, 0.0]), // Position
        Some([0.0, 0.0, 0.0, 1.0]), // No rotation (quaternion format)
        None, // Default scale
    );
    
    objects.push(ground_node);
    
    // Create a scene with all objects
    builder.add_scene(
        Some("Specular Glossiness Demo".to_string()),
        Some(objects),
    );
    
    // Export the model
    builder.export_glb("specular_glossiness_demo.glb")?;
    
    println!("Created specular_glossiness_demo.glb");
    
    Ok(())
}
