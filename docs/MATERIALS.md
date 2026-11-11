# Materials

Materials use the metallic-roughness model. Build one with `MaterialBuilder`, then add it with
`GltfBuilder::add_material_from`, which checks it against the document and stores nothing on error.

## Textures per slot

Every texture slot takes a `TextureRef`: a texture index and the `TEXCOORD_n` set that samples it.
A bare `usize` means set 0. A set of 0, a normal scale of 1 and an occlusion strength of 1 are the
spec defaults and are left out of the JSON.

```rust
use mesh_tools::{material::MaterialBuilder, AlphaMode, GltfBuilder, TextureRef};

let mut b = GltfBuilder::new();
let albedo = b.create_uv_test_texture(16, 16).unwrap();
let normals = b.create_uv_test_texture(16, 16).unwrap();
let ao = b.create_uv_test_texture(16, 16).unwrap();

let material = MaterialBuilder::new(Some("Brick".into()))
    .with_base_color_texture(albedo)                              // TEXCOORD_0
    .with_normal_texture(TextureRef::new(normals).with_tex_coord(1), 0.75)
    .with_occlusion_texture(TextureRef { texture: ao, tex_coord: 1 }, 0.4)
    .with_alpha(AlphaMode::Mask { cutoff: 0.3 })
    .try_build()
    .unwrap();
let index = b.add_material_from(material).unwrap();
assert_eq!(index, 0);
```

## Alpha modes

`AlphaMode::Opaque`, `Mask { cutoff }` and `Blend` replace the string-based `with_alpha_mode`
(deprecated). Only `Mask` carries a cutoff.

## Errors and validator codes

`try_build` and `add_material_from` return `GltfError::InvalidMaterial { index, code, message }`.
`validate()` reports the same codes for documents edited through `builder.gltf`.

| Code | Severity | Meaning |
|---|---|---|
| `MATERIAL_TEXTURE_INVALID` | error | a slot references a texture that does not exist |
| `MATERIAL_FACTOR_RANGE` | error | base color, metallic, roughness or emissive outside [0, 1] |
| `MATERIAL_FACTOR_NOT_FINITE` | error | NaN or infinity in a factor, strength or scale |
| `MATERIAL_ALPHA_MODE_INVALID` | error | `alphaMode` is not OPAQUE, MASK or BLEND |
| `MATERIAL_ALPHA_CUTOFF_INVALID` | error | negative or non-finite `alphaCutoff` |
| `MATERIAL_OCCLUSION_STRENGTH_RANGE` | error | occlusion strength outside [0, 1] |
| `MATERIAL_TEXCOORD_MISSING` | error | a primitive uses a material whose slot samples a `TEXCOORD_n` the primitive lacks (`validate()` only: meshes may be added after materials) |
| `MATERIAL_ALPHA_CUTOFF_UNUSED` | warning | cutoff set but the mode is not MASK |
| `MATERIAL_TEXCOORD_RANGE` | warning | `texCoord` 2 or higher; clients only have to support sets 0 and 1 |
| `MATERIAL_NORMAL_SCALE_ZERO` | warning | normal scale 0 (legal, but flattens the map) |
| `MATERIAL_UNUSED` | warning | no primitive uses the material |

```rust
use mesh_tools::{material::MaterialBuilder, GltfBuilder, GltfError};

let mut b = GltfBuilder::new();
let err = b
    .add_material_from(MaterialBuilder::new(None).with_base_color_texture(5).build())
    .unwrap_err();
assert!(matches!(err, GltfError::InvalidMaterial { code: "MATERIAL_TEXTURE_INVALID", .. }));
```

## Normalized texture coordinates

`TEXCOORD_n` may be normalized `u8` or `u16` (`TexCoords::U8` / `TexCoords::U16` in a
`PrimitiveSpec`); `0..=255` reads back as `0.0..=1.0`. The material's `texCoord` picks the set.
