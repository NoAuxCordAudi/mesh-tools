# mesh-tools

A Rust library for creating, manipulating, and exporting 3D meshes with support for the glTF 2.0 format (including binary GLB).

## Features

- Core mesh data structures for working with 3D geometry
- Primitive shape generators (box, plane, sphere, cylinder, cone, torus, icosahedron)
- Material creation with PBR properties (base color, metallic, roughness, emissive)
- Scene hierarchy and node transformation support
- Export to spec-conformant binary GLB (chunk alignment and padding handled); the document is validated first
- Skinning and keyframe animation (see [docs/SKINNING.md](docs/SKINNING.md))
- Morph targets (blend shapes) with weights animation (see [docs/MORPH_TARGETS.md](docs/MORPH_TARGETS.md))
- Vertex attributes: positions, normals, multiple UV sets, and skinning joints/weights
- Lightweight math types via mint instead of nalgebra

## Primitive Shapes

The library provides generators for these common 3D shapes:

- Box/Cube with configurable dimensions
- Plane with width and depth segments
- Sphere with configurable radius and segments
- Cylinder with top/bottom radii, height, and segment options
- Cone (special case of cylinder with zero top radius)
- Torus with main radius and tube radius settings
- Icosahedron (20-sided polyhedron)

## Usage

### Creating a Simple Box

```rust
use mesh_tools::GltfBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new glTF builder
    let mut builder = GltfBuilder::new();
    
    // Create a box mesh with a size of 1.0
    let box_mesh = builder.create_box(1.0);
    
    // Create a node referencing the box mesh
    let box_node = builder.add_node(
        Some("BoxNode".to_string()),
        Some(box_mesh),
        Some([0.0, 0.0, 0.0]),   // translation
        None,                     // rotation
        None,                     // scale
    );
    
    // Create a scene with the box node
    builder.add_scene(
        Some("Scene".to_string()),
        Some(vec![box_node]),
    );
    
    // Export the GLB file
    builder.export_glb("box.glb")?;
    
    println!("Successfully exported GLB file: box.glb");
    
    Ok(())
}
```

### Working with Materials

The library supports creating basic and PBR metallic materials:

```rust
// Create a basic colored material
let red_material = builder.create_basic_material(
    Some("Red Material".to_string()),
    [1.0, 0.0, 0.0, 1.0], // Red color
);

// Create a metallic material (e.g., gold)
let gold_material = builder.create_metallic_material(
    Some("Gold Material".to_string()),
    [1.0, 0.84, 0.0, 1.0],  // Gold color
    0.9, // High metallic factor
    0.1, // Low roughness factor (shiny)
);

// Apply material to a mesh
let sphere_mesh = builder.create_sphere(
    1.0,  // radius
    32,   // width segments
    16,   // height segments
    Some(gold_material)
);
```

## Mesh Export

The library exports GLB (binary glTF) and validates the document before writing it:

- Proper handling of chunk types and alignment
- Binary data padding and structure according to glTF spec
- Vertex attributes: positions, normals, UV sets and skinning joints/weights

## Validation

Besides the built-in `validate()`, the test corpus is checked against the Khronos glTF-Validator
in CI. See [docs/VALIDATION.md](docs/VALIDATION.md) for how to run it locally.

## Math Types and Compatibility Layer

This library uses the lightweight [mint](https://crates.io/crates/mint) crate for mathematical types like `Point3`, `Vector2`, and `Vector3`. A compatibility layer is provided to make working with these types easy:

```rust
// Import math types from the compatibility module
use mesh_tools::compat::{Point3, Vector2, Vector3};

// Use constructor functions
let position = mesh_tools::compat::point3::new(1.0, 2.0, 3.0);
let normal = mesh_tools::compat::vector3::new(0.0, 1.0, 0.0);
let uv = mesh_tools::compat::vector2::new(0.5, 0.5);

// Alternatively, use re-exported functions at the module level
use mesh_tools::compat::{point3_new, vector3_new};
let position = point3_new(1.0, 2.0, 3.0);

// Vector operations are also available
use mesh_tools::compat::{cross, normalize, dot};
let cross_product = cross(vec1, vec2);
let unit_normal = normalize(normal);
```

You don't need to add mint as a direct dependency - the library re-exports all necessary types.

## Building and Running

To build the library:

```bash
cargo build
```

To run one of the examples:

```bash
cargo run --example primitives_demo
```

## Examples

The library includes several examples demonstrating different features:

- `simple_box.rs`: Basic box creation and export
- `primitives_demo.rs`: Plane, sphere, cylinder, cone, torus and icosahedron with different materials
- `materials_demo.rs`: Various material types and properties
- `texture_demo.rs`: Texture mapping and image handling
- `custom_mesh_demo.rs`: Creating custom meshes from vertex data
- `hierarchy_demo.rs`: Building scene hierarchies with multiple nodes
- `animation_demo.rs`: Translation, rotation and scale keyframe animation
- `skeletal_animation_demo.rs`: A skinned arm that bends (joints, weights, linear and cubic-spline animation)
- `morph_demo.rs`: A sheet with Bulge and Ripple morph targets and a cubic-spline weights animation; it re-opens its own output and verifies it

More examples live in `examples/` (texture maps, PBR channel demos, terrain, instancing, panel grids, ...).

Run every example with `./run_all_examples.sh` (it exits non-zero if any example fails).

### Primitives Demo

Below is a screenshot of the primitives demo showing the various shapes with different materials that can be generated with the library:

![Primitives Demo](/docs/primitives_demo.png)

This image shows a plane (green), sphere (blue), cylinder (red), cone (gold), torus (purple), and icosahedron (cyan) arranged in a scene.

## glTF 2.0 Specification Coverage

The following table shows the current coverage of the [glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html) features in this library:

| Feature Category | Feature | Support Level | Notes |
|-----------------|---------|---------------|-------|
| **Core** |||||
| | Asset Information | ✅ Full | `version` 2.0, `generator` (settable or omitted), `copyright`, `minVersion`, and asset `extensions`/`extras`, with validation; see docs/EXTENSIONS.md |
| | Scenes | ⚠️ Partial | Multiple scenes; the first scene added becomes the default (no API to change it) |
| | Nodes | ⚠️ Partial | Hierarchies and TRS transforms; morph target `weights` overrides; the builder never sets `matrix`; no camera |
| | Buffers | ⚠️ Partial | One embedded buffer (created on first data write) exported as the GLB BIN chunk; no external `.bin` or data-URI export |
| | Buffer Views | ⚠️ Partial | `target` and `byteStride`: `add_data_view` for raw views, `add_interleaved_attributes` for interleaved vertex data (4-byte aligned elements, stride up to 252); validated. Single buffer only (`buffer` is always 0) |
| | Accessors | ✅ Full | Typed writer for every component type (`i8`..`f32`) and accessor type (SCALAR..MAT4, padded columns), `normalized`, computed or explicit `min`/`max` (exact for integers), `byteOffset` into shared views, zero accessors (no `bufferView`) and sparse accessors (`u8`/`u16`/`u32` indices, optional zero base); validated; see docs/ACCESSORS.md. extras/extensions: see the Custom Extensions row |
| **Geometry** |||||
| | Meshes | ✅ Full | Multiple primitives per mesh (`add_mesh_checked`), `PrimitiveSpec`/`create_primitive` with positions, normals, tangents, `TEXCOORD_n` (`f32`, normalized `u8`/`u16`), `COLOR_n` (RGB/RGBA, `f32`, normalized `u8`/`u16`), skin sets, `u8`/`u16`/`u32` indices, material, mode and extras; morph targets and default weights; mesh and primitive extras; validated; see docs/MESHES.md. No `KHR_mesh_quantization` attribute types |
| | Primitive Types | ✅ Full | All seven modes through `PrimitiveMode` and `create_points`/`create_lines`/`create_line_strip`/`create_triangle_strip`/`create_triangle_fan` (or `PrimitiveSpec.mode`), per-mode count rules, `strip_to_triangles`/`fan_to_triangles` helpers; validated; see docs/PRIMITIVE_MODES.md. The built-in shape generators still emit triangles |
| | Morph Targets | ⚠️ Partial | `POSITION` and `NORMAL` deltas, default and per-node weights, target names; no tangent deltas, no sparse accessors |
| **Materials** |||||
| | PBR Materials | ✅ Full | Metallic-roughness model: base color, metallic, roughness, emissive, normal and occlusion textures |
| | Alpha Modes | ✅ Full | Opaque, mask, blend modes |
| | Double Sided | ✅ Full | Flag for double-sided rendering |
| | Material Variations | ⚠️ Partial | Metallic-roughness and specular-glossiness (KHR_materials_pbrSpecularGlossiness); texture `texCoord` is fixed to 0 |
| **Textures** |||||
| | Samplers | ✅ Full | Typed `MagFilter`/`MinFilter`/`WrapMode` and `SamplerDesc` (all 108 combinations tested); `add_sampler`, `get_or_add_sampler` (dedup), one shared default sampler; `SAMPLER_*` and `TEXTURE_*` validation |
| | Images | ⚠️ Partial | Embedded PNG/JPEG (`add_image`, `add_image_bytes`, `add_image_from_file`, magic-number and header checks) and external `uri` (`add_image_uri`); `data:` URIs are not supported yet, and WebP/KTX2 are out of scope |
| | Texture Coordinates | ⚠️ Partial | Multiple `TEXCOORD_n` sets on meshes; specular-glossiness textures use set 0 |
| **Animation** |||||
| | Animation | ⚠️ Partial | Translation, rotation, scale and morph target weights tracks with LINEAR, STEP and CUBICSPLINE interpolation |
| | Skinning | ⚠️ Partial | Skins, computed inverse bind matrices, `JOINTS_n`/`WEIGHTS_n` (u8/u16 joints, f32 weights); no normalized integer weights |
| **Cameras** |||||
| | Cameras | ❌ None | Not yet implemented |
| **Extensions** |||||
| | KHR_materials_pbrSpecularGlossiness | ⚠️ Partial | Specular-glossiness workflow; texture `texCoord` is fixed to 0 |
| | KHR_materials_unlit | ❌ None | Not yet implemented |
| | KHR_texture_transform | ❌ None | Not yet implemented |
| | KHR_mesh_quantization | ❌ None | Not yet implemented |
| | Custom Extensions | ⚠️ Partial | Opaque-JSON `extensions` and `extras` through `set_extension`/`set_extras` on the root, asset, scenes, nodes, meshes, primitives, materials, textures, images, samplers, accessors, buffer views, buffers, skins and animations (with channels and samplers), plus `extensionsUsed`/`extensionsRequired` bookkeeping. Not yet reachable: texture-info structs, camera and sparse objects (not modelled) |

### Legend

- ✅ **Full**: Complete implementation according to the spec
- ⚠️ **Partial**: Basic functionality implemented with some limitations
- ❌ **None**: Feature not implemented yet

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
