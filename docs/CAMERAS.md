# Cameras

A camera is a projection (perspective or orthographic). It only takes a position and
orientation when a node references it, so the node's transform places the camera and the camera
looks down the node's local -Z axis with +Y up.

```rust
use mesh_tools::GltfBuilder;

let mut b = GltfBuilder::new();

// Perspective: yfov (radians), znear, optional zfar (None = infinite far plane), optional aspect ratio.
let persp = b.add_perspective_camera(Some("Main".into()), 0.8, 0.1, Some(100.0), Some(1.5)).unwrap();
// Orthographic: xmag, ymag (half extents), znear, zfar.
let ortho = b.add_orthographic_camera(None, 2.0, 1.0, 0.0, 50.0).unwrap();

// Place a camera by attaching it to a node.
let node = b.add_node(None, None, Some([0.0, 0.0, 5.0]), None, None);
b.set_node_camera(node, persp).unwrap();

// Replacing a camera is explicit: set_node_camera refuses a node that already has one.
assert!(b.set_node_camera(node, ortho).is_err());
b.clear_node_camera(node).unwrap();
b.set_node_camera(node, ortho).unwrap();

// Invalid values are rejected and leave the document unchanged.
assert!(b.add_perspective_camera(None, 0.8, 1.0, Some(0.5), None).is_err()); // zfar <= znear
assert!(b.add_orthographic_camera(None, 0.0, 1.0, 0.0, 1.0).is_err()); // xmag is zero
assert_eq!(b.gltf.cameras.as_ref().unwrap().len(), 2);

b.add_scene(None, Some(vec![node]));
assert!(b.validate().warnings.iter().any(|w| w.starts_with("CAMERA_UNUSED"))); // camera 0 has no node
```

## Extras and extensions

`ExtensionTarget::Camera(i)`, `CameraPerspective(i)` and `CameraOrthographic(i)` address the
camera and its projection object for `set_extras` and `set_extension`. The projection targets
fail if the camera has the other projection.

```rust
use mesh_tools::{ExtensionTarget, GltfBuilder};
use serde_json::json;

let mut b = GltfBuilder::new();
let c = b.add_orthographic_camera(None, 1.0, 1.0, 0.1, 10.0).unwrap();
b.set_extras(ExtensionTarget::Camera(c), json!({ "label": "top" })).unwrap();
b.set_extras(ExtensionTarget::CameraOrthographic(c), json!(1)).unwrap();
assert!(b.set_extras(ExtensionTarget::CameraPerspective(c), json!(1)).is_err());
```

## Rules

- Perspective: `yfov` > 0, `znear` > 0, `aspectRatio` (if present) > 0, `zfar` (if present)
  > `znear`. A `yfov` of pi or more is legal but warned about.
- Orthographic: `xmag` and `ymag` non-zero, `znear` >= 0, `zfar` > `znear`. A negative
  magnitude mirrors the view and is warned about. (The spec schema says `zfar` > 0; the extra
  `zfar` > `znear` rule is in the spec text and the Khronos validator.)
- All values must be finite. The builder rejects invalid values; the validator catches
  hand-edited documents (`gltf.cameras`, `Camera::type_` and so on).
- A node may have a camera and a mesh at the same time. No check is made for scaled or sheared
  camera nodes (the spec says the node's global transform should not be scaled; this crate does not
  validate that).
- Unused cameras are legal and warned about (`CAMERA_UNUSED`).

## Validator codes

| Code | Severity | Meaning |
|---|---|---|
| `CAMERA_TYPE_INVALID` | error | `type` is not `perspective` or `orthographic` |
| `CAMERA_TYPE_MISMATCH` | error | the projection object for `type` is missing, or the other one is present |
| `CAMERA_YFOV_INVALID` | error | `yfov` not finite or <= 0 |
| `CAMERA_ASPECT_RATIO_INVALID` | error | `aspectRatio` not finite or <= 0 |
| `CAMERA_ZNEAR_INVALID` | error | `znear` not finite, or <= 0 (perspective) / < 0 (orthographic) |
| `CAMERA_ZFAR_INVALID` | error | `zfar` not finite, <= 0 or <= `znear` |
| `CAMERA_XMAG_INVALID`, `CAMERA_YMAG_INVALID` | error | magnitude not finite or zero |
| `NODE_CAMERA_INVALID` | error | node references a camera that does not exist |
| `CAMERA_YFOV_TOO_LARGE` | warning | `yfov` >= pi |
| `CAMERA_MAG_NEGATIVE` | warning | negative `xmag` or `ymag` |
| `CAMERA_UNUSED` | warning | no node references the camera |
