# Accessors

Every accessor the crate writes goes through one writer. Use the typed methods below for your
own data.

- `add_accessor_from_slice`: a new view plus accessor from a typed slice (`i8`, `u8`, `i16`,
  `u16`, `u32`, `f32`), with optional normalization and bounds.
- `add_accessor_in_view`: an accessor over existing bytes of a view (see `add_data_view`).
- `add_zero_accessor`: an accessor without a `bufferView`; it reads as all zeros.
- `add_sparse_accessor`: a base array with a few elements replaced (below).

Bounds (`min`/`max`) are always in the raw component domain, also for normalized data.

## Sparse accessors

A sparse accessor stores a base array (or zeros, when it has no `bufferView`) and a short list
of replaced elements: strictly increasing indices plus one value per index. It is the compact
form for morph target deltas, where most vertices do not move. Readers see the substituted
array, and so do `Bounds::Compute` and `Bounds::Explicit`.

```rust
use mesh_tools::{AccessorSpec, AccessorType, Bounds, GltfBuilder, SparseSpec};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut builder = GltfBuilder::new();

// Five VEC3 positions; elements 1 and 3 are replaced.
let base: Vec<f32> = (0..15).map(|v| v as f32).collect();
let sparse = SparseSpec::new(vec![1, 3], vec![-1.0, -2.0, -3.0, 9.0, 9.0, 9.0]);
let spec = AccessorSpec::new().with_bounds(Bounds::Compute);
let a = builder.add_sparse_accessor(5, AccessorType::Vec3, Some(&base), sparse, spec)?;

let acc = &builder.gltf.accessors.as_ref().unwrap()[a];
assert_eq!(acc.min, Some(vec![-1.0, -2.0, -3.0]));
assert_eq!(acc.max, Some(vec![12.0, 13.0, 14.0]));
assert!(builder.validate().is_valid());

// Without a base the accessor has no bufferView and reads as zeros except at the indices.
let delta = SparseSpec::new(vec![2], vec![0.5f32, 0.0, 0.0]);
let z = builder.add_sparse_accessor(4, AccessorType::Vec3, None, delta, AccessorSpec::new())?;
assert!(builder.gltf.accessors.as_ref().unwrap()[z].buffer_view.is_none());

// Indices must be strictly increasing and below the element count; errors leave the builder as is.
let bad = SparseSpec::new(vec![2, 2], vec![1.0f32, 1.0]);
assert!(builder.add_sparse_accessor(4, AccessorType::Scalar, None, bad, AccessorSpec::new()).is_err());
# Ok(())
# }
```

The index width (`u8`, `u16` or `u32`) is the smallest that fits the last index. Indices and
values get their own views, without `target` or `byteStride`. Matrix values are padded per
column like any other accessor data.

## Validator codes

`validate()` checks `accessor.sparse` and the substituted data (bounds, NaN/infinity):

| Code | Meaning |
|---|---|
| `ACCESSOR_SPARSE_COUNT` | `count` is 0 or exceeds the accessor's `count` |
| `ACCESSOR_SPARSE_INDICES_TYPE` | index component type is not 5121, 5123 or 5125 |
| `ACCESSOR_SPARSE_INDICES_ORDER` | indices are not strictly increasing |
| `ACCESSOR_SPARSE_INDEX_RANGE` | an index is not below the accessor's `count` |
| `ACCESSOR_SPARSE_VIEW_INVALID` | indices or values view is missing, or has a `target` or `byteStride` |
| `ACCESSOR_SPARSE_RANGE` | indices or values do not fit in their view |
| `ACCESSOR_OFFSET_ALIGNMENT` | also used for a misaligned sparse `byteOffset` |
