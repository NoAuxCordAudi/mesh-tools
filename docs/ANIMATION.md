# Animation

Keyframe animation for node translation, rotation, scale and morph target weights. Every Rust
code block in this file is compiled and run by `cargo test --doc`.

## Model

- An **animation** holds **samplers** (keyframe times plus output values plus an interpolation)
  and **channels** (which node and path a sampler drives).
- Interpolation: `LINEAR`, `STEP` and `CUBICSPLINE`. Cubic splines store an in-tangent, a value
  and an out-tangent per keyframe and need at least two keyframes.
- Times are `FLOAT`, finite, `>= 0` and strictly increasing; the time accessor carries `min`/`max`.
- Within one animation each `(node, path)` pair has at most one channel.
- Not supported: `KHR_animation_pointer` and extension channel paths. A channel's `path` is one of
  `translation`, `rotation`, `scale`, `weights`.

## Tracks

The track functions write the accessors, the sampler and the channel in one atomic call: on
`Err` the document is unchanged.

```rust
use mesh_tools::{CubicKeyframe, GltfBuilder, InterpolationType};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut b = GltfBuilder::new();
let node = b.add_node(None, None, None, None, None);
let anim = b.add_animation(Some("Spin".into()));

b.add_translation_track(anim, node, &[0.0, 1.0], &[[0.0; 3], [1.0, 0.0, 0.0]], InterpolationType::Linear)?;

let k = |v: [f32; 3]| CubicKeyframe { in_tangent: [0.0; 3], value: v, out_tangent: [0.0; 3] };
b.add_cubic_scale_track(anim, node, &[0.0, 1.0], &[k([1.0; 3]), k([2.0; 3])])?;

// Times must strictly increase, and a (node, path) pair can only be animated once.
assert!(b.add_rotation_track(anim, node, &[0.0, 0.0], &[[0.0, 0.0, 0.0, 1.0]; 2], InterpolationType::Linear).is_err());
assert!(b.add_translation_track(anim, node, &[0.0], &[[0.0; 3]], InterpolationType::Linear).is_err());
assert!(b.validate().is_valid());
# Ok(())
# }
```

## Normalized integer output

Rotation and weights outputs may be normalized integers instead of `FLOAT`
(`add_rotation_track_normalized`, `add_cubic_rotation_track_normalized`,
`add_weights_track_normalized`, `add_cubic_weights_track_normalized`), chosen with
`NormalizedComponent::{I8, U8, I16, U16}`. Translation and scale must stay `FLOAT`, and there is
no normalized API for them; a hand-built channel with such an output is refused.

- Encoding is the glTF formula, rounded half away from zero: signed `c = round(f * 127)` (`i16`:
  `32767`), unsigned `c = round(f * 255)` (`u16`: `65535`). `-128` and `-32768` are never
  written.
- Representable range: `[-1, 1]` for signed types, `[0, 1]` for unsigned. Values and cubic
  tangents outside it are an error, never clamped. An unsigned rotation therefore needs
  non-negative components.
- Quaternions are normalized in `f32` first, then quantized. The stored quaternion is unit length
  within the quantization step; the validator uses `1.05 / max` as the tolerance for integer
  rotations.

```rust
use mesh_tools::{GltfBuilder, InterpolationType, NormalizedComponent};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut b = GltfBuilder::new();
let node = b.add_node(None, None, None, None, None);
let anim = b.add_animation(None);
// 0.70710678 * 127 = 89.8, stored as 90.
b.add_rotation_track_normalized(
    anim, node, &[0.0, 1.0],
    &[[0.0, 0.0, 0.0, 1.0], [0.0, 0.70710678, 0.0, 0.70710678]],
    InterpolationType::Linear, NormalizedComponent::I8,
)?;
assert!(b.validate().is_valid());

// Unsigned bytes cannot hold a negative component.
let other = b.add_animation(None);
assert!(b.add_rotation_track_normalized(
    other, node, &[0.0], &[[0.0, 0.0, 0.0, -1.0]],
    InterpolationType::Linear, NormalizedComponent::U8,
).is_err());
# Ok(())
# }
```

## Sharing a sampler

`add_channel_with_sampler` adds another channel for an existing sampler, so one set of keyframes
drives several nodes without writing the data again. (It is the documented route for what the low
level `add_animation_channel` does; both run the same checks.)

```rust
use mesh_tools::{AnimationPath, GltfBuilder, InterpolationType};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut b = GltfBuilder::new();
let (left, right) = (b.add_node(None, None, None, None, None), b.add_node(None, None, None, None, None));
let anim = b.add_animation(None);
let (_, sampler) = b.add_translation_track(anim, left, &[0.0, 1.0], &[[0.0; 3], [0.0, 1.0, 0.0]], InterpolationType::Linear)?;
b.add_channel_with_sampler(anim, sampler, right, AnimationPath::Translation)?;
// A VEC3 sampler cannot drive a rotation.
assert!(b.add_channel_with_sampler(anim, sampler, right, AnimationPath::Rotation).is_err());
# Ok(())
# }
```

## Builder and validator rules

The builder already rejects bad data when it is written (non-finite or non-increasing or negative
times, a duplicate `(node, path)`, a zero or non-finite quaternion). `add_animation_sampler` also
reads the stored times of a hand-made accessor and applies the same time rules. The validator
repeats every rule on the stored bytes, so a document edited through the public `gltf` field is
caught too.

| Code | Meaning | Khronos code (unverified) |
|---|---|---|
| `ANIMATION_INPUT_NOT_INCREASING` | a stored time is not greater than the previous one | `ACCESSOR_ANIMATION_INPUT_NON_INCREASING` |
| `ANIMATION_INPUT_NEGATIVE` | a stored time is below zero | `ACCESSOR_ANIMATION_INPUT_NEGATIVE` |
| `ANIMATION_CHANNEL_DUPLICATE` | two channels of one animation drive the same node and path | `ANIMATION_DUPLICATED_TARGETS` |
| `ANIMATION_ROTATION_NOT_UNIT` | a stored rotation (the value entries only, for cubic splines) is not unit length | `ACCESSOR_NON_UNIT` |
| `ANIMATION_OUTPUT_TYPE` / `ANIMATION_OUTPUT_COUNT` | wrong accessor type or element count for the path and interpolation | `ANIMATION_CHANNEL_TARGET_PATH_*`, `ANIMATION_SAMPLER_*` |
| `ANIMATION_INPUT_NO_BOUNDS` | the time accessor lacks `min`/`max` | `ACCESSOR_ANIMATION_INPUT_MISSING_BOUNDS`-like |

Non-finite keyframe data and a time `min`/`max` that disagrees with the data are reported by the
accessor codes `ACCESSOR_DATA_NONFINITE` and `ACCESSOR_BOUNDS_MISMATCH`, not by animation copies.
