# Skeletal Animation (Skinning) Support

This document describes how to use the skeletal animation (skinning) features in the mesh-tools library.

## Overview

The mesh-tools library now supports glTF skinning for skeletal animation. This includes:

- **Skins**: Collections of joints that define how a mesh deforms
- **Joints**: Individual bones in a skeleton hierarchy  
- **Inverse Bind Matrices**: Transform vertices from model space to joint space
- **Joint Weights**: Control how much each bone affects each vertex

## Key Components

### Data Models

- `Skin`: Represents a glTF skin with joints, inverse bind matrices, and skeleton root
- `Node.skin`: Optional reference to a skin for skinned mesh nodes

### Builder Methods

The `GltfBuilder` provides several methods for creating skinned content:

#### Core Skin Methods
- `add_skin()`: Create a skin with joints and inverse bind matrices
- `add_joint_node()`: Create individual joint/bone nodes
- `create_inverse_bind_matrices_accessor()`: Create accessors for inverse bind matrix data
- `add_skinned_node()`: Create nodes that reference both meshes and skins

#### Utility Methods
- `create_bone_chain()`: Create a linear chain of connected joints
- `create_skinned_mesh()`: Create meshes with `JOINTS_0` and `WEIGHTS_0` attributes

## Basic Usage

```rust
use mesh_tools::GltfBuilder;

let mut builder = GltfBuilder::new();

// 1. Create a bone hierarchy
let joint_names = vec!["Shoulder".to_string(), "Elbow".to_string(), "Wrist".to_string()];
let joint_positions = vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
let joint_indices = builder.create_bone_chain(&joint_names, &joint_positions)?;

// 2. Create a skinned mesh with joint weights
let (positions, indices, normals, joints, weights) = create_mesh_with_weights();
let mesh_index = builder.create_skinned_mesh(
    Some("SkinnedMesh".to_string()),
    &positions,
    &indices,
    Some(&normals),
    None, // texture coordinates
    &joints,
    &weights,
    Some(material_index),
)?;

// 3. Create inverse bind matrices
let inverse_bind_matrices = create_inverse_bind_matrices(&joint_positions);
let ibm_accessor = builder.create_inverse_bind_matrices_accessor(&inverse_bind_matrices);

// 4. Create the skin
let skin_index = builder.add_skin(
    Some("MySkin".to_string()),
    joint_indices.clone(),
    Some(ibm_accessor),
    Some(joint_indices[0]), // skeleton root
);

// 5. Create a node that uses the skinned mesh
let skinned_node = builder.add_skinned_node(
    Some("SkinnedObject".to_string()),
    mesh_index,
    skin_index,
    Some([0.0, 0.0, 0.0]), // position
    None, // rotation
    None, // scale
);
```

## Joint Weights Format

Joint weights follow the glTF 2.0 specification:
- **JOINTS_0**: Array of `[u16; 4]` - indices of joints affecting each vertex
- **WEIGHTS_0**: Array of `[f32; 4]` - weights for each joint (should sum to 1.0)

Each vertex can be influenced by up to 4 joints. Unused joint slots should use index 0 and weight 0.0.

## Example

See `examples/skeletal_animation_demo.rs` for a complete working example that creates:
- A 3-joint arm skeleton (shoulder, elbow, wrist)
- A cylindrical mesh with proper joint weights
- Inverse bind matrices for proper binding
- Complete skinned character export

Run with:
```bash
cargo run --example skeletal_animation_demo
```

## glTF Specification Compliance

This implementation follows the [glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#skins) for skinning:

- Skins contain arrays of joint node indices
- Inverse bind matrices are stored as 4x4 matrices in column-major order
- Joint indices use `UNSIGNED_SHORT` component type
- Joint weights use `FLOAT` component type
- Meshes reference skins through the `skin` property on nodes
