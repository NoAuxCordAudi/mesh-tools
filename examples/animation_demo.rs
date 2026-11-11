use std::f32::consts::PI;
use std::path::Path;

use mesh_tools::{GltfBuilder, InterpolationType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();

    // Create a plain material for our objects
    let red_material = builder.add_material(
        Some("RedMaterial".to_string()),
        Some([0.8, 0.2, 0.2, 1.0]), // Red color
        Some(0.0), // Non-metallic
        Some(0.5), // Medium roughness
        None       // Not necessarily double-sided
    );
    
    let blue_material = builder.add_material(
        Some("BlueMaterial".to_string()),
        Some([0.2, 0.2, 0.8, 1.0]), // Blue color
        Some(0.0), // Non-metallic
        Some(0.5), // Medium roughness
        None       // Not necessarily double-sided
    );
    
    // Create a static cube at the origin as a reference point
    let static_cube = builder.create_box_with_material(
        1.0, Some(blue_material)
    );
    
    let static_node = builder.add_node(
        Some("StaticCube".to_string()),
        Some(static_cube),
        Some([0.0, 0.0, 0.0]), // Position at origin
        None,                   // No rotation
        None                    // Default scale
    );
    
    // Create a sphere that will be animated
    let sphere = builder.create_sphere(
        0.5, 32, 16, Some(red_material)
    );
    
    let animated_sphere_node = builder.add_node(
        Some("AnimatedSphere".to_string()),
        Some(sphere),
        Some([0.0, 0.0, 0.0]), // Initial position (will be animated)
        None,                  // No rotation (will be animated)
        None                   // Default scale (will be animated)
    );
    
    // Create a scene and add both nodes to it
    // (the first scene added becomes the default scene)
    builder.add_scene(
        Some("AnimatedScene".to_string()),
        Some(vec![static_node, animated_sphere_node])
    );

    // Create an animation
    let orbit_animation = builder.add_animation(Some("OrbitAnimation".to_string()));

    // Define our animation timeframe
    // We'll create a 4-second animation with 60 keyframes (15 per second)
    let frames = 60;
    let duration = 4.0; // seconds
    
    // Calculate keyframe timestamps and values
    let mut timestamps = Vec::with_capacity(frames);
    let mut translations = Vec::with_capacity(frames);
    let mut rotations = Vec::with_capacity(frames); 
    let mut scales = Vec::with_capacity(frames);
    
    for i in 0..frames {
        // Calculate the current time (0.0 to duration)
        let time = (i as f32) * duration / ((frames - 1) as f32);
        timestamps.push(time);
        
        // Calculate the position for this keyframe - orbit around the origin
        let angle = (i as f32) * 2.0 * PI / ((frames - 1) as f32);
        let orbit_radius = 2.0;
        let x = orbit_radius * angle.cos();
        let z = orbit_radius * angle.sin();
        let y = 0.5 + (angle * 2.0).sin() * 0.5; // Add some up/down motion
        
        translations.push([x, y, z]);
        
        // For rotation, rotate the sphere as it moves
        let axis_angle = angle * 2.0; // Rotate twice per orbit
        // Store as quaternion [x, y, z, w]
        let w = (axis_angle * 0.5).cos();
        let y_component = (axis_angle * 0.5).sin();
        rotations.push([0.0, y_component, 0.0, w]); // Rotate around Y axis
        
        // For scale, make the sphere pulse by varying its scale
        let scale_factor = 0.8 + 0.4 * (time * PI).sin().abs();
        scales.push([scale_factor, scale_factor, scale_factor]);
    }
    
    // Add translation, rotation, and scale animations to the animated sphere
    builder.add_translation_track(
        orbit_animation,
        animated_sphere_node,
        &timestamps,
        &translations,
        InterpolationType::Linear
    )?;
    
    builder.add_rotation_track(
        orbit_animation,
        animated_sphere_node,
        &timestamps,
        &rotations,
        InterpolationType::Linear
    )?;
    
    builder.add_scale_track(
        orbit_animation,
        animated_sphere_node,
        &timestamps,
        &scales,
        InterpolationType::Linear
    )?;
    
    // Export the animated scene as a binary glTF file
    // Optional first argument: output path (default: animation_demo.glb in the current directory)
    let output = std::env::args().nth(1).unwrap_or_else(|| "animation_demo.glb".to_string());
    let path = Path::new(&output);
    builder.export_glb(&path.to_str().unwrap())?;
    
    println!("Successfully exported GLB file with animations: {}", path.display());
    println!("This example demonstrates a simple animation with:");
    println!("  - A rotating, moving sphere that orbits around a static cube");
    println!("  - Translation animation (orbital path with vertical motion)");
    println!("  - Rotation animation (sphere spinning on Y axis)");
    println!("  - Scale animation (sphere pulsing larger and smaller)");
    println!("  - Linear interpolation for smooth motion between keyframes");
    println!("\nThe animation lasts 4 seconds with 60 keyframes.");
    println!("\nYou can view this animation in any glTF viewer that supports animations,");
    println!("such as the official Khronos glTF Sample Viewer:");
    println!("https://github.khronos.org/glTF-Sample-Viewer-Release/");
    
    Ok(())
}
