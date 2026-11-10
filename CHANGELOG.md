# Changelog

## 0.5.0

Skinning and skeletal animation are now complete and validated. This release has breaking
changes; most are `Result` returns where the old code panicked or silently produced an
invalid file.

### Breaking changes
- `add_skinned_node`, `add_skin`, `create_inverse_bind_matrices_accessor`,
  `add_animation_sampler`, `add_animation_channel` and the `create_*_animation` functions now
  return `Result`. Bad indices, bad times and similar input are errors, not panics.
- `create_skinned_mesh` now validates weights, joint slots and index ranges.
- `MeshBuilder::build()` is removed. It discarded all geometry unless a custom primitive
  builder was set. Use `MeshBuilder::build_into(&mut GltfBuilder)`.
- `to_glb_bytes` and `export_glb` validate the document first and fail with
  `GltfError::InvalidData` listing every problem. Use `to_glb_bytes_unchecked` or
  `export_glb_unchecked` to skip validation.
- Channels with the `weights` path are rejected (morph targets are not supported yet).
- `add_skin` and `add_animation_channel` are stricter: skins need a common root and an ancestor
  skeleton, and a `(node, path)` pair can have one channel per animation; nodes with a
  `matrix` cannot be animated.
- `add_animation` no longer creates empty `channels`/`samplers` arrays, and the builder no
  longer serializes empty top-level arrays (which glTF forbids).
- `AnimationPath` and `InterpolationType` print through `Display` (so `.to_string()` still
  works) and now derive `Debug`, `Clone`, `Copy`, `PartialEq` and `Eq`.
- Animation data and inverse bind matrix buffer views no longer set a `target`.
- `export_glb` serializes before touching the file, so a failed export leaves no partial file.

### Added
- `GltfBuilder::validate()` returning a `ValidationReport` (public `errors` and `warnings`,
  plus `is_valid`, `has_error`, `has_warning`) with stable error codes; the `validate` module.
- `to_glb_bytes_unchecked` and `export_glb_unchecked`.
- `add_skin_from_joints` and `compute_inverse_bind_matrices`: inverse bind matrices computed
  from the node hierarchy.
- `create_inverse_bind_matrices_accessor_from_f32` (raw column-major matrices).
- `create_skinned_primitive` with multiple influence sets (`JOINTS_n`/`WEIGHTS_n`), `u8`/`u16`
  joints, optional indices, and validation that runs before anything is written. Types
  `SkinSet` and `JointIndices`.
- `normalize_weights` and `limit_influences` helpers.
- `MeshBuilder::build_into` and `with_joint_count`.
- Track-based animation API: `add_translation_track`, `add_rotation_track`,
  `add_scale_track`, plus `add_cubic_*_track` with `CubicKeyframe` for CUBICSPLINE.
- `GltfBuilder::to_glb_bytes`.
- Integration tests (using the `gltf` crate as an independent reader), a CI workflow, and
  `docs/SKINNING.md` rewritten with doctested examples.

### Fixed
- `add_skinned_node` did not register the node for parent tracking or normalize its rotation,
  so later `add_child_to_node` calls failed or hit the wrong node.
- Animation time accessors now carry the `min`/`max` the glTF spec requires.
- CUBICSPLINE animations: the old API required one output per keyframe, which is wrong for
  cubic splines (three per keyframe), so valid cubic data could not be expressed. Use the new
  `add_cubic_*_track` functions.
- The skeletal animation example had the wrong inverse bind matrix layout, node indices in
  `JOINTS_0`, a shifted weight row and mismatched mesh and bone axes. It is rewritten and
  now animates.
- `run_all_examples.sh` lists all examples and exits non-zero when one fails.
- A stale `add_material` doctest.

### Not supported yet
- Morph targets and weights animation (animating `weights` returns an explicit error).
- Normalized `u8`/`u16` joint weights.
