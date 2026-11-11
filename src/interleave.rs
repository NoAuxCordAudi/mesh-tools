//! Interleaved vertex attributes: several attributes sharing one `ARRAY_BUFFER` view with a
//! `byteStride`.

use crate::accessor::*;
use crate::builder::GltfBuilder;
use crate::builder_accessor::{check_explicit, encode_elements};
use crate::error::{GltfError, Result};
use crate::models::Accessor;

fn invalid(msg: String) -> GltfError {
    GltfError::InvalidInput(msg)
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

/// One vertex attribute to interleave; see [`GltfBuilder::add_interleaved_attributes`].
///
/// Built with [`new`](Self::new), which validates the data, so a value of this type is always
/// writable.
#[derive(Debug, Clone, PartialEq)]
pub struct InterleavedAttribute {
    semantic: String,
    ty: AccessorType,
    component: ComponentType,
    normalized: bool,
    count: usize,
    /// Tightly packed elements (matrix columns padded), `elem_size` bytes each.
    bytes: Vec<u8>,
    elem_size: usize,
    bounds: Option<(Vec<f64>, Vec<f64>)>,
}

impl InterleavedAttribute {
    /// An attribute named `semantic` (`"POSITION"`, `"TEXCOORD_0"`, `"_CUSTOM"`, ...) from tightly
    /// packed `data` (`ty.components()` values per vertex).
    ///
    /// `bounds` follows [`Bounds`]: `POSITION` needs `Bounds::Compute` or `Explicit`.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if `semantic` is empty, `data` is empty or not a whole number of
    /// elements, a float is NaN or infinite, `normalized` is set for `f32` / `u32`, or explicit
    /// bounds are malformed or differ from the data.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::{AccessorType, Bounds, InterleavedAttribute};
    /// let a = InterleavedAttribute::new("POSITION", &[0.0f32, 0.0, 0.0], AccessorType::Vec3, false, Bounds::Compute);
    /// assert!(a.is_ok());
    /// assert!(InterleavedAttribute::new("POSITION", &[0.0f32, 0.0], AccessorType::Vec3, false, Bounds::None).is_err());
    /// ```
    pub fn new<C: Component>(semantic: impl Into<String>, data: &[C], ty: AccessorType, normalized: bool, bounds: Bounds) -> Result<Self> {
        let semantic = semantic.into();
        if semantic.is_empty() {
            return Err(invalid("attribute semantic is empty".to_string()));
        }
        let comps = ty.components();
        if data.is_empty() || data.len() % comps != 0 {
            return Err(invalid(format!(
                "attribute {semantic}: {} values are not a non-empty whole number of {} elements", data.len(), ty.as_str()
            )));
        }
        if normalized && !C::TYPE.can_normalize() {
            return Err(invalid(format!("attribute {semantic}: normalized is not allowed for {:?}", C::TYPE)));
        }
        if C::TYPE == ComponentType::F32 && data.iter().any(|v| !v.to_f64().is_finite()) {
            return Err(invalid(format!("attribute {semantic}: data contains NaN or infinity")));
        }
        let actual = bounds_of(data.chunks_exact(comps).map(|e| e.iter().map(|v| v.to_f64()).collect()));
        let bounds = match bounds {
            Bounds::None => None,
            Bounds::Compute => actual,
            Bounds::Explicit { min, max } => {
                check_explicit(&min, &max, comps, C::TYPE, actual.as_ref()).map_err(|e| invalid(format!("attribute {semantic}: {e}")))?;
                Some((min, max))
            }
        };
        let bytes = encode_elements(data, ty);
        let count = data.len() / comps;
        Ok(Self { semantic, ty, component: C::TYPE, normalized, count, elem_size: bytes.len() / count, bytes, bounds })
    }
}

impl GltfBuilder {
    /// Write `attributes` interleaved in one `ARRAY_BUFFER` view and return `(semantic, accessor
    /// index)` per attribute, in input order.
    ///
    /// Each element starts on a 4-byte boundary and the `byteStride` is the packed size rounded
    /// up to a multiple of 4, as glTF requires. Pass the accessors to a primitive's attributes.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if `attributes` is empty, `vertex_count` is 0 or differs from an
    /// attribute's element count, a semantic is used twice, or the stride would exceed 252 (the
    /// glTF maximum). The document is unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::{AccessorType, Bounds, GltfBuilder, InterleavedAttribute as A};
    /// let mut b = GltfBuilder::new();
    /// let pos = A::new("POSITION", &[0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0], AccessorType::Vec3, false, Bounds::Compute).unwrap();
    /// let uv = A::new("TEXCOORD_0", &[0.0f32, 0.0, 1.0, 0.0], AccessorType::Vec2, false, Bounds::None).unwrap();
    /// let made = b.add_interleaved_attributes(2, &[pos, uv]).unwrap();
    /// assert_eq!(made[1].0, "TEXCOORD_0");
    /// // 12 + 8 bytes per vertex.
    /// assert_eq!(b.gltf.buffer_views.as_ref().unwrap()[0].byte_stride, Some(20));
    /// ```
    pub fn add_interleaved_attributes(&mut self, vertex_count: usize, attributes: &[InterleavedAttribute]) -> Result<Vec<(String, usize)>> {
        if attributes.is_empty() {
            return Err(invalid("at least one attribute is required".to_string()));
        }
        if vertex_count == 0 {
            return Err(invalid("vertex count must be at least 1".to_string()));
        }
        let mut offsets = Vec::with_capacity(attributes.len());
        let mut end = 0;
        for (i, a) in attributes.iter().enumerate() {
            if a.count != vertex_count {
                return Err(invalid(format!("attribute {} has {} elements, expected the vertex count {vertex_count}", a.semantic, a.count)));
            }
            if attributes[..i].iter().any(|p| p.semantic == a.semantic) {
                return Err(invalid(format!("duplicate attribute {}", a.semantic)));
            }
            let at = align4(end);
            offsets.push(at);
            end = at + a.elem_size;
        }
        let stride = align4(end);
        if stride > 252 {
            return Err(invalid(format!("interleaved stride {stride} exceeds the glTF maximum of 252 bytes")));
        }
        let mut data = vec![0u8; stride * vertex_count];
        for (a, &at) in attributes.iter().zip(&offsets) {
            for (v, elem) in a.bytes.chunks_exact(a.elem_size).enumerate() {
                data[v * stride + at..v * stride + at + a.elem_size].copy_from_slice(elem);
            }
        }
        // Everything is validated; the writes below cannot fail.
        let view = self.add_data_view(&data, Some(BufferViewTarget::ArrayBuffer), Some(stride))?;
        Ok(attributes
            .iter()
            .zip(offsets)
            .map(|(a, at)| {
                let index = self.push_accessor(Accessor {
                    buffer_view: Some(view),
                    byte_offset: (at != 0).then_some(at),
                    component_type: a.component.code(),
                    count: vertex_count,
                    type_: a.ty.as_str().to_string(),
                    min: a.bounds.as_ref().map(|b| b.0.clone()),
                    max: a.bounds.as_ref().map(|b| b.1.clone()),
                    normalized: a.normalized.then_some(true),
                    ..Default::default()
                });
                (a.semantic.clone(), index)
            })
            .collect())
    }
}
