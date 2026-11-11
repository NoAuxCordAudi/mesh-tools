# Scenes

A scene lists root nodes. A document may hold several scenes and names one as the default.

```rust
use mesh_tools::GltfBuilder;

let mut b = GltfBuilder::new();
let mesh = b.create_box(1.0);
let a = b.add_node(Some("a".into()), Some(mesh), None, None, None);
let c = b.add_node(Some("c".into()), Some(mesh), None, None, None);

// Checked: non-empty, existing, unique, root nodes only. The first scene becomes the default.
let main = b.add_scene_checked(Some("main".into()), vec![a]).unwrap();
let alt = b.add_scene_checked(Some("alt".into()), vec![c]).unwrap();
b.add_node_to_scene(alt, a).unwrap(); // a node may be a root of several scenes

b.set_default_scene(alt).unwrap();
assert_eq!(b.gltf.scene, Some(alt));
assert!(b.set_default_scene(9).is_err());

b.clear_default_scene(); // the property is omitted; export still works
assert!(b.validate().has_warning("NO_DEFAULT_SCENE"));
assert!(b.to_glb_bytes().is_ok());
# let _ = main;
```

## Rules

- `add_scene` is unchanged and unchecked; it still makes the first scene the default. Later
  scenes never reselect, so `clear_default_scene` stays in effect.
- Scene `extras` and `extensions`: `set_extras(ExtensionTarget::Scene(i), ..)`, see EXTENSIONS.md.

## Validator codes

| Code | Severity | Meaning |
|---|---|---|
| `SCENE_DEFAULT_INVALID` | error | the default scene does not exist |
| `SCENE_NODE_INVALID` | error | a scene lists a missing node |
| `SCENE_ROOT_HAS_PARENT` | error | a scene lists a node that is another node's child |
| `SCENE_NODES_EMPTY` | error | `nodes` is present but empty (the spec requires at least one) |
| `SCENE_NODE_DUPLICATE` | error | a scene lists a node twice |
| `NO_DEFAULT_SCENE` | warning | scenes exist but none is the default |
