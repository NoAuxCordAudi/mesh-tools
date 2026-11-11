# Meshes and primitives

A mesh is a list of primitives; a primitive is one draw call: attributes, optional indices, a
material and a mode (see [PRIMITIVE_MODES.md](PRIMITIVE_MODES.md)). Not covered: attribute types
from `KHR_mesh_quantization` (for example `i8`/`i16` positions).

## Describe a primitive, then add it

`PrimitiveSpec` holds everything a primitive can carry. `create_primitive` validates it, writes the
accessors and returns a `Primitive`; `add_mesh_checked` adds one or more of them as a mesh.

```rust
use mesh_tools::{ColorData, GltfBuilder, Indices, PrimitiveSpec, TexCoords};
use serde_json::json;

let mut b = GltfBuilder::new();
let red = b.create_basic_material(None, [1.0, 0.0, 0.0, 1.0]);

let tri = b
    .create_primitive(PrimitiveSpec {
        positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        indices: Some(Indices::U16(vec![0, 1, 2])),
        normals: Some(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]),
        // x, y, z and the handedness w (1 or -1).
        tangents: Some(vec![1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0]),
        // Integer sets are written normalized: 255 reads as 1.0.
        texcoords: vec![TexCoords::U8(vec![0, 0, 255, 0, 0, 255])],
        colors: vec![ColorData::RgbaU8(vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255])],
        material: Some(red),
        extras: Some(json!({ "part": "body" })),
        ..Default::default()
    })
    .unwrap();

let mesh = b.add_mesh_checked(Some("Triangle".into()), vec![tri], Some(json!({ "author": "me" }))).unwrap();
assert!(b.validate().is_valid());
```

Indices keep the width you give them (`Indices::U8`, `U16`, `U32`); `Indices::auto` picks `u16`
unless a value needs `u32`. The restart value of a width (255, 65535, 4294967295) is never a legal
index.

## Several primitives in one mesh

```rust
use mesh_tools::{GltfBuilder, PrimitiveSpec};

let mut b = GltfBuilder::new();
let quad = |b: &mut GltfBuilder, x: f32| {
    b.create_primitive(PrimitiveSpec {
        positions: vec![x, 0.0, 0.0, x + 1.0, 0.0, 0.0, x, 1.0, 0.0],
        ..Default::default()
    })
    .unwrap()
};
let (a, c) = (quad(&mut b, 0.0), quad(&mut b, 2.0));
let mesh = b.add_mesh_checked(None, vec![a, c], None).unwrap();
assert_eq!(b.gltf.meshes.as_ref().unwrap()[mesh].primitives.len(), 2);

// An empty primitive list is rejected, and nothing is added.
assert!(b.add_mesh_checked(None, vec![], None).is_err());
assert_eq!(b.gltf.meshes.as_ref().unwrap().len(), 1);
```

## MeshBuilder

`MeshBuilder` is the single-primitive shortcut and supports the same data: `with_tangents`,
`with_colors`, `with_mode`, `with_indices_u32`, `with_primitive_extras` and `with_mesh_extras`.
Its `with_primitive_builder` closure is deprecated because it bypasses validation.

```rust
use mesh_tools::{mesh::MeshBuilder, GltfBuilder, PrimitiveMode};

let mut b = GltfBuilder::new();
let pos = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0];
// Lines need an even index count: this is rejected and the document stays empty.
let bad = MeshBuilder::new(None).with_positions(pos.clone()).with_mode(PrimitiveMode::Lines).build_into(&mut b);
assert!(bad.is_err());
assert_eq!(b.gltf.meshes.as_ref().map_or(0, |m| m.len()), 0);
```

## Extras

`set_mesh_extras` and `set_primitive_extras` are wrappers over `set_extras` (see
[EXTENSIONS.md](EXTENSIONS.md)). A mesh with morph target names keeps them under
`extras.targetNames`.

## Validator codes

| Code | Severity | Meaning |
|---|---|---|
| `MESH_NO_PRIMITIVES` | error | the mesh has no primitives |
| `MESH_PRIMITIVE_NO_POSITION` | warning | no `POSITION` (the spec does not strictly require it) |
| `PRIMITIVE_MODE_INVALID` | error | mode is not 0 to 6 |
| `PRIMITIVE_COUNT_INVALID` | error | index (or vertex) count does not fit the mode; a missing mode is 4 |
| `PRIMITIVE_ATTRIBUTE_NAME_INVALID` | error | not a glTF semantic or `_NAME` |
| `PRIMITIVE_ATTRIBUTE_TYPE` | error | accessor type or component type does not fit the semantic |
| `PRIMITIVE_ATTRIBUTE_COUNT_MISMATCH` | error | attributes disagree on the vertex count |
| `PRIMITIVE_ATTRIBUTE_SET_GAP` | warning | `TEXCOORD_1` without `TEXCOORD_0`, and so on |
| `PRIMITIVE_INDEX_OUT_OF_RANGE` | error | an index is not below the vertex count |
| `PRIMITIVE_INDEX_RESTART` | error | an index is the restart value of its width |
| `PRIMITIVE_TANGENT_W_INVALID` | error | tangent `w` is not 1 or -1 |
| `PRIMITIVE_TANGENT_WITHOUT_NORMAL` | warning | `TANGENT` without `NORMAL` |
| `PRIMITIVE_NORMAL_NOT_UNIT` | warning | a normal's length differs from 1 by more than 0.00674 |
| `PRIMITIVE_ACCESSOR_INVALID`, `PRIMITIVE_INDICES_INVALID`, `PRIMITIVE_INDICES_TYPE`, `PRIMITIVE_MATERIAL_INVALID` | error | dangling references, indices not unsigned `SCALAR` |
| `PRIMITIVE_JOINTS_WITHOUT_WEIGHTS`, `PRIMITIVE_WEIGHTS_WITHOUT_JOINTS` | error | unpaired skin sets |

The attribute table: `POSITION`/`NORMAL` are `FLOAT` `VEC3`, `TANGENT` is `FLOAT` `VEC4`,
`TEXCOORD_n` is `VEC2` and `COLOR_n` is `VEC3`/`VEC4` (both `FLOAT` or normalized `u8`/`u16`),
`JOINTS_n` is `VEC4` of `u8`/`u16`, `WEIGHTS_n` is `VEC4` of `FLOAT` or normalized `u8`/`u16`.
Application-specific `_NAME` attributes may use any type.
