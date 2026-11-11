//! The single accessor writer. Every accessor the crate writes (primitives, skins, morph
//! targets, animation) goes through [`GltfBuilder::write_slice`] / `push_accessor`; the public
//! methods add validation in front of it. Public methods validate everything before touching the
//! document, so an `Err` leaves it unchanged.

use crate::accessor::*;
use crate::builder::GltfBuilder;
use crate::error::{GltfError, Result};
use crate::models::Accessor;

fn invalid(msg: String) -> GltfError {
    GltfError::InvalidInput(msg)
}

fn check_normalized(normalized: bool, component: ComponentType) -> Result<()> {
    if normalized && !component.can_normalize() {
        return Err(invalid(format!("normalized is not allowed for {component:?} components")));
    }
    Ok(())
}

/// Reject explicit bounds that are malformed or differ from the data's `actual` bounds.
pub(crate) fn check_explicit(min: &[f64], max: &[f64], comps: usize, component: ComponentType, actual: Option<&(Vec<f64>, Vec<f64>)>) -> Result<()> {
    if min.len() != comps || max.len() != comps {
        return Err(invalid(format!(
            "bounds length is {} (min) / {} (max), expected {comps}, one per component", min.len(), max.len()
        )));
    }
    if let Some(i) = min.iter().chain(max).position(|v| !v.is_finite()) {
        return Err(invalid(format!("bounds value {i} is not finite")));
    }
    if let Some(k) = (0..comps).find(|&k| min[k] > max[k]) {
        return Err(invalid(format!("min[{k}] = {} exceeds max[{k}] = {}", min[k], max[k])));
    }
    let same = |a: f64, b: f64| if component == ComponentType::F32 { a as f32 == b as f32 } else { a == b };
    match actual {
        Some((lo, hi)) if (0..comps).all(|k| same(min[k], lo[k]) && same(max[k], hi[k])) => Ok(()),
        _ => Err(invalid("explicit bounds do not match the data".to_string())),
    }
}

/// Little-endian bytes of tightly packed `data`, with matrix columns padded to 4 bytes.
pub(crate) fn encode_elements<C: Component>(data: &[C], ty: AccessorType) -> Vec<u8> {
    let (cols, rows) = ty.shape();
    let mut bytes = Vec::with_capacity(data.len() * C::TYPE.size());
    if cols == 1 {
        data.iter().for_each(|v| v.push_le(&mut bytes));
    } else {
        for column in data.chunks_exact(rows) {
            let start = bytes.len();
            column.iter().for_each(|v| v.push_le(&mut bytes));
            while (bytes.len() - start) % 4 != 0 {
                bytes.push(0);
            }
        }
    }
    bytes
}

impl GltfBuilder {
    /// Append `accessor` and return its index.
    pub(crate) fn push_accessor(&mut self, accessor: Accessor) -> usize {
        let accessors = self.gltf.accessors.get_or_insert_with(Vec::new);
        accessors.push(accessor);
        accessors.len() - 1
    }

    /// Unchecked writer behind every accessor: pads matrix columns, writes a new buffer view
    /// (with `spec.target`) and the accessor. `data` must hold a whole number of elements.
    pub(crate) fn write_slice<C: Component>(&mut self, data: &[C], ty: AccessorType, spec: &AccessorSpec) -> usize {
        let comps = ty.components();
        let bytes = encode_elements(data, ty);
        let bounds = match &spec.bounds {
            Bounds::None => None,
            Bounds::Compute => bounds_of(data.chunks_exact(comps).map(|e| e.iter().map(|v| v.to_f64()).collect())),
            Bounds::Explicit { min, max } => Some((min.clone(), max.clone())),
        };
        let (offset, length) = self.add_buffer_data(&bytes);
        let view = self.add_buffer_view(offset, length, spec.target.map(BufferViewTarget::code));
        self.push_accessor(Accessor {
            name: spec.name.clone(),
            buffer_view: Some(view),
            component_type: C::TYPE.code(),
            count: data.len() / comps,
            type_: ty.as_str().to_string(),
            min: bounds.as_ref().map(|b| b.0.clone()),
            max: bounds.map(|b| b.1),
            normalized: spec.normalized.then_some(true),
            ..Default::default()
        })
    }

    /// Write `data` as a new buffer view plus accessor and return the accessor index.
    ///
    /// `data` is tightly packed: `ty.components()` values per element (a MAT3 is 9 values).
    /// Matrix columns are padded to 4 bytes in the file as glTF requires, so a MAT3 of `i8` takes
    /// 12 bytes per element. Values are written little-endian. Bounds follow `spec.bounds` and are
    /// in the raw component domain, also for normalized data.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if `data` is empty or not a whole number of elements, a float is
    /// NaN or infinite, `spec.normalized` is set for `f32` or `u32`, or explicit bounds have the
    /// wrong length, `min > max`, or differ from the data. The document is unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::{AccessorSpec, AccessorType, Bounds, GltfBuilder};
    /// let mut b = GltfBuilder::new();
    /// // Normalized signed bytes: -127..127 read as -1..1.
    /// let spec = AccessorSpec::new().with_normalized(true).with_bounds(Bounds::Compute);
    /// let a = b.add_accessor_from_slice(&[-127i8, 0, 127], AccessorType::Vec3, spec).unwrap();
    /// assert_eq!(b.gltf.accessors.as_ref().unwrap()[a].min, Some(vec![-127.0, 0.0, 127.0]));
    /// // NaN is rejected.
    /// assert!(b.add_accessor_from_slice(&[f32::NAN], AccessorType::Scalar, AccessorSpec::new()).is_err());
    /// ```
    pub fn add_accessor_from_slice<C: Component>(&mut self, data: &[C], ty: AccessorType, spec: AccessorSpec) -> Result<usize> {
        let comps = ty.components();
        check_normalized(spec.normalized, C::TYPE)?;
        if data.is_empty() {
            return Err(invalid("accessor data is empty".to_string()));
        }
        if data.len() % comps != 0 {
            return Err(invalid(format!(
                "{} values is not a multiple of {comps} components per {} element", data.len(), ty.as_str()
            )));
        }
        if C::TYPE == ComponentType::F32 {
            if let Some(i) = data.iter().position(|v| !v.to_f64().is_finite()) {
                return Err(invalid(format!("value {i} is not finite")));
            }
        }
        if let Bounds::Explicit { min, max } = &spec.bounds {
            let actual = bounds_of(data.chunks_exact(comps).map(|e| e.iter().map(|v| v.to_f64()).collect()));
            check_explicit(min, max, comps, C::TYPE, actual.as_ref())?;
        }
        Ok(self.write_slice(data, ty, &spec))
    }

    /// Add an accessor with no `bufferView`: `count` elements that all read as zero.
    ///
    /// Nothing is written to the buffer. This is the spec's way to say "all zeros" (for example
    /// a morph target delta). The accessor has no bounds.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if `count` is 0, or `normalized` is set for `F32` / `U32`.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::{AccessorType, ComponentType, GltfBuilder};
    /// let mut b = GltfBuilder::new();
    /// let a = b.add_zero_accessor(4, AccessorType::Vec3, ComponentType::F32, false).unwrap();
    /// assert!(b.gltf.accessors.as_ref().unwrap()[a].buffer_view.is_none());
    /// assert!(b.add_zero_accessor(0, AccessorType::Vec3, ComponentType::F32, false).is_err());
    /// ```
    pub fn add_zero_accessor(&mut self, count: usize, ty: AccessorType, component: ComponentType, normalized: bool) -> Result<usize> {
        if count == 0 {
            return Err(invalid("accessor count must be at least 1".to_string()));
        }
        check_normalized(normalized, component)?;
        Ok(self.push_accessor(Accessor {
            component_type: component.code(),
            count,
            type_: ty.as_str().to_string(),
            normalized: normalized.then_some(true),
            ..Default::default()
        }))
    }

    /// Add `data` to the buffer as a new buffer view and return the view index.
    ///
    /// Use it with [`add_accessor_in_view`](Self::add_accessor_in_view) to put several
    /// accessors in one view. `byte_stride` makes the view interleaved: it must be a multiple of
    /// 4 in `4..=252` and is only allowed on `ArrayBuffer` or untargeted views.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if `data` is empty or the stride is invalid. The document is
    /// unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::{BufferViewTarget, GltfBuilder};
    /// let mut b = GltfBuilder::new();
    /// let v = b.add_data_view(&[0u8; 24], Some(BufferViewTarget::ArrayBuffer), Some(12)).unwrap();
    /// assert_eq!(v, 0);
    /// assert!(b.add_data_view(&[0u8; 24], None, Some(6)).is_err()); // stride not a multiple of 4
    /// ```
    pub fn add_data_view(&mut self, data: &[u8], target: Option<BufferViewTarget>, byte_stride: Option<usize>) -> Result<usize> {
        if data.is_empty() {
            return Err(invalid("buffer view data is empty".to_string()));
        }
        if let Some(s) = byte_stride {
            if !(4..=252).contains(&s) || s % 4 != 0 {
                return Err(invalid(format!("byte stride {s} must be a multiple of 4 in 4..=252")));
            }
            if target == Some(BufferViewTarget::ElementArrayBuffer) {
                return Err(invalid("byte stride is not allowed on an ELEMENT_ARRAY_BUFFER view".to_string()));
            }
        }
        let (offset, length) = self.add_buffer_data(data);
        let view = self.add_buffer_view(offset, length, target.map(BufferViewTarget::code));
        self.gltf.buffer_views.as_mut().expect("view was just added")[view].byte_stride = byte_stride;
        Ok(view)
    }

    /// Add an accessor over existing bytes in buffer view `view`.
    ///
    /// `byte_offset` is relative to the start of the view and must be a multiple of the
    /// component size (as must the view's offset plus it). Elements are `byteStride` apart if the
    /// view has one, else tightly packed with padded matrix columns. [`Bounds::Compute`] reads
    /// the bytes; `Explicit` must equal them.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the view does not exist or does not hold the data (range),
    /// `count` is 0, the offset is misaligned, the view's stride is smaller than an element,
    /// `normalized` is set for `F32` / `U32`, or the bounds are malformed or wrong. The
    /// document is unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::{AccessorType, Bounds, ComponentType, GltfBuilder};
    /// let mut b = GltfBuilder::new();
    /// let bytes: Vec<u8> = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0].iter().flat_map(|f| f.to_le_bytes()).collect();
    /// let v = b.add_data_view(&bytes, None, None).unwrap();
    /// // The second VEC3 starts at byte 12.
    /// let a = b.add_accessor_in_view(v, 12, 1, AccessorType::Vec3, ComponentType::F32, false, Bounds::Compute).unwrap();
    /// assert_eq!(b.gltf.accessors.as_ref().unwrap()[a].max, Some(vec![4.0, 5.0, 6.0]));
    /// // Three VEC3 do not fit in a 24-byte view.
    /// assert!(b.add_accessor_in_view(v, 0, 3, AccessorType::Vec3, ComponentType::F32, false, Bounds::None).is_err());
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub fn add_accessor_in_view(
        &mut self,
        view: usize,
        byte_offset: usize,
        count: usize,
        ty: AccessorType,
        component: ComponentType,
        normalized: bool,
        bounds: Bounds,
    ) -> Result<usize> {
        let n_views = self.gltf.buffer_views.as_ref().map_or(0, |v| v.len());
        let v = self
            .gltf
            .buffer_views
            .as_ref()
            .and_then(|v| v.get(view))
            .ok_or_else(|| invalid(format!("buffer view {view} does not exist ({n_views} present)")))?;
        if count == 0 {
            return Err(invalid("accessor count must be at least 1".to_string()));
        }
        check_normalized(normalized, component)?;
        let comp = component.size();
        let elem = element_size(component.code(), ty.as_str()).expect("typed enums always have a size");
        if byte_offset % comp != 0 || v.byte_offset.checked_add(byte_offset).map_or(true, |abs| abs % comp != 0) {
            return Err(invalid(format!(
                "byte offset {byte_offset} (view offset {}) is not aligned to the {comp}-byte component", v.byte_offset
            )));
        }
        let stride = v.byte_stride.unwrap_or(elem);
        if stride < elem {
            return Err(invalid(format!("the view's byte stride {stride} is smaller than the {elem}-byte element")));
        }
        let end = stride.checked_mul(count - 1).and_then(|s| s.checked_add(elem)).and_then(|s| s.checked_add(byte_offset));
        let end = end.filter(|&e| v.buffer == 0 && e <= v.byte_length).ok_or_else(|| {
            invalid(format!(
                "range: {count} {} elements at offset {byte_offset} do not fit in view {view} ({} bytes)", ty.as_str(), v.byte_length
            ))
        })?;
        let data = v
            .byte_offset
            .checked_add(end)
            .and_then(|e| self.buffer_data.get(v.byte_offset + byte_offset..e))
            .ok_or_else(|| invalid(format!("view {view} extends past the buffer data")))?;
        let elements: Vec<Vec<f64>> = (0..count)
            .map(|i| decode_element(component.code(), ty.as_str(), &data[i * stride..i * stride + elem]).expect("sized above"))
            .collect();
        // Float data in the file must be finite whatever the bounds mode (NaN/inf are not valid glTF).
        if component == ComponentType::F32 {
            if let Some(e) = elements.iter().position(|e| e.iter().any(|v| !v.is_finite())) {
                return Err(invalid(format!("element {e} in view {view} is not finite")));
            }
        }
        let actual = || bounds_of(elements.iter().cloned());
        let stored = match &bounds {
            Bounds::None => None,
            Bounds::Compute => actual(),
            Bounds::Explicit { min, max } => {
                check_explicit(min, max, ty.components(), component, actual().as_ref())?;
                Some((min.clone(), max.clone()))
            }
        };
        Ok(self.push_accessor(Accessor {
            buffer_view: Some(view),
            byte_offset: (byte_offset != 0).then_some(byte_offset),
            component_type: component.code(),
            count,
            type_: ty.as_str().to_string(),
            min: stored.as_ref().map(|b| b.0.clone()),
            max: stored.map(|b| b.1),
            normalized: normalized.then_some(true),
            ..Default::default()
        }))
    }
}

/// Bounds are stored as `f64`; on export, whole-number bounds of integer accessors become JSON
/// integers (`127`, not `127.0`), as the spec's "same data type as componentType" asks.
/// `FLOAT` accessors are left alone.
pub(crate) fn write_integer_bounds_as_integers(doc: &mut serde_json::Value) {
    let Some(accessors) = doc.get_mut("accessors").and_then(|a| a.as_array_mut()) else { return };
    for a in accessors {
        let integer = a.get("componentType").and_then(|c| c.as_u64()).is_some_and(|c| integer_range(c as usize).is_some());
        if !integer {
            continue;
        }
        for key in ["min", "max"] {
            let Some(values) = a.get_mut(key).and_then(|v| v.as_array_mut()) else { continue };
            for v in values {
                match v.as_f64() {
                    Some(f) if f.fract() == 0.0 && f >= i64::MIN as f64 && f < i64::MAX as f64 => *v = serde_json::Value::from(f as i64),
                    _ => {}
                }
            }
        }
    }
}
