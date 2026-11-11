# Morph Targets (Blend Shapes)

How to build meshes with morph targets and animate their weights with mesh-tools. Every Rust
code block in this file is compiled and run by `cargo test --doc`, so it cannot silently go stale.

## Concepts

- A **morph target** stores per-vertex **deltas** (not absolute positions) relative to the
  mesh's base attributes.
- A renderer computes `base + sum(weight_i * delta_i)` for every vertex. Weights are **not
  clamped**: negative values and values above 1 are legal and extrapolate.
- **Weights** come from, in order of priority: the node's `weights`, the mesh's default
  `weights`, then zeros.
- Normals: normal deltas are blended the same way. The library stores them as given and never
  renormalizes; renderers renormalize.
- Morphing happens **before** skinning: `skin(base + morph)`.
- All primitives of a mesh must have the same number of targets, in the same order.

## Building a morphed mesh

`MeshBuilder` is the shortest path. Deltas are flat `x, y, z` triples, one per vertex, in the
base vertex order.

```rust
use mesh_tools::mesh::MeshBuilder;
use mesh_tools::{GltfBuilder, InterpolationType, MorphTarget};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut builder = GltfBuilder::new();

let positions = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
let normals = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];

// Target 0 lifts every vertex by 1 along z; target 1 moves them sideways.
let lift = MorphTarget::new()
    .with_positions(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0])
    .with_normals(vec![0.0, 0.5, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0]);
let slide = MorphTarget::new().with_positions(vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

let mesh = MeshBuilder::new(Some("Triangle".into()))
    .with_positions(positions)
    .with_normals(normals) // required if any target has normal deltas
    .with_indices(vec![0, 1, 2])
    .with_morph_targets(vec![lift, slide])
    .with_default_weights(vec![0.5, 0.0]) // optional; one per target
    .with_target_names(vec!["Lift".into(), "Slide".into()]) // optional; one per target
    .build_into(&mut builder)?;

// A node can override the mesh's default weights.
let node = builder.add_node(None, Some(mesh), None, None, None);
builder.set_node_weights(node, vec![1.0, 0.25])?;
builder.add_scene(None, Some(vec![node]));

// Animate the weights: one row per keyframe, one weight per target.
let animation = builder.add_animation(Some("Wobble".into()));
builder.add_weights_track(
    animation,
    node,
    &[0.0, 1.0, 2.0],
    &[vec![0.0, 0.0], vec![1.0, 0.0], vec![0.0, 1.0]],
    InterpolationType::Linear,
)?;

let bytes = builder.to_glb_bytes()?; // validates first
assert!(bytes.starts_with(b"glTF"));
# Ok(())
# }
```

Target names are written to `mesh.extras.targetNames`, the convention Blender, three.js and
other tools read. Default weights and names are optional: when omitted, nothing is written.

## Morphing a skinned mesh

Attach targets to a primitive *before* adding it to a mesh. This works with primitives from
`create_skinned_primitive`, so one primitive can be both skinned and morphed.

```rust
use mesh_tools::{GltfBuilder, JointIndices, MorphTarget, SkinSet};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut builder = GltfBuilder::new();
let joint = builder.add_joint_node(Some("Root".into()), None, None, None);
let skin = builder.add_skin_from_joints(None, vec![joint], Some(joint))?;

let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
let influences = SkinSet::new(JointIndices::U8(vec![[0; 4]; 3]), vec![[1.0, 0.0, 0.0, 0.0]; 3]);
let mut primitive =
    builder.create_skinned_primitive(&positions, Some(&[0, 1, 2]), None, None, &[influences], 1, None)?;

// Atomic: if any target is invalid, nothing is written and `primitive` is unchanged.
builder.attach_morph_targets(&mut primitive, &[MorphTarget::new().with_positions(vec![0.0, 0.0, 0.5].repeat(3))])?;

let mesh = builder.add_morphable_mesh(None, vec![primitive], Some(vec![0.0]), None)?;
let node = builder.add_skinned_node(None, mesh, skin, None, None, None)?;
builder.add_scene(None, Some(vec![node, joint]));
assert!(builder.validate().is_valid());
# Ok(())
# }
```

## Animating weights

| Function | Keyframe values |
|---|---|
| `add_weights_track` | `&[Vec<f32>]`, one row per keyframe (`LINEAR` or `STEP`) |
| `add_cubic_weights_track` | `&[CubicKeyframe<Vec<f32>>]` (`CUBICSPLINE`, at least two keyframes) |

The number of targets `M` always comes from the animated node's mesh, never from the caller. A
row with the wrong width is an error naming the keyframe. The output holds `N * M` values for
`LINEAR`/`STEP` and `3 * N * M` for `CUBICSPLINE`, laid out per keyframe as
`[in-tangents(M), values(M), out-tangents(M)]`.

```rust
use mesh_tools::mesh::MeshBuilder;
use mesh_tools::{CubicKeyframe, GltfBuilder, MorphTarget};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut builder = GltfBuilder::new();
let mesh = MeshBuilder::new(None)
    .with_positions(vec![0.0; 9])
    .with_morph_targets(vec![MorphTarget::new().with_positions(vec![0.1; 9])])
    .build_into(&mut builder)?;
let node = builder.add_node(None, Some(mesh), None, None, None);
let animation = builder.add_animation(None);

let key = |w: f32| CubicKeyframe { in_tangent: vec![0.0], value: vec![w], out_tangent: vec![0.0] };
builder.add_cubic_weights_track(animation, node, &[0.0, 1.0], &[key(0.0), key(1.0)])?;
# Ok(())
# }
```

Rules (each is an error, never a panic, and a failed call writes nothing):

- The node must have a mesh with morph targets.
- Times are non-empty, finite, `>= 0` and strictly increasing; weights must be finite.
- A `(node, weights)` pair has at most one channel per animation.
- Passing plain values with `InterpolationType::CubicSpline` is an error pointing to
  `add_cubic_weights_track`; tangents are never invented.
- Unlike translation, rotation and scale, weights can be animated on a node that has a `matrix`.

## What is checked when attaching targets

`attach_morph_targets` (and `MeshBuilder::build_into`, which validates *before* writing the base
primitive) rejects, with a message naming the target:

- no targets, a target with no deltas, or a primitive that already has targets
- a primitive with no valid `POSITION`
- delta arrays that are not exactly `3 * vertex_count` floats, or contain non-finite values
- normal deltas on a primitive with no base `NORMAL`
- tangent deltas on a primitive with no base `TANGENT`
- sparse deltas with no entries, indices that are not strictly increasing or reach the vertex
  count, or not exactly three values per index

`add_morphable_mesh` additionally requires every primitive to have the same non-zero number of
targets, and `default_weights` and `target_names` (when given) to match that count.
`set_node_weights` requires a node whose mesh has morph targets.

Position delta accessors carry `min`/`max` computed from the deltas (for sparse ones, from the
dense result including the implicit zeros).

## Tangent deltas

A target may carry tangent deltas if the base primitive has `TANGENT` (4 floats per vertex, `w`
is the handedness). Deltas are `x, y, z` only (VEC3): `w` is never morphed.

```rust
use mesh_tools::mesh::MeshBuilder;
use mesh_tools::{GltfBuilder, MorphTarget};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut builder = GltfBuilder::new();
let build = |tangents: Option<Vec<f32>>| {
    let mut m = MeshBuilder::new(None)
        .with_positions(vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0])
        .with_normals(vec![0.0, 0.0, 1.0].repeat(3))
        .with_indices(vec![0, 1, 2])
        .with_morph_targets(vec![MorphTarget::new().with_tangents(vec![0.0, 0.5, 0.0].repeat(3))]);
    if let Some(t) = tangents {
        m = m.with_tangents(t);
    }
    m
};
// Without a base TANGENT the target is rejected and nothing is written.
assert!(build(None).build_into(&mut builder).is_err());
build(Some(vec![1.0, 0.0, 0.0, 1.0].repeat(3))).build_into(&mut builder)?;
# Ok(())
# }
```

## Sparse deltas

Most targets move only a few vertices. A sparse delta stores just those: strictly increasing
vertex `indices` and three values each. It is written as a glTF sparse accessor with no
`bufferView`, so every other vertex reads as zero. Sparse and dense deltas decode identically.

```rust
use mesh_tools::{GltfBuilder, MorphDelta, MorphTarget, PrimitiveSpec};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut builder = GltfBuilder::new();
let mut prim = builder.create_primitive(PrimitiveSpec {
    positions: vec![0.0; 18], // 6 vertices (one non-indexed triangle pair)
    ..Default::default()
})?;

// Vertices 1 and 3 move; 0, 2 and 4 stay.
let a = MorphTarget::new().with_sparse_positions(vec![1, 3], vec![0.0, 1.0, 0.0, 0.0, 2.0, 0.0]);

// From a dense array: exact zeros are dropped; `Some(eps)` also drops |delta| <= eps (lossy,
// only on request). Nothing left is an error.
let dense = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
let b = MorphTarget::new().with_position_delta(MorphDelta::sparse_from_dense(&dense, None)?);
assert_eq!(a.positions, b.positions);

// From absolute morphed positions: deltas = morphed - base.
let base = [0.0; 18];
let mut morphed = [0.0; 18];
morphed[4] = 1.0;
let delta = MorphDelta::from_absolute(&base, &morphed)?; // dense deltas, 18 floats
let c = MorphTarget::new().with_positions(delta);

// Invalid sparse data is rejected before anything is written.
let bad = MorphTarget::new().with_sparse_positions(vec![3, 1], vec![0.0; 6]);
assert!(builder.attach_morph_targets(&mut prim, &[bad]).is_err());
builder.attach_morph_targets(&mut prim, &[a, c])?;
# Ok(())
# }
```

`with_sparse_normals` and `with_sparse_tangents` work the same way, and `with_normal_delta` /
`with_tangent_delta` take a `MorphDelta`.

## Validation

`GltfBuilder::validate()` and export check morph data. Messages start with a stable code:

- Targets: `MORPH_TARGETS_EMPTY`, `MORPH_TARGETS_INCONSISTENT`, `MORPH_TARGET_EMPTY`,
  `MORPH_TARGET_ACCESSOR_INVALID`, `MORPH_TARGET_ATTRIBUTE_INVALID`,
  `MORPH_TARGET_ATTRIBUTE_NOT_IN_BASE`, `MORPH_TARGET_TYPE`, `MORPH_TARGET_COUNT_MISMATCH`
- Weights and names: `MESH_WEIGHTS_EMPTY`, `MESH_WEIGHTS_COUNT`, `MESH_WEIGHTS_NOT_FINITE`,
  `MESH_TARGET_NAMES_COUNT`, `MESH_TARGET_NAMES_INVALID`, `NODE_WEIGHTS_WITHOUT_MESH`,
  `NODE_WEIGHTS_COUNT`, `NODE_WEIGHTS_NOT_FINITE`
- Animation: `ANIMATION_CHANNEL_WEIGHTS_NO_MORPHS`, `ANIMATION_OUTPUT_TYPE`,
  `ANIMATION_OUTPUT_COUNT`
- Warnings (never block export): `MORPH_TARGET_POSITION_NO_BOUNDS`, `MORPH_ATTRIBUTE_BUDGET`,
  `MESH_TARGET_NAMES_DUPLICATE`

A non-finite weight would otherwise be written as `null`; it is an error instead.

## Limits

- `POSITION`, `NORMAL` and `TANGENT` deltas only. Custom `_APP` attribute deltas are not
  supported by the builder (the validator accepts them in hand-built documents).
- Delta accessors are always FLOAT. Normalized-integer deltas need `KHR_mesh_quantization`,
  which is not implemented; the validator rejects them (`MORPH_TARGET_TYPE`).
- The generator shapes (`create_box`, `create_sphere`, ...) have no morph parameter and add
  their meshes directly. Prefer `MeshBuilder` or `create_skinned_primitive`. A generated mesh can
  still be morphed by taking its primitive out of `builder.gltf.meshes`, calling
  `attach_morph_targets`, and putting it back (`tests/morph_targets.rs` does this with a box).
- `MORPH_ATTRIBUTE_BUDGET` is a heuristic (16 attributes in total) and the validator's codes are
  this library's own names. The Khronos glTF Validator has not been run against this library's
  output.

A complete runnable example that verifies its own output is in `examples/morph_demo.rs`:

```bash
cargo run --example morph_demo [output.glb]
```
