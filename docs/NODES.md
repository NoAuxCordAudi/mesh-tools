# Nodes: matrix and TRS transforms

A node has either a `matrix` or translation/rotation/scale (TRS), never both. The builder
never clears one form for you: switching is explicit.

```rust
use mesh_tools::GltfBuilder;

let mut b = GltfBuilder::new();

// A matrix node. matrix_from_trs builds T * R * S in glTF column-major order.
let m = GltfBuilder::matrix_from_trs([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], [2.0; 3]);
let n = b.add_node_with_matrix(Some("m".into()), None, m).unwrap();

// Mixing is refused.
assert!(b.set_node_transform(n, [0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]).is_err());
b.clear_node_matrix(n).unwrap();
b.set_node_transform(n, [0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 2.0], [1.0; 3]).unwrap(); // rotation normalized
assert!(b.set_node_matrix(n, m).is_err());

// Invalid matrices are rejected, each with its own code.
let err = b.add_node_with_matrix(None, None, [0.0; 16]).unwrap_err().to_string();
assert!(err.contains("NODE_MATRIX_NOT_AFFINE"));
```

## Rules

- Valid matrix: finite, bottom row `[0, 0, 0, 1]`, invertible. A **negative determinant** (a mirror)
  is legal. **Shear** is allowed but warned about (`NODE_MATRIX_SHEAR`), since glTF only requires
  that the matrix is decomposable to TRS and viewers differ on sheared input.
- `set_node_matrix` rejects a node that has TRS or is targeted by a translation, rotation or scale
  animation channel. Weights channels are independent of the transform. The animation builder in
  turn refuses TRS tracks on matrix nodes.
- Skins: `compute_inverse_bind_matrices` uses a joint's matrix. A joint with `matrix` translate
  (0, 1, 0) gets an inverse bind matrix translate (0, -1, 0). A matrix joint cannot have TRS
  animation.
- Camera references are not supported (there is no camera API), so the README keeps Nodes Partial.

## Validator codes

| Code | Severity | Meaning |
|---|---|---|
| `NODE_MATRIX_AND_TRS` | error | both forms present |
| `NODE_MATRIX_NOT_FINITE` | error | NaN or infinity in the matrix |
| `NODE_MATRIX_NOT_AFFINE` | error | bottom row is not `[0, 0, 0, 1]` |
| `NODE_MATRIX_NOT_DECOMPOSABLE` | error | singular (`\|det\| <= 1e-6 * \|c0\|\|c1\|\|c2\|`) |
| `NODE_MATRIX_SHEAR` | warning | two matrix columns deviate from orthogonal by more than 1e-4 (cosine) |
| `NODE_TRANSLATION_NOT_FINITE`, `NODE_SCALE_NOT_FINITE`, `NODE_ROTATION_NOT_FINITE` | error | non-finite TRS value |
| `NODE_ROTATION_NOT_UNIT` | error | quaternion length differs from 1 by more than 5e-6 |
| `NODE_SCALE_ZERO` | warning | a scale component is exactly 0 |
| `NODE_CHILDREN_EMPTY` | error | `children` present but empty |

The builder's `add_node*` functions normalize rotations, so the new TRS errors only affect
hand-edited documents.
