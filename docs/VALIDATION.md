# Validation

mesh-tools validates documents with three independent layers:

1. **In-crate**: `GltfBuilder::validate()` (also run by `export_glb` / `to_glb_bytes`). Every
   message starts with a stable upper-case code.
2. **Independent reader**: tests re-open the exported GLB with the `gltf` crate and compare
   against hand-derived values.
3. **Khronos glTF-Validator**: the reference validator, run on the corpus in CI.

```rust
use mesh_tools::GltfBuilder;

let mut b = GltfBuilder::new();
let mesh = b.create_box(1.0);
b.add_node(None, Some(mesh), None, None, None);

// Break the document through the public field: the validator reports a stable code.
b.gltf.nodes.as_mut().unwrap()[0].mesh = Some(99);
assert!(b.validate().has_error("NODE_MESH_INVALID"));
assert!(b.to_glb_bytes().is_err());
```

## The corpus

`tests/corpus.rs` has `corpus()`: one document per README feature. `cargo test` writes them to
`target/corpus/*.glb`. Every new feature adds its document there.

## Running the Khronos validator

CI pins `gltf_validator` 2.0.0-dev.3.10 (Linux release binary from
<https://github.com/KhronosGroup/glTF-Validator/releases>) and runs:

```text
GLTF_VALIDATOR=/path/to/gltf_validator cargo test --test corpus -- --ignored
```

Locally, `tests/khronos/run.sh` downloads the pinned release into `target/khronos/`, optionally
verifies it against `GLTF_VALIDATOR_SHA256`, and runs the same command. Any binary whose
`--version` does not name the pinned release is rejected, so the verdict is reproducible.

The two Khronos tests are `#[ignore]`d so a plain `cargo test` needs no binary and shows the
reason. They never pass silently: with `--ignored`, a missing or unusable `GLTF_VALIDATOR`
makes them fail. The CI job fails if the download or install fails.

The validator is invoked as `gltf_validator -o -r <file>` (JSON report on stdout, embedded
resources validated). The build fails on every **error** and on every **warning** whose code is
not in `tests/khronos/allowlist.txt`. Infos and hints are reported but never fail. A truncated
report fails (issues would be hidden). Errors cannot be allow-listed.

Allow-list format, one entry per line, reason mandatory (an entry without one fails a test):

```text
CODE | why this finding is accepted
```

### Status of the corpus run

The Khronos corpus has **not been run locally**: the development sandbox blocks downloading and
executing the release binary, so no real validator output exists yet. Instead the corpus was
audited statically (a script independent of the crate parsed every `target/corpus/*.glb`)
against the rules the validator is known to enforce: GLB chunk alignment, accessor alignment,
`min`/`max` against the data, finite values, unit normals/tangents (w = +-1), weight sums,
attribute count agreement, index range and primitive-restart values, mode count rules, degenerate
triangles, `TEXCOORD_n` presence, skinned-mesh/skin pairing, inverse bind matrix shape,
animation input monotonicity, unit rotations, undeclared extensions and PNG chunk CRCs.

Triage: no error or warning findings. Expected infos only (not blocking): a normal texture
without `TANGENT` (`MESH_PRIMITIVE_GENERATED_TANGENT_SPACE`, info) in `pbr_slots` and `textured`,
and a joint node with no children (`NODE_EMPTY`, info) in `skinned`. The first real run happens
in the CI `khronos-validator` job; any finding there must be fixed or, for a warning, allow-listed
with a reason. No README row depends on the Khronos run yet.

Known gap: the CI download is pinned by version but not by checksum.

## Negative cross-checks

Documents broken through the public `gltf` field must be rejected by both validators:

| Case | In-crate code | Khronos code |
|---|---|---|
| Node references a missing mesh | `NODE_MESH_INVALID` | `UNRESOLVED_REFERENCE` |
| Node cycle | `NODE_CYCLE` | `NODE_LOOP` |
| Node with two parents | `NODE_PARENT_OVERRIDE` | `NODE_PARENT_OVERRIDE` |
| Matrix together with TRS | `NODE_MATRIX_AND_TRS` | `NODE_MATRIX_TRS` |
| Buffer view past its buffer (per buffer) | `BUFFER_VIEW_RANGE` | `BUFFER_VIEW_TOO_LONG` |
| Buffer view references a missing buffer | `BUFFER_VIEW_BUFFER_INVALID` | `UNRESOLVED_REFERENCE` |
| Buffer length differs from its data | `BUFFER_LENGTH_MISMATCH` | `BUFFER_EXTERNAL_BYTE_LENGTH_MISMATCH` / `BUFFER_GLB_CHUNK_TOO_BIG` |
| Views into a buffer whose bytes are not held | `BUFFER_DATA_UNVERIFIED` (warning) | none (in-crate only) |
| Accessor past its buffer view | `ACCESSOR_RANGE` | `ACCESSOR_TOO_LONG` |
| Scene references a missing node | `SCENE_NODE_INVALID` | `UNRESOLVED_REFERENCE` |
| Primitive references a missing material | `PRIMITIVE_MATERIAL_INVALID` | `UNRESOLVED_REFERENCE` |
| Node references a missing camera | `NODE_CAMERA_INVALID` | `UNRESOLVED_REFERENCE` |
| Camera `zfar` not beyond `znear` | `CAMERA_ZFAR_INVALID` | `CAMERA_ZFAR_LEQUAL_ZNEAR` |

Other camera codes (see [CAMERAS.md](CAMERAS.md)) have no negative cross-check. Severities
follow the spec text: the Khronos validator may report some of them (zero `xmag`/`ymag`, negative
magnitudes, a large `yfov`, an unused camera) at a different severity, which the CI run will show.

Extension and asset codes (`ASSET_*`, `EXTENSION_*`, `EXTENSIONS_*`) are listed in
[EXTENSIONS.md](EXTENSIONS.md); the Khronos validator reports the same problems as
`UNDECLARED_EXTENSION`, `EMPTY_ENTRY`, `UNUSED_EXTENSION_REQUIRED` and `UNSUPPORTED_EXTENSION`.
The `extensions_everywhere` corpus document carries extensions and extras on every addressable object (texture infos, sparse objects, channel targets); the in-crate `EXTENSION_NOT_DECLARED` search covers them all.
Skin codes (see [SKINNING.md](SKINNING.md)) correspond to the Khronos `ACCESSOR_WEIGHTS_NEGATIVE`,
`ACCESSOR_WEIGHTS_NON_NORMALIZED`, `ACCESSOR_JOINTS_INDEX_DUPLICATE`, `ACCESSOR_JOINTS_USED_ZERO_WEIGHT`
(a warning there, so `SKIN_JOINT_ZERO_WEIGHT_INDEX` is one too), `NODE_SKINNED_MESH_NON_ROOT`
(`SKINNED_NODE_NOT_ROOT`) and `NODE_SKINNED_MESH_WITHOUT_SKIN` (`SKINNED_MESH_WITHOUT_SKIN`); the
inverse bind matrix view rule maps to a buffer view usage error. The names and severities are
from memory, so no negative cross-check is registered for them yet. The `skinned_u8_weights`
and `skinned_mixed_sets` corpus documents cover normalized integer weights, two influence sets
and an inverse-bind-matrix-less skin.
Animation codes (see [ANIMATION.md](ANIMATION.md)): `ANIMATION_INPUT_NOT_INCREASING` and
`ANIMATION_INPUT_NEGATIVE` correspond to the Khronos `ACCESSOR_ANIMATION_INPUT_NON_INCREASING` and
`ACCESSOR_ANIMATION_INPUT_NEGATIVE` (errors), `ANIMATION_CHANNEL_DUPLICATE` to
`ANIMATION_DUPLICATED_TARGETS` (error), `ANIMATION_ROTATION_NOT_UNIT` to `ACCESSOR_NON_UNIT`
(error). Non-finite keyframe data and a wrong time `min`/`max` reuse `ACCESSOR_DATA_NONFINITE` and
`ACCESSOR_BOUNDS_MISMATCH` instead of animation-specific copies. The `animation_normalized` and
`animation_shared_sampler` corpus documents cover normalized integer outputs and sampler reuse.
These mappings are unverified against a local run of the pinned validator.

The Khronos codes come from the validator's `lib/src/errors.dart` at the pinned version. Later
milestones add rows here when they add in-crate codes.
