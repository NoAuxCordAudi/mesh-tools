# Primitive modes

`PrimitiveMode` names the seven glTF topologies. `Primitive.mode` stays an `Option<usize>` in the
model (so documents round-trip unchanged); `Primitive::mode_enum()` converts it, and a missing mode
is triangles.

| Mode | Code | Minimum count | Count must be a multiple of |
|---|---|---|---|
| `Points` | 0 | 1 | 1 |
| `Lines` | 1 | 2 | 2 |
| `LineLoop` | 2 | 2 | 1 |
| `LineStrip` | 3 | 2 | 1 |
| `Triangles` | 4 | 3 | 3 |
| `TriangleStrip` | 5 | 3 | 1 |
| `TriangleFan` | 6 | 3 | 1 |

The count is that of the indices, or of the vertices for non-indexed geometry.

```rust
use mesh_tools::{Primitive, PrimitiveMode};

assert_eq!(PrimitiveMode::LineLoop.code(), 2);
assert_eq!(PrimitiveMode::Lines.count_multiple(), 2);
assert!(PrimitiveMode::TriangleFan.is_valid_count(5));
assert!(!PrimitiveMode::Triangles.is_valid_count(5));
assert_eq!(Primitive::default().mode_enum().unwrap(), PrimitiveMode::Triangles);
assert!(PrimitiveMode::from_code(7).is_err());
```

## Creating primitives

`create_points`, `create_lines`, `create_line_strip(positions, closed, ..)` (a loop when `closed`),
`create_triangle_strip` and `create_triangle_fan` take positions, optional `u32` indices and a
material. They return a `Primitive` for `add_mesh_checked`, and are atomic like
`create_primitive`. Use `create_primitive` with a `PrimitiveSpec { mode, .. }` to add normals,
colors and the rest to a non-triangle primitive.

```rust
use mesh_tools::GltfBuilder;

let mut b = GltfBuilder::new();
// A closed square outline: 4 vertices, drawn as a loop.
let square = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
let outline = b.create_line_strip(&square, true, None, None).unwrap();
let corners = b.create_points(&square, None).unwrap();
b.add_mesh_checked(Some("Outline".into()), vec![outline, corners], None).unwrap();
assert!(b.validate().is_valid());

// Three vertices cannot form line segments.
assert!(b.create_lines(&square[..9], None, None).is_err());
```

## Strips and fans to triangle lists

Renderers that only take triangles need a list. `strip_to_triangles` and `fan_to_triangles` are
pure functions on index lists:

- Strip triangle `i` is `(i, i+1, i+2)` for even `i` and `(i+1, i, i+2)` for odd `i`; the swap keeps
  every triangle facing the same way.
- Fan triangle `i` is `(0, i+1, i+2)`.

Degenerate triangles are kept.

```rust
use mesh_tools::{fan_to_triangles, strip_to_triangles};

assert_eq!(strip_to_triangles(&[0, 1, 2, 3, 4]).unwrap(), [0, 1, 2, 2, 1, 3, 2, 3, 4]);
assert_eq!(fan_to_triangles(&[0, 1, 2, 3, 4]).unwrap(), [0, 1, 2, 0, 2, 3, 0, 3, 4]);
assert!(strip_to_triangles(&[0, 1]).is_err());
```

## Notes

- Index values must be below the vertex count and never the restart value of the index width
  (255, 65535 or 4294967295); glTF has no primitive restart.
- Skin sets, morph targets and every other attribute work with every mode. Normals and tangents are
  of little use for points and lines; the validator does not warn about them.
