//! # Skeletal Animation Demo
//!
//! A three-joint arm (shoulder, elbow, wrist) whose skinned mesh bends as the joints animate.
//!
//! What it shows:
//! - a joint hierarchy built with `create_bone_chain`
//! - a skinned mesh whose `JOINTS_0` values are skin *slots* (positions in the skin's joint
//!   list), not node indices, with weights that sum to 1 on every vertex
//! - a skin whose inverse bind matrices are computed by the library from the bind pose
//!   (`add_skin_from_joints`)
//! - a skinned node with no transform of its own (viewers ignore it anyway)
//! - a `LINEAR` elbow animation and a `CUBICSPLINE` wrist animation
//!
//! The arm lies along +X: shoulder at x=0, elbow at x=2, wrist at x=4, hand tip at x=6.
//! The joints rotate about Z, so the arm bends in the XY plane.
//!
//! Usage: `cargo run --example skeletal_animation_demo [output.glb]`

use mesh_tools::{normalize_weights, CubicKeyframe, GltfBuilder, InterpolationType};
use std::f32::consts::PI;

/// Output path used when no argument is given.
const DEFAULT_OUTPUT: &str = "skeletal_animation_demo.glb";

/// Positions along the arm (x) where vertex rings are placed. Extra rings around the
/// elbow (x=2) and wrist (x=4) give the bend something to deform.
const RING_X: [f32; 11] = [0.0, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 6.0];
const SEGMENTS: usize = 8;
const RADIUS: f32 = 0.3;

/// Skin slots: positions in the skin's `joints` list. They are *not* node indices (the
/// joints are nodes 1, 2, 3 because the ground slab is node 0).
const SHOULDER: u16 = 0;
const ELBOW: u16 = 1;
const WRIST: u16 = 2;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_OUTPUT.to_string());
    let mut builder = GltfBuilder::new();

    let material = builder.create_basic_material(Some("ArmMaterial".to_string()), [0.8, 0.6, 0.4, 1.0]);

    // A thin ground slab for visual reference. It is created first on purpose: the joint
    // nodes below become nodes 1, 2, 3, so node indices differ from skin slots 0, 1, 2.
    let ground_mesh = builder.create_box(1.0);
    let ground = builder.add_node(
        Some("Ground".to_string()),
        Some(ground_mesh),
        Some([3.0, -1.0, 0.0]),
        None,
        Some([8.0, 0.05, 3.0]),
    );

    // Joint hierarchy. Positions are relative to the parent joint.
    let joint_names = vec!["Shoulder".to_string(), "Elbow".to_string(), "Wrist".to_string()];
    let joint_offsets = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
    let joints = builder.create_bone_chain(&joint_names, &joint_offsets)?;

    // Skin: inverse bind matrices come from the joints' current (bind) transforms.
    let skin = builder.add_skin_from_joints(Some("ArmSkin".to_string()), joints.clone(), Some(joints[0]))?;

    // Skinned mesh. Joint indices in the mesh are slots into `joints` above.
    let arm = build_arm_mesh()?;
    let mesh = builder.create_skinned_mesh(
        Some("ArmMesh".to_string()),
        &arm.positions,
        &arm.indices,
        Some(&arm.normals),
        None,
        &arm.joints,
        &arm.weights,
        Some(material),
    )?;

    // The skinned node has no transform: the joints drive the deformation.
    let skinned_node = builder.add_skinned_node(Some("SkinnedArm".to_string()), mesh, skin, None, None, None)?;

    // The first scene becomes the default scene. Include the skeleton root so the joints are in it.
    builder.add_scene(Some("ArmScene".to_string()), Some(vec![ground, skinned_node, joints[0]]));

    // Elbow bend: LINEAR, 0 -> 70 degrees -> 0 over two seconds.
    let bend = builder.add_animation(Some("ElbowBend".to_string()));
    builder.add_rotation_track(
        bend,
        joints[1],
        &[0.0, 1.0, 2.0],
        &[z_rotation(0.0), z_rotation(70.0), z_rotation(0.0)],
        InterpolationType::Linear,
    )?;

    // Wrist wave: CUBICSPLINE with explicit tangents (zero tangents ease in and out).
    let wave = builder.add_animation(Some("WristWave".to_string()));
    let key = |degrees: f32| CubicKeyframe {
        in_tangent: [0.0; 4],
        value: z_rotation(degrees),
        out_tangent: [0.0; 4],
    };
    builder.add_cubic_rotation_track(wave, joints[2], &[0.0, 1.0, 2.0], &[key(0.0), key(-45.0), key(0.0)])?;

    // Export validates the document first and fails with a list of problems if it is invalid.
    builder.export_glb(&output)?;
    println!("Exported {output}");
    println!("  joints (node indices): {joints:?}");
    println!("  animations: ElbowBend (LINEAR), WristWave (CUBICSPLINE)");
    Ok(())
}

/// Quaternion `[x, y, z, w]` for a rotation of `degrees` about Z.
fn z_rotation(degrees: f32) -> [f32; 4] {
    let half = degrees.to_radians() * 0.5;
    [0.0, 0.0, half.sin(), half.cos()]
}

struct ArmMesh {
    positions: Vec<f32>,
    indices: Vec<u16>,
    normals: Vec<f32>,
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
}

/// Skin slots and weights for a ring at position `x` along the arm.
///
/// Rigid near the shoulder, blends shoulder -> elbow over 1.5..2.5, rigid on the
/// forearm, blends elbow -> wrist over 3.5..4.5, rigid on the hand.
fn influences(x: f32) -> ([u16; 4], [f32; 4]) {
    if x <= 1.5 {
        ([SHOULDER, 0, 0, 0], [1.0, 0.0, 0.0, 0.0])
    } else if x < 2.5 {
        let t = x - 1.5;
        ([SHOULDER, ELBOW, 0, 0], [1.0 - t, t, 0.0, 0.0])
    } else if x <= 3.5 {
        ([ELBOW, 0, 0, 0], [1.0, 0.0, 0.0, 0.0])
    } else if x < 4.5 {
        let t = x - 3.5;
        ([ELBOW, WRIST, 0, 0], [1.0 - t, t, 0.0, 0.0])
    } else {
        ([WRIST, 0, 0, 0], [1.0, 0.0, 0.0, 0.0])
    }
}

/// A cylinder along +X with outward-facing (counter-clockwise) triangles.
fn build_arm_mesh() -> Result<ArmMesh, Box<dyn std::error::Error>> {
    let mut mesh = ArmMesh {
        positions: Vec::new(),
        indices: Vec::new(),
        normals: Vec::new(),
        joints: Vec::new(),
        weights: Vec::new(),
    };

    for &x in &RING_X {
        let (slots, weights) = influences(x);
        for j in 0..SEGMENTS {
            let angle = j as f32 / SEGMENTS as f32 * 2.0 * PI;
            let (sin, cos) = angle.sin_cos();
            mesh.positions.extend_from_slice(&[x, cos * RADIUS, sin * RADIUS]);
            mesh.normals.extend_from_slice(&[0.0, cos, sin]);
            mesh.joints.push(slots);
            mesh.weights.push(weights);
        }
    }
    // Make every vertex's weights sum to exactly 1 (the library rejects sums that are off).
    normalize_weights(&mut mesh.weights)?;

    for ring in 0..RING_X.len() - 1 {
        for j in 0..SEGMENTS {
            let next = (j + 1) % SEGMENTS;
            let a = (ring * SEGMENTS + j) as u16;
            let b = (ring * SEGMENTS + next) as u16;
            let c = ((ring + 1) * SEGMENTS + j) as u16;
            let d = ((ring + 1) * SEGMENTS + next) as u16;
            // Viewed from outside, the angle increases counter-clockwise around +X.
            mesh.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    Ok(mesh)
}
