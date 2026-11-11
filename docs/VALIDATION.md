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

Known gap: the CI download is pinned by version but not by checksum.

## Negative cross-checks

Documents broken through the public `gltf` field must be rejected by both validators:

| Case | In-crate code | Khronos code |
|---|---|---|
| Node references a missing mesh | `NODE_MESH_INVALID` | `UNRESOLVED_REFERENCE` |
| Node cycle | `NODE_CYCLE` | `NODE_LOOP` |
| Node with two parents | `NODE_PARENT_OVERRIDE` | `NODE_PARENT_OVERRIDE` |
| Matrix together with TRS | `NODE_MATRIX_AND_TRS` | `NODE_MATRIX_TRS` |
| Buffer view past its buffer | `BUFFER_VIEW_RANGE` | `BUFFER_VIEW_TOO_LONG` |
| Accessor past its buffer view | `ACCESSOR_RANGE` | `ACCESSOR_TOO_LONG` |
| Scene references a missing node | `SCENE_NODE_INVALID` | `UNRESOLVED_REFERENCE` |
| Primitive references a missing material | `PRIMITIVE_MATERIAL_INVALID` | `UNRESOLVED_REFERENCE` |

The Khronos codes come from the validator's `lib/src/errors.dart` at the pinned version. Later
milestones add rows here when they add in-crate codes.
