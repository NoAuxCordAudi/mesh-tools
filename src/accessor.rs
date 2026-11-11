//! # Typed accessors
//!
//! Types for writing accessors with [`GltfBuilder::add_accessor_from_slice`],
//! [`GltfBuilder::add_accessor_in_view`] and [`GltfBuilder::add_zero_accessor`], plus the
//! element-layout helpers shared with the validator.
//!
//! Bounds (`min`/`max`) are always in the *raw* component domain: for a normalized `i8`
//! accessor holding `[-127, 0, 127]` they are `[-127]` and `[127]`, not `-1` and `1`.
//!
//! [`GltfBuilder::add_accessor_from_slice`]: crate::GltfBuilder::add_accessor_from_slice
//! [`GltfBuilder::add_accessor_in_view`]: crate::GltfBuilder::add_accessor_in_view
//! [`GltfBuilder::add_zero_accessor`]: crate::GltfBuilder::add_zero_accessor
//! [`GltfBuilder`]: crate::GltfBuilder

use crate::constants::{buffer_view_target, component_type};

/// Accessor component type (`componentType`).
///
/// glTF has no 64-bit or `i32` components; `U32` is only legal for indices and custom
/// attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentType {
    /// Signed byte (5120).
    I8,
    /// Unsigned byte (5121).
    U8,
    /// Signed short (5122).
    I16,
    /// Unsigned short (5123).
    U16,
    /// Unsigned int (5125).
    U32,
    /// 32-bit float (5126).
    F32,
}

impl ComponentType {
    /// The glTF `componentType` code.
    pub fn code(self) -> usize {
        match self {
            Self::I8 => component_type::BYTE,
            Self::U8 => component_type::UNSIGNED_BYTE,
            Self::I16 => component_type::SHORT,
            Self::U16 => component_type::UNSIGNED_SHORT,
            Self::U32 => component_type::UNSIGNED_INT,
            Self::F32 => component_type::FLOAT,
        }
    }

    /// Bytes per component.
    pub fn size(self) -> usize {
        component_size(self.code()).expect("every ComponentType has a size")
    }

    /// Whether glTF allows `normalized: true` for this type (all integers except `U32`).
    pub fn can_normalize(self) -> bool {
        !matches!(self, Self::F32 | Self::U32)
    }
}

/// Accessor element type (`type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessorType {
    Scalar,
    Vec2,
    Vec3,
    Vec4,
    Mat2,
    Mat3,
    Mat4,
}

impl AccessorType {
    /// The glTF `type` string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scalar => "SCALAR",
            Self::Vec2 => "VEC2",
            Self::Vec3 => "VEC3",
            Self::Vec4 => "VEC4",
            Self::Mat2 => "MAT2",
            Self::Mat3 => "MAT3",
            Self::Mat4 => "MAT4",
        }
    }

    /// Number of components per element (a matrix counts all its entries: MAT3 is 9).
    pub fn components(self) -> usize {
        let (cols, rows) = self.shape();
        cols * rows
    }

    /// (columns, rows); vectors and scalars have one column.
    pub(crate) fn shape(self) -> (usize, usize) {
        match self {
            Self::Scalar => (1, 1),
            Self::Vec2 => (1, 2),
            Self::Vec3 => (1, 3),
            Self::Vec4 => (1, 4),
            Self::Mat2 => (2, 2),
            Self::Mat3 => (3, 3),
            Self::Mat4 => (4, 4),
        }
    }
}

/// Target of a buffer view (`target`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BufferViewTarget {
    /// Vertex attribute data (34962).
    ArrayBuffer,
    /// Index data (34963).
    ElementArrayBuffer,
}

impl BufferViewTarget {
    /// The glTF `target` code.
    pub fn code(self) -> usize {
        match self {
            Self::ArrayBuffer => buffer_view_target::ARRAY_BUFFER,
            Self::ElementArrayBuffer => buffer_view_target::ELEMENT_ARRAY_BUFFER,
        }
    }
}

/// How an accessor's `min`/`max` are produced.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Bounds {
    /// Write no bounds. (Required on `POSITION`, and on animation sampler inputs.)
    #[default]
    None,
    /// Compute them from the data, per component, in the raw component domain.
    Compute,
    /// Use these, one value per component. They must equal the data's actual bounds: the writer
    /// rejects a mismatch rather than emit a file the validator would reject.
    Explicit { min: Vec<f64>, max: Vec<f64> },
}

/// Options for [`GltfBuilder::add_accessor_from_slice`](crate::GltfBuilder::add_accessor_from_slice).
///
/// Build it with [`AccessorSpec::new`] and the `with_*` methods; the default is a plain,
/// un-named accessor with no bounds in a view without a target.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct AccessorSpec {
    /// Integer components are mapped to \[0, 1\] / \[-1, 1\]. Not allowed for `F32` and `U32`.
    pub normalized: bool,
    /// Optional user-facing name.
    pub name: Option<String>,
    /// Target of the new buffer view.
    pub target: Option<BufferViewTarget>,
    /// `min`/`max` handling.
    pub bounds: Bounds,
}

impl AccessorSpec {
    /// A plain spec; see the type docs for the defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set `normalized`.
    pub fn with_normalized(mut self, normalized: bool) -> Self {
        self.normalized = normalized;
        self
    }

    /// Set the accessor name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the buffer view target.
    pub fn with_target(mut self, target: BufferViewTarget) -> Self {
        self.target = Some(target);
        self
    }

    /// Set the bounds handling.
    pub fn with_bounds(mut self, bounds: Bounds) -> Self {
        self.bounds = bounds;
        self
    }
}

mod sealed {
    pub trait Sealed {}
}

/// A Rust type that is a legal glTF accessor component: `i8`, `u8`, `i16`, `u16`, `u32`, `f32`.
///
/// Sealed: it cannot be implemented outside this crate.
pub trait Component: sealed::Sealed + Copy {
    /// The matching glTF component type.
    const TYPE: ComponentType;
    #[doc(hidden)]
    fn push_le(self, out: &mut Vec<u8>);
    #[doc(hidden)]
    fn to_f64(self) -> f64;
}

macro_rules! component {
    ($t:ty, $c:expr) => {
        impl sealed::Sealed for $t {}
        impl Component for $t {
            const TYPE: ComponentType = $c;
            fn push_le(self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_le_bytes());
            }
            fn to_f64(self) -> f64 {
                self as f64
            }
        }
    };
}
component!(i8, ComponentType::I8);
component!(u8, ComponentType::U8);
component!(i16, ComponentType::I16);
component!(u16, ComponentType::U16);
component!(u32, ComponentType::U32);
component!(f32, ComponentType::F32);

// ---- layout helpers shared with the validator (operate on raw glTF codes) ----

/// Bytes per component for a glTF component type code.
pub(crate) fn component_size(component_type: usize) -> Option<usize> {
    match component_type {
        5120 | 5121 => Some(1),
        5122 | 5123 => Some(2),
        5125 | 5126 => Some(4),
        _ => None,
    }
}

/// (columns, rows) for a glTF accessor type string.
pub(crate) fn type_shape(type_: &str) -> Option<(usize, usize)> {
    Some(match type_ {
        "SCALAR" => (1, 1),
        "VEC2" => (1, 2),
        "VEC3" => (1, 3),
        "VEC4" => (1, 4),
        "MAT2" => (2, 2),
        "MAT3" => (3, 3),
        "MAT4" => (4, 4),
        _ => return None,
    })
}

/// Bytes per element. Matrix columns are padded to 4-byte boundaries (glTF 3.6.2.4), so
/// MAT2 BYTE is 8, MAT3 BYTE is 12 and MAT3 SHORT is 24.
pub(crate) fn element_size(component_type: usize, type_: &str) -> Option<usize> {
    let comp = component_size(component_type)?;
    let (cols, rows) = type_shape(type_)?;
    if cols == 1 {
        return Some(comp * rows);
    }
    Some((comp * rows).div_ceil(4) * 4 * cols)
}

/// Decode one element's bytes (as laid out in a buffer, matrix padding included) into its
/// components, in column-major order.
pub(crate) fn decode_element(component_type: usize, type_: &str, elem: &[u8]) -> Option<Vec<f64>> {
    let comp = component_size(component_type)?;
    let (cols, rows) = type_shape(type_)?;
    let col_stride = if cols == 1 { comp * rows } else { (comp * rows).div_ceil(4) * 4 };
    let mut out = Vec::with_capacity(cols * rows);
    for c in 0..cols {
        for r in 0..rows {
            let at = c * col_stride + r * comp;
            let b = elem.get(at..at + comp)?;
            out.push(match component_type {
                5120 => b[0] as i8 as f64,
                5121 => b[0] as f64,
                5122 => i16::from_le_bytes([b[0], b[1]]) as f64,
                5123 => u16::from_le_bytes([b[0], b[1]]) as f64,
                5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
                _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
            });
        }
    }
    Some(out)
}

/// Per-component (min, max) of decoded elements; `None` for no elements. Mirrors `f32::min`
/// / `max` semantics, in element order.
pub(crate) fn bounds_of(elements: impl Iterator<Item = Vec<f64>>) -> Option<(Vec<f64>, Vec<f64>)> {
    let mut acc: Option<(Vec<f64>, Vec<f64>)> = None;
    for e in elements {
        match &mut acc {
            None => acc = Some((e.clone(), e)),
            Some((lo, hi)) => {
                for (k, v) in e.iter().enumerate() {
                    lo[k] = lo[k].min(*v);
                    hi[k] = hi[k].max(*v);
                }
            }
        }
    }
    acc
}

/// Inclusive value range of an integer component type code.
pub(crate) fn integer_range(component_type: usize) -> Option<(f64, f64)> {
    Some(match component_type {
        5120 => (i8::MIN as f64, i8::MAX as f64),
        5121 => (0.0, u8::MAX as f64),
        5122 => (i16::MIN as f64, i16::MAX as f64),
        5123 => (0.0, u16::MAX as f64),
        5125 => (0.0, u32::MAX as f64),
        _ => return None,
    })
}
