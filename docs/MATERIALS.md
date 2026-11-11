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
| `SG_FACTOR_RANGE` | error | a specular-glossiness diffuse, specular or glossiness component outside [0, 1] (non-finite values give `MATERIAL_FACTOR_NOT_FINITE`) |
| `SG_TEXTURE_INVALID` | error | a specular-glossiness texture references a texture that does not exist |
| `SG_EXTENSION_REQUIRED` | warning | `KHR_materials_pbrSpecularGlossiness` is listed in `extensionsRequired` |
| `SG_WITHOUT_FALLBACK` | warning | a specular-glossiness material has no `pbrMetallicRoughness` fallback |

```rust
use mesh_tools::{material::MaterialBuilder, GltfBuilder, GltfError};

let mut b = GltfBuilder::new();
let err = b
    .add_material_from(MaterialBuilder::new(None).with_base_color_texture(5).build())
    .unwrap_err();
assert!(matches!(err, GltfError::InvalidMaterial { code: "MATERIAL_TEXTURE_INVALID", .. }));
```

## Specular-glossiness (deprecated extension)

`KHR_materials_pbrSpecularGlossiness` is deprecated upstream in favour of
`KHR_materials_specular` + `KHR_materials_ior`, which this crate does not write. It is write-only
here. `MaterialBuilder::with_specular_glossiness(SpecularGlossiness { .. })` sets the diffuse and
specular factors, glossiness and the two textures (each a `TextureRef`, so each has its own
`texCoord`). The normal, occlusion and emissive slots are shared with the core model.

Set the metallic-roughness properties on the same builder to write a fallback for viewers that
ignore the extension. `add_material_from` registers the extension in `extensionsUsed` exactly
once and never adds it to `extensionsRequired`.

```rust
use mesh_tools::{material::MaterialBuilder, GltfBuilder, GltfError, SpecularGlossiness, TextureRef};

let mut b = GltfBuilder::new();
let tex = b.create_uv_test_texture(16, 16).unwrap();
let m = MaterialBuilder::new(Some("Plastic".into()))
    .with_base_color([0.8, 0.1, 0.1, 1.0]) // fallback
    .with_roughness_factor(0.3)
    .with_specular_glossiness(SpecularGlossiness {
        diffuse_factor: Some([0.8, 0.1, 0.1, 1.0]),
        diffuse_texture: Some(TextureRef::new(tex).with_tex_coord(1)),
        specular_factor: Some([0.5, 0.5, 0.5]),
        glossiness_factor: Some(0.7),
        ..Default::default()
    })
    .try_build()
    .unwrap();
b.add_material_from(m).unwrap();
assert_eq!(b.gltf.extensions_used.as_deref().unwrap(), ["KHR_materials_pbrSpecularGlossiness"]);
assert!(b.gltf.extensions_required.is_none());

// glossiness outside [0, 1]
let bad = MaterialBuilder::new(None)
    .with_specular_glossiness(SpecularGlossiness { glossiness_factor: Some(2.0), ..Default::default() })
    .try_build();
assert!(matches!(bad, Err(GltfError::InvalidMaterial { code: "SG_FACTOR_RANGE", .. })));
```

`add_specular_glossiness_material` and `add_textured_material` are deprecated; they now return
`Result` and reject the invalid input they used to store silently.

## Normalized texture coordinates

`TEXCOORD_n` may be normalized `u8` or `u16` (`TexCoords::U8` / `TexCoords::U16` in a
`PrimitiveSpec`); `0..=255` reads back as `0.0..=1.0`. The material's `texCoord` picks the set.
