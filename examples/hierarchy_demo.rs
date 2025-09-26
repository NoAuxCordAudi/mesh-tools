use mesh_tools::GltfBuilder;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();
    
    // Create materials for our objects
    let red_material = builder.create_basic_material(
        Some("Red Material".to_string()),
        [1.0, 0.0, 0.0, 1.0], // Red color
    );
    
    let blue_material = builder.create_basic_material(
        Some("Blue Material".to_string()),
        [0.0, 0.0, 1.0, 1.0], // Blue color
    );
    
    let green_material = builder.create_basic_material(
        Some("Green Material".to_string()),
        [0.0, 1.0, 0.0, 1.0], // Green color
    );
    
    let gold_material = builder.create_metallic_material(
        Some("Gold Material".to_string()),
        [1.0, 0.84, 0.0, 1.0],  // Gold color
        0.9, // High metallic factor
        0.1, // Low roughness factor (shiny)
    );
    
    // Create boxes with different materials and sizes
    let main_box_mesh = builder.create_box_with_material(1.0, Some(red_material));
    let small_box1_mesh = builder.create_box_with_material(0.2, Some(green_material));
    let small_box2_mesh = builder.create_box_with_material(0.2, Some(blue_material));
    let small_box3_mesh = builder.create_box_with_material(0.2, Some(gold_material));
    
    // Create solar system hierarchy
    
    // 1. Create the sun (main box)
    let sun_node = builder.add_node(
        Some("Sun".to_string()),
        Some(main_box_mesh),
        Some([0.0, 0.0, 0.0]), // At origin
        None,
        None,
    );
    
    // 2. Create planet1 that orbits the sun
    let planet1_node = builder.add_node(
        Some("Planet1".to_string()),
        Some(small_box1_mesh),
        Some([2.0, 0.0, 0.0]), // 2 units away from sun
        None,
        None,
    );
    
    // 3. Create planet2 that also orbits the sun
    let planet2_node = builder.add_node(
        Some("Planet2".to_string()),
        Some(small_box2_mesh),
        Some([0.0, 0.0, 3.0]), // 3 units away from sun in different direction
        None,
        None,
    );
    
    // 4. Create a moon that orbits planet1
    let moon_node = builder.add_node(
        Some("Moon".to_string()),
        Some(small_box3_mesh),
        Some([0.5, 0.0, 0.0]), // 0.5 units away from planet1
        None,
        None,
    );
    
    // Create the hierarchy relationships
    
    // Add planet1 and planet2 as children of the sun
    builder.add_child_to_node(sun_node, planet1_node)?;
    builder.add_child_to_node(sun_node, planet2_node)?;
    
    // Add moon as a child of planet1
    builder.add_child_to_node(planet1_node, moon_node)?;
    
    // Create a completely separate hierarchy to demonstrate multiple hierarchies
    
    // Create a parent with 3 children arranged vertically
    let parent_box_mesh = builder.create_box_with_material(0.5, Some(gold_material));
    let child1_mesh = builder.create_box_with_material(0.15, Some(red_material));
    let child2_mesh = builder.create_box_with_material(0.15, Some(green_material));
    let child3_mesh = builder.create_box_with_material(0.15, Some(blue_material));
    
    // Create the stack of boxes
    let stack_parent = builder.add_node(
        Some("StackParent".to_string()),
        Some(parent_box_mesh), 
        Some([-3.0, 0.0, 0.0]), // Positioned away from the solar system
        None,
        None,
    );
    
    let stack_child1 = builder.add_node(
        Some("StackChild1".to_string()),
        Some(child1_mesh),
        Some([0.0, 0.8, 0.0]), // Positioned above parent
        None,
        None,
    );
    
    let stack_child2 = builder.add_node(
        Some("StackChild2".to_string()),
        Some(child2_mesh),
        Some([0.0, 1.2, 0.0]), // Positioned above child1
        None,
        None,
    );
    
    let stack_child3 = builder.add_node(
        Some("StackChild3".to_string()),
        Some(child3_mesh),
        Some([0.0, 1.6, 0.0]), // Positioned above child2
        None,
        None,
    );
    
    // Create vertical stack hierarchy using the create_node_hierarchy method
    builder.add_child_to_node(stack_parent, stack_child1)?;
    builder.add_child_to_node(stack_child1, stack_child2)?;
    builder.add_child_to_node(stack_child2, stack_child3)?;
    
    // Create a third example showing the create_node_hierarchy method
    // This one will be a wheel with spokes
    let hub_mesh = builder.create_box_with_material(0.3, Some(gold_material));
    let spoke_mesh = builder.create_box_with_material(0.1, Some(red_material));
    
    // Create spokes
    let spoke1 = builder.add_node(
        Some("Spoke1".to_string()),
        Some(spoke_mesh),
        Some([0.7, 0.0, 0.0]), // Right
        None,
        None,
    );
    
    let spoke2 = builder.add_node(
        Some("Spoke2".to_string()),
        Some(spoke_mesh),
        Some([0.0, 0.7, 0.0]), // Up
        None,
        None,
    );
    
    let spoke3 = builder.add_node(
        Some("Spoke3".to_string()),
        Some(spoke_mesh),
        Some([-0.7, 0.0, 0.0]), // Left
        None,
        None,
    );
    
    let spoke4 = builder.add_node(
        Some("Spoke4".to_string()),
        Some(spoke_mesh),
        Some([0.0, -0.7, 0.0]), // Down
        None,
        None,
    );
    
    // Create the wheel as a hierarchy
    let wheel = builder.create_node_hierarchy(
        Some("Wheel".to_string()),
        Some([3.0, 0.0, 0.0]), // Positioned to the right of the solar system
        None,
        None,
        vec![spoke1, spoke2, spoke3, spoke4]
    )?;
    
    // Add the hub as a separate node (could also be part of the hierarchy creation)
    let hub = builder.add_node(
        Some("Hub".to_string()),
        Some(hub_mesh),
        Some([0.0, 0.0, 0.0]), // At center of wheel
        None,
        None,
    );
    
    builder.add_child_to_node(wheel, hub)?;
    
    // Create a viewpoint node
    let viewpoint = builder.add_node(
        Some("Viewpoint".to_string()),
        None, // No mesh for viewpoint
        Some([0.0, 3.0, 8.0]), // Position for a good view
        Some([-0.2, 0.0, 0.0, 0.98]), // Rotation quaternion looking down slightly
        None,
    );
    
    // Create a scene with all our hierarchies
    builder.add_scene(
        Some("Hierarchical Scene".to_string()),
        Some(vec![
            sun_node,     // Solar system hierarchy
            stack_parent, // Stack hierarchy
            wheel,        // Wheel hierarchy
            viewpoint,    // Viewpoint
        ]),
    );
    
    // Export the GLB file
    let output_path = "hierarchy_demo.glb";
    builder.export_glb(output_path)?;
    
    println!("Successfully exported a hierarchical scene: {}", output_path);
    println!("");
    println!("This demonstrates 3 different hierarchy examples:");
    println!("1. A 'solar system' with sun, planets and a moon");
    println!("2. A vertical 'stack' of boxes with parent-child relationships");
    println!("3. A 'wheel' with a hub and spokes created using create_node_hierarchy");
    println!("");
    println!("The object transformations are relative to their parents. Transforming");
    println!("a parent will automatically transform all of its children.");
    
    Ok(())
}
