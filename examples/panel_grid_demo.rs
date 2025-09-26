use mesh_tools::GltfBuilder;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();

    // Create materials
    let metal_material = builder.create_metallic_material(
        Some("metal".to_string()),
        [0.8, 0.8, 0.9, 1.0],  // Metallic silver color
        0.95, // High metallic factor
        0.2,  // Low roughness (shiny)
    );

    let dirt_material = builder.create_basic_material(
        Some("dirt".to_string()),
        [0.4, 0.3, 0.2, 1.0], // Brown dirt color
    );

    let glass_material = builder.create_metallic_material(
        Some("glass".to_string()),
        [0.7, 0.9, 1.0, 0.8],  // Light blue with transparency
        0.1, // Low metallic factor
        0.0, // Very low roughness (very shiny)
    );

    // LEVEL 1: Create base panel meshes (2x2m each)
    let metal_panel_mesh = builder.create_plane(
        2.0,  // width: 2 meters
        2.0,  // depth: 2 meters
        1,    // width segments: 1 (simple quad)
        1,    // depth segments: 1 (simple quad)
        Some(metal_material)
    );

    let dirt_panel_mesh = builder.create_plane(
        2.0,  // width: 2 meters
        2.0,  // depth: 2 meters
        1,    // width segments: 1 (simple quad)
        1,    // depth segments: 1 (simple quad)
        Some(dirt_material)
    );

    let glass_panel_mesh = builder.create_plane(
        2.0,  // width: 2 meters
        2.0,  // depth: 2 meters
        1,    // width segments: 1 (simple quad)
        1,    // depth segments: 1 (simple quad)
        Some(glass_material)
    );

    // LEVEL 2 & 3: Create sections with panel groups using true mesh instancing
    let panel_spacing = 2.5; // 0.5m gap between panels
    let group_spacing = 6.0; // 1m gap between panel groups
    let section_spacing = 14.0; // Space between section centers

    // Simple pseudo-random number generator for material selection
    let mut seed = 12345u32;

    // Create sections with their own panel groups (true instancing at mesh level)
    let mut world_sections = Vec::new();

    for section_id in 0..4 {
        let mut section_groups = Vec::new();
        
        // Arrange sections in a 2x2 grid
        let section_row = section_id / 2;
        let section_col = section_id % 2;
        let section_x = (section_col as f32 - 0.5) * section_spacing;
        let section_z = (section_row as f32 - 0.5) * section_spacing;
        
        // Create a 2x2 arrangement of panel groups within each section
        for row in 0..2 {
            for col in 0..2 {
                // Choose which panel group pattern to use (cycling through the 4 types)
                let group_pattern = (section_id + row + col) % 4;
                
                // Position within the section
                let group_x = section_x + (col as f32 - 0.5) * group_spacing;
                let group_z = section_z + (row as f32 - 0.5) * group_spacing;

                // Create a 2x2 group of panels for this group
                let mut group_panels = Vec::new();
                
                for panel_row in 0..2 {
                    for panel_col in 0..2 {
                        // Generate next pseudo-random number
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        let random_value = seed >> 16;

                        // Choose material based on group pattern and randomness
                        let panel_mesh = match (group_pattern + (random_value as usize % 3)) % 3 {
                            0 => metal_panel_mesh,
                            1 => dirt_panel_mesh,
                            _ => glass_panel_mesh,
                        };

                        // Position within the group (relative to group center)
                        let panel_x = group_x + (panel_col as f32 - 0.5) * panel_spacing;
                        let panel_z = group_z + (panel_row as f32 - 0.5) * panel_spacing;

                        let panel_node = builder.add_node(
                            Some(format!("S{}_G{}{}_Panel_{}_{}", section_id, row, col, panel_row, panel_col)),
                            Some(panel_mesh),
                            Some([panel_x, 0.0, panel_z]),
                            None,
                            None,
                        );

                        group_panels.push(panel_node);
                    }
                }

                // Create a parent node for this panel group
                let group_node = builder.add_node(
                    Some(format!("S{}_Group_{}_{}", section_id, row, col)),
                    None, // No mesh, just a container
                    Some([group_x, 0.0, group_z]),
                    None,
                    None,
                );

                // Add all group panels as children of the group node
                for panel in group_panels {
                    builder.add_child_to_node(group_node, panel)?;
                }

                section_groups.push(group_node);
            }
        }

        // Create a parent node for this section
        let section_node = builder.add_node(
            Some(format!("Section_{}", section_id)),
            None, // No mesh, just a container
            Some([section_x, 0.0, section_z]),
            None,
            None,
        );

        // Add all section groups as children of the section node
        for group in section_groups {
            builder.add_child_to_node(section_node, group)?;
        }

        world_sections.push(section_node);
    }

    // Add a viewpoint node for better camera positioning
    // Normalize the rotation quaternion
    let quat = [-0.3f32, 0.4f32, 0.1f32, 0.9f32];
    let magnitude = (quat[0]*quat[0] + quat[1]*quat[1] + quat[2]*quat[2] + quat[3]*quat[3]).sqrt();
    let normalized_quat = [quat[0]/magnitude, quat[1]/magnitude, quat[2]/magnitude, quat[3]/magnitude];
    
    let viewpoint = builder.add_node(
        Some("Viewpoint".to_string()),
        None, // No mesh for viewpoint
        Some([20.0, 15.0, 20.0]), // Position for a good diagonal view of the larger scene
        Some(normalized_quat), // Normalized rotation quaternion looking down at the grid
        None,
    );

    // Create a scene with all our sections
    builder.add_scene(
        Some("Multi-Level Panel Grid Demo".to_string()),
        Some(world_sections),
    );

    // Create a camera scene
    builder.add_scene(
        Some("Camera View".to_string()),
        Some(vec![viewpoint]),
    );

    // Set the default scene to be the first scene (the panel grid)
    if let Some(scenes) = &mut builder.gltf.scenes {
        if !scenes.is_empty() {
            builder.gltf.scene = Some(0);
        }
    }

    // Export the GLB file
    let output_path = "panel_grid_demo.glb";
    builder.export_glb(output_path)?;

    println!("Successfully exported multi-level panel grid demo: {}", output_path);
    println!("");
    println!("This example demonstrates THREE LEVELS of instancing:");
    println!("");
    println!("LEVEL 1 - Base Meshes (3 types):");
    println!("  • Metal panels (metallic silver)");
    println!("  • Dirt panels (brown basic material)"); 
    println!("  • Glass panels (transparent blue)");
    println!("");
    println!("LEVEL 2 - Panel Groups (4 types):");
    println!("  • Each group contains 4 panels (2x2 arrangement)");
    println!("  • Groups use different combinations of the 3 base panel types");
    println!("  • Panel spacing: 2.5m between centers");
    println!("");
    println!("LEVEL 3 - Sections (4 instances):");
    println!("  • Each section contains 4 panel groups (2x2 arrangement)");
    println!("  • Sections reuse the 4 panel group types in different patterns");
    println!("  • Group spacing: 6m between centers");
    println!("");
    println!("FINAL RESULT:");
    println!("  • 4 sections × 4 groups × 4 panels = 64 total panels");
    println!("  • Only 3 base meshes created, reused 64 times through instancing");
    println!("  • Hierarchical organization: World → Sections → Groups → Panels");
    println!("  • Total scene size: ~28m × 28m");

    Ok(())
}
