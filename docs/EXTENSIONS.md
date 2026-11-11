# Extensions, extras and asset metadata

glTF lets every object carry `extensions` (payloads keyed by extension name) and `extras`
(any JSON). mesh-tools treats both as **opaque JSON**: it writes what you give it and checks
the bookkeeping, but it does not interpret or validate payloads.

## Setting extensions and extras

`ExtensionTarget` names the object. Indices are checked when you call.

```rust
use mesh_tools::{ExtensionTarget, GltfBuilder};
use serde_json::json;

let mut b = GltfBuilder::new();
let mesh = b.create_box(1.0);
let node = b.add_node(None, Some(mesh), None, None, None);
b.add_scene(None, Some(vec![node]));

// Extension payloads must be JSON objects. The name is added to `extensionsUsed`.
b.set_extension(ExtensionTarget::Node(node), "VENDOR_label", json!({ "text": "hi" })).unwrap();

// Extras may be any JSON value, here on the document root.
b.set_extras(ExtensionTarget::Root, json!([1, 2, 3])).unwrap();

assert!(b.validate().is_valid());

// Errors leave the document unchanged.
assert!(b.set_extension(ExtensionTarget::Node(42), "VENDOR_label", json!({})).is_err());
assert!(b.set_extension(ExtensionTarget::Node(node), "VENDOR_label", json!("not an object")).is_err());
```

Targets: `Root`, `Asset`, `Scene`, `Node`, `Mesh`, `Primitive { mesh, primitive }`, `Material`,
`PbrMetallicRoughness`, `Texture`, `Image`, `Sampler`, `Accessor`, `BufferView`, `Buffer`,
`Skin`, `Animation`, `AnimationChannel { animation, channel }` and
`AnimationSampler { animation, sampler }`. Not yet addressable: texture-info structs
(`baseColorTexture` ...), the animation channel `target`, and objects the crate does not model
(cameras, sparse accessors).

On a material, `KHR_materials_pbrSpecularGlossiness` (deprecated upstream) is stored in the
typed struct, so its payload must match that schema; every other name is kept verbatim.

## `extensionsUsed` and `extensionsRequired`

- `set_extension` adds the name to `extensionsUsed` (idempotent).
- `declare_extension_used` does the same without a payload, for extensions that have none, such
  as `KHR_mesh_quantization`.
- `require_extension` adds to `extensionsRequired` and `extensionsUsed`.

```rust
use mesh_tools::GltfBuilder;

let mut b = GltfBuilder::new();
b.require_extension("KHR_draco_mesh_compression").unwrap();
assert_eq!(b.gltf.extensions_required, b.gltf.extensions_used);
assert!(b.require_extension("").is_err());
```

An empty `extensionsUsed` / `extensionsRequired` array is an error on export (the spec forbids
it); it is never silently dropped. The `_unchecked` exports keep it in the output.

## Mesh extras and morph target names

Morph target names live in `mesh.extras.targetNames`. `set_extras` on a mesh keeps them and
merges your object with them (a `targetNames` key in your value, or a non-object value, is an
error); `set_mesh_target_names` sets them on an existing mesh and keeps your other keys.

```rust
use mesh_tools::{ExtensionTarget, GltfBuilder};
use serde_json::json;

let mut b = GltfBuilder::new();
let mesh = b.create_box(1.0); // no morph targets
b.set_extras(ExtensionTarget::Mesh(mesh), json!({ "author": "me" })).unwrap();
assert!(b.set_mesh_target_names(mesh, vec!["smile".into()]).is_err());
```

## Asset metadata

```rust
use mesh_tools::GltfBuilder;

let mut b = GltfBuilder::new();
b.set_copyright("(c) Example").unwrap();
b.set_generator("my tool 1.0").unwrap(); // default: "Rust glTF Export Library"
b.set_min_version("2.0").unwrap();       // <major>.<minor>, not above the asset version
assert!(b.set_copyright("").is_err());
assert!(b.set_min_version("2.1").is_err());
b.clear_generator();                      // omit the generator
```

## Validator codes

| Code | Severity | Meaning |
|---|---|---|
| `ASSET_VERSION_INVALID` | error | `asset.version` is not `<major>.<minor>` |
| `ASSET_MIN_VERSION_INVALID` | error | `minVersion` is not `<major>.<minor>` |
| `ASSET_MIN_VERSION_EXCEEDS_VERSION` | error | `minVersion` is greater than `version` |
| `ASSET_COPYRIGHT_EMPTY` / `ASSET_GENERATOR_EMPTY` | error | empty string |
| `EXTENSION_VALUE_NOT_OBJECT` | error | an `extensions` entry is not a JSON object |
| `EXTENSION_NOT_DECLARED` | error | an extension is used but not in `extensionsUsed` |
| `EXTENSIONS_USED_DUPLICATE` | error | a name appears twice in `extensionsUsed` or `extensionsRequired` |
| `EXTENSIONS_USED_EMPTY` / `EXTENSIONS_REQUIRED_EMPTY` | error | empty array |
| `EXTENSIONS_REQUIRED_NOT_USED` | error | required name missing from `extensionsUsed` |
| `EXTENSION_NAME_INVALID` | warning | name is not `VENDOR_name` (upper-case vendor prefix) |
| `EXTENSION_DECLARED_UNUSED` | warning | declared but no object carries it (expected for extensions without a payload) |

`extras` subtrees are application data: an `"extensions"` key inside `extras` is not searched.
`serde_json::Value` cannot hold NaN or infinity, so there is no non-finite check for extras.
