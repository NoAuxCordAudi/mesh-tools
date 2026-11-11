# Skinning and Skeletal Animation

How to build skinned meshes and animate them with mesh-tools. Every code block in this file
is compiled and run by `cargo test --doc`, so it cannot silently go stale.

## Concepts

- **Joints** are ordinary nodes. A **skin** lists them in an order; a joint's position in that
  list is its **slot**.
- **`JOINTS_n` holds slots, not node indices.** If your joints are nodes 5, 6, 7, the mesh
  refers to them as 0, 1, 2.
- **Inverse bind matrices** (IBMs) map mesh space into each joint's space in the bind pose. They
  are stored column-major, 16 floats per joint, with the translation in elements 12..15.
- **Weights** say how much each of up to four joints (per set) moves a vertex. Across all sets, a
  vertex's weights sum to 1.
- A skinned node's own transform is ignored by glTF viewers. The joints drive the deformation,
  so give the skinned node no transform.

## Building a skinned mesh

```rust
use mesh_tools::{GltfBuilder, InterpolationType};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut builder = GltfBuilder::new();

// 1. A joint chain. Offsets are relative to the parent joint.
let names = vec!["Shoulder".to_string(), "Elbow".to_string()];
let joints = builder.create_bone_chain(&names, &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]])?;

// 2. The skin. Inverse bind matrices are computed from the joints' current transforms,
//    so build the hierarchy first.
let skin = builder.add_skin_from_joints(Some("ArmSkin".into()), joints.clone(), Some(joints[0]))?;

// 3. A skinned mesh: one triangle. Slots 0/1 refer to Shoulder/Elbow, not to node indices.
let positions = [0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 2.0, 1.0, 0.0];
let mesh = builder.create_skinned_mesh(
    None,
    &positions,
    &[0, 1, 2],
    None, // normals
    None, // texcoords
    &[[0, 0, 0, 0], [1, 0, 0, 0], [0, 1, 0, 0]],
    &[[1.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0]],
    None, // material
)?;

// 4. A node that renders the mesh with the skin, and a scene containing it and the skeleton.
let node = builder.add_skinned_node(None, mesh, skin, None, None, None)?;
builder.add_scene(None, Some(vec![node, joints[0]]));

// 5. Bend the elbow 45 degrees about Z over one second.
let animation = builder.add_animation(Some("Bend".into()));
builder.add_rotation_track(
    animation,
    joints[1],
    &[0.0, 1.0],
    &[[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.38268343, 0.9238795]],
    InterpolationType::Linear,
)?;

// Export validates first: a dangling index or out-of-range joint is an error, not a bad file.
let bytes = builder.to_glb_bytes()?;
assert!(bytes.starts_with(b"glTF"));
# Ok(())
# }
```

A complete, runnable version with a cylinder mesh and two animations is in
`examples/skeletal_animation_demo.rs`:

```bash
cargo run --example skeletal_animation_demo [output.glb]
```

## Weights

The mesh functions **reject** bad weights rather than silently changing your data. Each
vertex's weights, summed over all sets, must be finite, non-negative and add up to 1 within
`1e-5`. A joint with a non-zero weight may not appear twice on one vertex, and its slot must be below
the skin's joint count. That count is checked by `create_skinned_primitive` and by `MeshBuilder`
with `with_joint_count`; `create_skinned_mesh` only checks that slots fit in `u16`. In every
case, export reports any slot beyond the skin's joints as `SKIN_JOINT_OUT_OF_RANGE`. Unused
slots use any joint value with weight `0.0`.

Two explicit helpers fix data up when you want that:

```rust
use mesh_tools::{limit_influences, normalize_weights};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
// Reduce an arbitrary number of (slot, weight) influences per vertex to the strongest four,
// renormalized. This is lossy by design.
let (joints, mut weights) = limit_influences(&[
    vec![(3, 0.4), (1, 0.2), (9, 0.2), (7, 0.1), (5, 0.1)],
])?;
assert_eq!(joints[0], [3, 1, 9, 5]);

// Rescale each vertex's four weights to sum to exactly 1.
normalize_weights(&mut weights)?;
# Ok(())
# }
```

For more than four influences, or `u8` joints, use `create_skinned_primitive` with one or more
`SkinSet`s (`JOINTS_0`/`WEIGHTS_0`, `JOINTS_1`/`WEIGHTS_1`, ...). It also validates everything
before writing anything, so a failed call leaves the document unchanged. `MeshBuilder` can
build the same data with `with_joints`, `with_weights`, `with_joint_count` and `build_into`.

## Animation

Tracks animate a node's translation, rotation or scale:

| Function | Keyframe values |
|---|---|
| `add_translation_track`, `add_scale_track` | `&[[f32; 3]]` |
| `add_rotation_track` | `&[[f32; 4]]` quaternions (normalized for you) |
| `add_cubic_translation_track`, `add_cubic_scale_track`, `add_cubic_rotation_track` | `&[CubicKeyframe<..>]` |

Rules the library enforces (each is an error, never a panic, and a failed call writes nothing):

- Times are non-empty, finite, `>= 0` and strictly increasing.
- Within one animation each `(node, path)` pair has at most one channel.
- A node with a `matrix` cannot be animated.
- `CUBICSPLINE` needs `CubicKeyframe { in_tangent, value, out_tangent }` and at least two
  keyframes. Passing plain values with `InterpolationType::CubicSpline` is an error, because
  tangents are never invented for you.
- Morph target `weights` tracks (`add_weights_track`, `add_cubic_weights_track`) are covered in [MORPH_TARGETS.md](MORPH_TARGETS.md).

```rust
use mesh_tools::{CubicKeyframe, GltfBuilder};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut builder = GltfBuilder::new();
let node = builder.add_node(None, None, None, None, None);
let animation = builder.add_animation(None);

let key = |x: f32| CubicKeyframe {
    in_tangent: [0.0; 3],
    value: [x, 0.0, 0.0],
    out_tangent: [0.0; 3],
};
builder.add_cubic_translation_track(animation, node, &[0.0, 1.0], &[key(0.0), key(1.0)])?;
# Ok(())
# }
```

## Validation

`GltfBuilder::validate()` returns a `ValidationReport { errors, warnings }`. `export_glb` and
`to_glb_bytes` call it and fail with `GltfError::InvalidData` listing every error; the
`*_unchecked` variants skip it. Every message starts with a stable code. The skinning-related
ones include:

- `NODE_SKIN_WITHOUT_MESH`, `NODE_SKIN_INVALID`
- `SKIN_MESH_MISSING_ATTRIBUTES` (skinned node's mesh lacks `JOINTS_0`/`WEIGHTS_0`)
- `SKIN_JOINT_OUT_OF_RANGE` (read back from the buffer: catches node indices used as slots)
- `SKIN_NO_JOINTS`, `SKIN_JOINT_INVALID`, `SKIN_JOINT_DUPLICATE`
- `SKIN_NO_COMMON_ROOT`, `SKIN_SKELETON_NOT_ANCESTOR`, `SKIN_SKELETON_INVALID`
- `SKIN_IBM_INVALID`, `SKIN_IBM_TYPE`, `SKIN_IBM_COUNT`
- `ANIMATION_EMPTY`, `ANIMATION_CHANNEL_NODE_MATRIX`, `ANIMATION_INPUT_NO_BOUNDS`
- `ANIMATION_INPUT_TYPE`, `ANIMATION_INTERPOLATION_INVALID`, `ANIMATION_OUTPUT_TYPE`, `ANIMATION_OUTPUT_COUNT`

A skinned node with a non-identity transform is reported as the warning
`SKINNED_NODE_LOCAL_TRANSFORM`; warnings never block export.

## Limits

- Inverse bind matrices assume each joint's world transform is affine (bottom row `0 0 0 1`),
  built from a node's `matrix` or its translation, rotation and scale. A non-affine `matrix`
  gives a meaningless result. Singular transforms (for example a zero scale) are an error.
- Normalized `u8`/`u16` weights are not supported yet.
- The validator checks structure and ranges. It is not a replacement for the Khronos glTF
  Validator, which has not been run against this library's output.
