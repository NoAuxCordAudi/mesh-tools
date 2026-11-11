# Changelog

## 0.9.0

Typed accessor writer (part 1 of the Accessors / Buffer Views work; no README row becomes Full yet,
sparse accessors and interleaved views are still to come). Existing output is byte-identical,
pinned by golden tests for the primitive, skin, animation and morph writers
(`tests/accessor_goldens.rs`).

### Added
- `ComponentType`, `AccessorType`, `BufferViewTarget`, the sealed `Component` trait (`i8`, `u8`,
  `i16`, `u16`, `u32`, `f32`), `AccessorSpec` and `Bounds` (`None`, `Compute`, `Explicit`).
- `GltfBuilder::add_accessor_from_slice` (pads MAT2/MAT3 columns from tight input, rejects NaN and
  infinity, normalized only for 8/16-bit integers), `add_accessor_in_view`, `add_zero_accessor`
  (no `bufferView`, writes no bytes) and `add_data_view`. `add_accessor_in_view` also rejects NaN/infinity in `FLOAT` data. All validate first, so an `Err` leaves
  the document unchanged.
- Validator errors `ACCESSOR_NORMALIZED_INVALID`, `ACCESSOR_BUFFER_VIEW_MISSING` (a `byteOffset`
  without a `bufferView`), `ACCESSOR_BOUNDS_LENGTH`, `ACCESSOR_BOUNDS_ORDER`,
  `ACCESSOR_BOUNDS_TYPE` (non-integer or out-of-range bound on an integer accessor),
  `ACCESSOR_BOUNDS_MISMATCH` (bounds differ from the data; an error from the start),
  `ACCESSOR_BOUNDS_REQUIRED` (`POSITION` without min/max) and `ACCESSOR_DATA_NONFINITE`.
- Bounds of integer accessors are written as JSON integers.

### Changed (breaking, pre-1.0)
- `Accessor.buffer_view` is `Option<usize>` (`None` = all zeros). `Accessor::default()` therefore
  has no view; set `Some(i)` when building by hand.
- `Accessor.min` / `max` are `Option<Vec<f64>>` (were `f32`), so `u32` bounds above 2^24 are exact.
  `FLOAT` bounds are the widened `f32` values, as before.
- Every accessor the crate writes (primitives, skins, inverse bind matrices, morph targets,
  animation) goes through one writer. The `POSITION` accessor of any primitive must now carry
  `min`/`max` (`ACCESSOR_BOUNDS_REQUIRED`); builder-made primitives always do.

## 0.8.0

Buffer and view hardening (groundwork for the Buffers, Buffer Views and Accessors rows; no row
becomes Full yet). Output for every document that contains data is byte-identical, pinned by golden
tests for the box, plane and cylinder.

### Fixed
- Vertex, normal, texcoord and index bytes are written with `to_le_bytes` instead of
  reinterpreting native-endian memory with `unsafe` (wrong on big-endian hosts).
- The validator uses checked arithmetic for every buffer view, accessor and animation range, so
  hand-edited values such as `usize::MAX` produce errors instead of panics.
- `element_size` pads matrix columns to 4 bytes as the spec requires: MAT2 BYTE is 8, MAT3 BYTE is
  12 and MAT3 SHORT is 24 bytes (previously unpadded, so `ACCESSOR_RANGE` was too lenient).
- The validator decodes `JOINTS_n` / `WEIGHTS_n` honouring `byteStride`; strided data used to be
  skipped silently, hiding `SKIN_JOINT_OUT_OF_RANGE`.

### Added
- `name` on `Buffer`, `BufferView` and `Accessor`, with `set_buffer_name`,
  `set_buffer_view_name` and `set_accessor_name` (return `Err` for a missing index).
- Validator errors `BUFFER_EMPTY`, `BUFFER_VIEW_LENGTH_ZERO`, `BUFFER_VIEW_TARGET_INVALID`,
  `BUFFER_VIEW_STRIDE_RANGE` (4..=252), `BUFFER_VIEW_STRIDE_ALIGNMENT`,
  `BUFFER_VIEW_STRIDE_ON_NON_VERTEX` (index, animation and inverse-bind-matrix data), `BUFFER_VIEW_STRIDE_TOO_SMALL`,
  `BUFFER_VIEW_TARGET_MISMATCH`, `ACCESSOR_OFFSET_ALIGNMENT`, `ACCESSOR_STRIDE_ALIGNMENT`.
  `BUFFER_VIEW_OFFSET_ALIGNMENT` is a warning (a view with a target whose byteOffset is not a
  multiple of 4 is legal, but GPU uploads may need a copy).
- `BUFFER_LENGTH_MISMATCH` now covers every buffer index: a buffer after the first with no `uri`
  has no stored data.

### Changed (breaking for hand-edited documents only)
- Buffer 0 is created lazily by the first data write. An empty document now has no `buffers`
  array (it used to emit an invalid zero-length buffer). Code that indexes
  `gltf.buffers[0]` on an empty builder must check for presence.
- The validator is stricter on hand-edited documents (new codes above). Builder-made documents are
  unaffected.

## 0.7.0

Model groundwork for extensions/extras (part of the "Custom Extensions" and "Asset Information"
work). Default output is byte-identical except where noted under Breaking.

### Added
- `extensions: Option<Extensions>` and `extras: Option<serde_json::Value>` on every model struct
  (`Asset`, `Scene`, `Node`, `Mesh`, `Primitive`, `Accessor`, `BufferView`, `Buffer`, `Material`,
  `PbrMetallicRoughness`, the texture-info structs, `Texture`, `Image`, `Sampler`, `Skin`,
  `Animation`, `AnimationChannel`, `AnimationChannelTarget`, `AnimationSampler`,
  `PbrSpecularGlossiness`). Payloads are opaque JSON.
- `Extensions` type alias (`serde_json::Map<String, Value>`).
- `Asset.min_version` (`minVersion`).
- `MaterialExtensions.other`: flattened map keeping unknown material extensions.
- `Default` on `Accessor`, `BufferView`, `Buffer`, `Texture` and `Image`.

- `ExtensionTarget`, `GltfBuilder::set_extension` / `set_extras` (opaque JSON on the root,
  asset, scenes, nodes, meshes, primitives, materials, `pbrMetallicRoughness`, textures, images,
  samplers, accessors, buffer views, buffers, skins, animations, channels and samplers),
  `declare_extension_used` and `require_extension`. See `docs/EXTENSIONS.md`.
- Asset setters `set_copyright`, `set_generator`, `clear_generator`, `set_min_version`.
- `set_mesh_target_names`. Mesh `extras` set through `set_extras` and morph `targetNames` are
  merged in either call order.
- Validator codes `ASSET_VERSION_INVALID`, `ASSET_MIN_VERSION_INVALID`,
  `ASSET_MIN_VERSION_EXCEEDS_VERSION`, `ASSET_COPYRIGHT_EMPTY`, `ASSET_GENERATOR_EMPTY`,
  `EXTENSION_VALUE_NOT_OBJECT`, `EXTENSION_NOT_DECLARED`, `EXTENSIONS_USED_DUPLICATE`,
  `EXTENSIONS_USED_EMPTY`, `EXTENSIONS_REQUIRED_EMPTY`, `EXTENSIONS_REQUIRED_NOT_USED` (errors),
  `EXTENSION_NAME_INVALID`, `EXTENSION_DECLARED_UNUSED` (warnings). There is no
  `EXTRAS_NOT_FINITE`: `serde_json::Value` cannot hold NaN or infinity.
- `add_specular_glossiness_material` now registers its extension through the same registry;
  its output is unchanged (covered by a test).
- Corpus document `extensions`.

### Breaking
- Documents that use an extension without listing it in `extensionsUsed`, or that have an empty
  `extensionsUsed` / `extensionsRequired` array (previously silently stripped on export), now fail
  validation and export. `to_glb_bytes_unchecked` keeps such an empty array in the output.
- Asset fields that are empty strings or malformed versions are now validation errors.
- `Gltf.extras` is now `Option<serde_json::Value>` (was `Option<HashMap<String, Value>>`); the
  spec allows any JSON value.
- `Gltf.extensions` is now `Option<Extensions>` (was `Option<Value>`).
- New public fields break struct literals. `Asset`, `Scene`, `Node`, `Primitive`, `Accessor`,
  `BufferView`, `Buffer`, `Material`, `TextureInfo`, `NormalTextureInfo` and
  `OcclusionTextureInfo` are now `#[non_exhaustive]`, so they cannot be built with a struct
  expression outside the crate (not even with `..Default::default()`): create them with
  `T::default()` and assign fields. Future fields will not break this again. The remaining
  structs also gained fields, so their literals need `..Default::default()`.
- `MaterialExtensions` literals need `..Default::default()` (new `other` field).

## 0.6.1

Validation harness. No library behavior changes.

### Added
- `tests/corpus.rs`: one document per README feature, written to `target/corpus/*.glb`, checked
  by the in-crate validator and read back through the independent `gltf` crate.
- Khronos glTF-Validator (pinned 2.0.0-dev.3.10) integration: a CI job runs it on the corpus and
  fails on any error or any warning not in `tests/khronos/allowlist.txt`. It fails (never
  skips) when the binary is missing. Locally, opt in with `GLTF_VALIDATOR=/path/to/gltf_validator
  cargo test --test corpus -- --ignored`.
- Negative cross-checks: documents broken through the public `gltf` field must be rejected by
  both the in-crate validator and the Khronos validator, with the code mapping recorded in
  `docs/VALIDATION.md`.
- The `gltf` dev-dependency enables the `extras`, `extensions`, `names`,
  `KHR_materials_pbrSpecularGlossiness` and `KHR_texture_transform` features.

## 0.6.0

Morph targets (blend shapes) and morph target weights animation.

### Fixed
- `generate_plane` / `create_plane`: triangles wound clockwise seen from +Y (back-facing against the
  +Y normals); now counter-clockwise. 0 width or depth segments produced NaN positions; counts
  below 1 are now raised to 1.
- `generate_cylinder` / `generate_cone` (and `create_cylinder` / `create_cone`): cap centre
  vertices were indexed at `positions.len() / 3` instead of `positions.len()`, so caps fanned
  from an unrelated vertex; side and cap triangles wound inward (clockwise from outside); sloped
  side normals used the reciprocal slope (`h / (rb - rt)` instead of `(rb - rt) / h`), which also
  flipped them; zero-radius caps and collapsed apex triangles produced zero-area faces and are
  now skipped. Output geometry of these shapes changes accordingly.

### Breaking changes
- `texture::generate_checkerboard`, `create_colored_checkerboard` and `create_uv_test_pattern`
  now return `Result` (new `TextureError::InvalidParameter`): a zero cell size (or a UV test
  pattern narrower than 8 px) used to panic with a division by zero.
- `Primitive`, `Mesh` and `Node` gain public fields (`Primitive.targets`, `Mesh.weights`,
  `Mesh.extras`, `Node.weights`). Code that builds these structs with a struct literal must add
  `..Default::default()`, and exhaustive destructuring patterns need `..`. The types still
  derive `Default`. Documents that don't use morph targets serialize exactly as before.
- Animation channels with the `weights` path are now accepted. This supersedes the 0.5.0 rule
  that rejected them. `add_animation_channel` with the `weights` path rejects a node without a
  mesh that has morph targets (all primitives with the same number of targets) and a sampler
  whose output count is not exactly `N * M` (`3 * N * M` for cubic spline).
- A node with a `matrix` can now carry a `weights` channel (the spec only forbids a matrix with
  translation, rotation and scale animation).
- `add_animation_sampler`: for a SCALAR output, a positive multiple of the keyframe count `N`
  (of `3N` for cubic spline) is accepted, because the number of morph targets is not known at
  that level. The exact `N * M` count is checked when a `weights` channel is added.
- Validation is stricter for **all** animation channels, not only weights. `validate()` and
  export now also report `ANIMATION_INPUT_TYPE`, `ANIMATION_INTERPOLATION_INVALID`,
  `ANIMATION_OUTPUT_TYPE` and `ANIMATION_OUTPUT_COUNT` for translation, rotation and scale
  channels, and a `CUBICSPLINE` sampler with fewer than two keyframes. Documents built through
  the builder are unaffected; hand-edited documents with such problems now fail to export.

### Added
- `MorphTarget` (`positions`, `normals`, `tangents` deltas; `#[non_exhaustive]`, built with
  `MorphTarget::new().with_positions(..)`) and `GltfBuilder::attach_morph_targets`, which works
  on a primitive before it is added to a mesh, including skinned primitives.
- `GltfBuilder::add_morphable_mesh` (default weights, target names in
  `mesh.extras.targetNames`) and `GltfBuilder::set_node_weights`.
- `MeshBuilder::with_morph_targets`, `with_default_weights` and `with_target_names`.
  `build_into` validates all morph data before writing anything. Morph data cannot be combined
  with a custom primitive builder, and default weights or names require morph targets.
- `add_weights_track` and `add_cubic_weights_track`. The number of targets always comes from
  the node's mesh.
- Validator codes for morph targets, mesh and node weights, target names, and weights
  animation channels: `MORPH_TARGETS_EMPTY`, `MORPH_TARGETS_INCONSISTENT`, `MORPH_TARGET_EMPTY`,
  `MORPH_TARGET_ACCESSOR_INVALID`, `MORPH_TARGET_ATTRIBUTE_INVALID`,
  `MORPH_TARGET_ATTRIBUTE_NOT_IN_BASE`, `MORPH_TARGET_TYPE`, `MORPH_TARGET_COUNT_MISMATCH`,
  `MESH_WEIGHTS_EMPTY`, `MESH_WEIGHTS_COUNT`, `MESH_WEIGHTS_NOT_FINITE`,
  `MESH_TARGET_NAMES_COUNT`, `MESH_TARGET_NAMES_INVALID`, `NODE_WEIGHTS_WITHOUT_MESH`,
  `NODE_WEIGHTS_COUNT`, `NODE_WEIGHTS_NOT_FINITE`, `ANIMATION_CHANNEL_WEIGHTS_NO_MORPHS`,
  `ANIMATION_INPUT_TYPE`, `ANIMATION_INTERPOLATION_INVALID`, `ANIMATION_OUTPUT_TYPE`,
  `ANIMATION_OUTPUT_COUNT` (these four apply to every animation path); and the warnings
  `MORPH_TARGET_POSITION_NO_BOUNDS`, `MORPH_ATTRIBUTE_BUDGET` and `MESH_TARGET_NAMES_DUPLICATE`.
  A non-finite default or node weight is rejected by both the builder and the validator.
- `docs/MORPH_TARGETS.md` (doctested) and `examples/morph_demo.rs`, which re-opens its own
  output with the `gltf` crate and verifies it.
- Tests that read documents back through the `gltf` crate and compare against hand-derived
  morphing, skinning and animation-sampling values.

### Not supported yet
- Tangent deltas (the library cannot create base `TANGENT` attributes), sparse accessors,
  normalized integer delta types, and morph target hooks on the generator shapes
  (`create_box`, `create_sphere`, ...).
- Normalized `u8`/`u16` joint weights.

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
