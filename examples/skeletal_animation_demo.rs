//! # Skeletal Animation Demo
//!
//! This example demonstrates how to create skeletal animation in glTF using the mesh-tools library.
//! It creates a simple animated character with:
//! - A hierarchical skeleton (bone chain)
//! - A skinned mesh that deforms with the skeleton
//! - Inverse bind matrices for proper joint-to-mesh binding
//! - Joint weights that control how much each bone affects each vertex

use mesh_tools::GltfBuilder;
use std::f32::consts::PI;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = GltfBuilder::new();

    // Create a simple material for our mesh
    let material_index = builder.create_basic_material(
        Some("SkinnedMaterial".to_string()),
        [0.8, 0.6, 0.4, 1.0], // Base color - skin tone
    );

    // Create a bone hierarchy - a simple arm with shoulder, elbow, and wrist
    let joint_names = vec![
        "Shoulder".to_string(),
        "Elbow".to_string(),
        "Wrist".to_string(),
    ];

    let joint_positions = vec![
        [0.0, 0.0, 0.0],    // Shoulder at origin
        [2.0, 0.0, 0.0],    // Elbow 2 units along X
        [2.0, 0.0, 0.0],    // Wrist 2 more units along X
    ];

    // Create the bone chain
    let joint_indices = builder.create_bone_chain(&joint_names, &joint_positions)?;
    println!("Created joint chain with indices: {:?}", joint_indices);

    // Create a simple cylindrical mesh representing an arm
    // The mesh will be deformed by the joints
    let (positions, indices, normals, joints, weights) = create_arm_mesh(&joint_indices);

    // Create the skinned mesh
    let mesh_index = builder.create_skinned_mesh(
        Some("ArmMesh".to_string()),
        &positions,
        &indices,
        Some(&normals),
        None, // No texture coordinates for this demo
        &joints,
        &weights,
        Some(material_index),
    )?;

    // Create inverse bind matrices for the joints
    // These matrices transform vertices from model space to joint space
    let inverse_bind_matrices = create_inverse_bind_matrices(&joint_positions);
    let inverse_bind_matrices_accessor = builder.create_inverse_bind_matrices_accessor(&inverse_bind_matrices);

    // Create the skin
    let skin_index = builder.add_skin(
        Some("ArmSkin".to_string()),
        joint_indices.clone(),
        Some(inverse_bind_matrices_accessor),
        Some(joint_indices[0]), // Shoulder is the skeleton root
    );

    // Create a node that uses the skinned mesh
    let skinned_node_index = builder.add_skinned_node(
        Some("SkinnedArm".to_string()),
        mesh_index,
        skin_index,
        Some([0.0, 0.0, 0.0]), // Position
        None,                   // No rotation
        None,                   // No scale
    );

    // Create a scene containing our skinned object
    let scene_index = builder.add_scene(
        Some("SkeletalAnimationScene".to_string()),
        Some(vec![skinned_node_index, joint_indices[0]]), // Include both the skinned mesh and the skeleton root
    );

    // Set the default scene
    builder.gltf.scene = Some(scene_index);

    // Export the GLB file
    builder.export_glb("skeletal_animation_demo.glb")?;
    println!("Exported skeletal_animation_demo.glb");

    Ok(())
}

/// Creates a simple arm mesh with joint weights
/// Returns (positions, indices, normals, joints, weights)
fn create_arm_mesh(joint_indices: &[usize]) -> (Vec<f32>, Vec<u16>, Vec<f32>, Vec<[u16; 4]>, Vec<[f32; 4]>) {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    let mut normals = Vec::new();
    let mut joints = Vec::new();
    let mut weights = Vec::new();

    // Create a simple cylindrical arm mesh with 3 segments
    let segments = 8; // Number of segments around the cylinder
    let length_segments = 3; // Number of segments along the length
    let radius = 0.3;
    let total_length = 4.0; // Total arm length

    // Generate vertices
    for i in 0..=length_segments {
        let z = (i as f32 / length_segments as f32) * total_length;
        
        for j in 0..segments {
            let angle = (j as f32 / segments as f32) * 2.0 * PI;
            let x = angle.cos() * radius;
            let y = angle.sin() * radius;

            positions.extend_from_slice(&[x, y, z]);
            normals.extend_from_slice(&[angle.cos(), angle.sin(), 0.0]);

            // Assign joint weights based on position along the arm
            let joint_weight = if i == 0 {
                // First segment - fully influenced by shoulder
                [joint_indices[0] as u16, 0, 0, 0]
            } else if i == 1 {
                // Second segment - blend between shoulder and elbow
                [joint_indices[0] as u16, joint_indices[1] as u16, 0, 0]
            } else if i == 2 {
                // Third segment - blend between elbow and wrist
                [joint_indices[1] as u16, joint_indices[2] as u16, 0, 0]
            } else {
                // Last segment - fully influenced by wrist
                [joint_indices[2] as u16, 0, 0, 0]
            };

            let weight_values = if i == 0 {
                [1.0, 0.0, 0.0, 0.0] // Fully shoulder
            } else if i == 1 {
                [0.7, 0.3, 0.0, 0.0] // Mostly shoulder, some elbow
            } else if i == 2 {
                [0.0, 0.7, 0.3, 0.0] // Mostly elbow, some wrist
            } else {
                [0.0, 0.0, 1.0, 0.0] // Fully wrist
            };

            joints.push(joint_weight);
            weights.push(weight_values);
        }
    }

    // Generate indices for triangles
    for i in 0..length_segments {
        for j in 0..segments {
            let current = i * segments + j;
            let next = i * segments + ((j + 1) % segments);
            let current_next_row = (i + 1) * segments + j;
            let next_next_row = (i + 1) * segments + ((j + 1) % segments);

            // First triangle
            indices.extend_from_slice(&[
                current as u16,
                next as u16,
                current_next_row as u16,
            ]);

            // Second triangle
            indices.extend_from_slice(&[
                next as u16,
                next_next_row as u16,
                current_next_row as u16,
            ]);
        }
    }

    (positions, indices, normals, joints, weights)
}

/// Creates inverse bind matrices for the joints
fn create_inverse_bind_matrices(joint_positions: &[[f32; 3]]) -> Vec<mint::ColumnMatrix4<f32>> {
    let mut matrices = Vec::new();
    let mut cumulative_translation = [0.0, 0.0, 0.0];

    for position in joint_positions {
        // Update cumulative translation
        cumulative_translation[0] += position[0];
        cumulative_translation[1] += position[1];
        cumulative_translation[2] += position[2];

        // Create inverse bind matrix (inverse of the joint's world transform)
        // This is a simple translation matrix inverted
        #[rustfmt::skip]
        let inverse_bind_matrix = mint::ColumnMatrix4 {
            x: mint::Vector4 { x: 1.0, y: 0.0, z: 0.0, w: -cumulative_translation[0] },
            y: mint::Vector4 { x: 0.0, y: 1.0, z: 0.0, w: -cumulative_translation[1] },
            z: mint::Vector4 { x: 0.0, y: 0.0, z: 1.0, w: -cumulative_translation[2] },
            w: mint::Vector4 { x: 0.0, y: 0.0, z: 0.0, w: 1.0 },
        };

        matrices.push(inverse_bind_matrix);
    }

    matrices
}
