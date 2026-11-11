# Textures, images and samplers

A glTF texture pairs an image (the pixels) with a sampler (how they are filtered and wrapped).

## Samplers

`SamplerDesc` holds four optional fields built from the typed enums `MagFilter`, `MinFilter` and
`WrapMode`. A `None` field is left out of the file, so readers apply the spec default (wrapping
`REPEAT`, filtering up to the client).

- `add_sampler(desc)` always appends.
- `get_or_add_sampler(desc)` returns the first existing sampler equal to `desc` by full struct
  equality (the four fields, and no name, extensions or extras), else appends.
- `create_default_sampler()` and the `create_*texture*` helpers share one sampler,
  `SamplerDesc::default_sampler()` (linear, trilinear, repeat). Before 0.11.0 each texture got its
  own copy.

```rust
use mesh_tools::{GltfBuilder, MagFilter, MinFilter, SamplerDesc, WrapMode};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut b = GltfBuilder::new();
let pixel_art = SamplerDesc {
    mag_filter: Some(MagFilter::Nearest),
    min_filter: Some(MinFilter::NearestMipmapNearest),
    wrap_s: Some(WrapMode::ClampToEdge),
    wrap_t: Some(WrapMode::MirroredRepeat),
};
let s = b.get_or_add_sampler(pixel_art);
assert_eq!(b.get_or_add_sampler(pixel_art), s);
# Ok(())
# }
```

## Images

| Method | Source |
|---|---|
| `add_image(name, &DynamicImage, format)` | encodes and embeds in the binary buffer |
| `add_image_bytes(name, format, &[u8])` | embeds PNG/JPEG bytes unchanged |
| `add_image_from_file(name, path)` | reads a file and embeds it; the format comes from the magic number |
| `add_image_uri(name, uri, Option<format>)` | references an external file; the file is not read or checked |

`add_image_bytes` checks the magic number against the declared format and reads the header
(width and height, no pixel decoding), so garbage never reaches the file. `data:` URIs are not
supported yet.

```rust
use mesh_tools::{texture::{create_uv_test_pattern, TextureFormat}, GltfBuilder};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut b = GltfBuilder::new();
let png = mesh_tools::texture::image_to_bytes(&create_uv_test_pattern(8, 8)?, TextureFormat::PNG)?;
let image = b.add_image_bytes(Some("uv".into()), TextureFormat::PNG, &png)?;
let sampler = b.create_default_sampler();
let texture = b.add_texture(Some("uv".into()), image, Some(sampler))?;
assert_eq!(texture, 0);

// Wrong format and missing indices are errors, and leave the document unchanged.
assert!(b.add_image_bytes(None, TextureFormat::JPEG, &png).is_err());
assert!(b.add_texture(None, 9, None).is_err());
# Ok(())
# }
```

## Validation codes

Errors: `SAMPLER_MAG_FILTER_INVALID`, `SAMPLER_MIN_FILTER_INVALID`, `SAMPLER_WRAP_INVALID`,
`TEXTURE_SOURCE_OUT_OF_RANGE`, `TEXTURE_SAMPLER_OUT_OF_RANGE`, `IMAGE_SOURCE_AMBIGUOUS` (needs
exactly one of `uri` and `bufferView`), `IMAGE_MIME_MISSING`, `IMAGE_MIME_INVALID`,
`IMAGE_URI_EMPTY`, `IMAGE_BUFFER_VIEW_INVALID`, `IMAGE_DATA_MISMATCH`, `IMAGE_DATA_UNDECODABLE`,
`IMAGE_DIMENSIONS_INVALID`. Warnings: `SAMPLER_UNUSED`, `IMAGE_UNUSED`.
