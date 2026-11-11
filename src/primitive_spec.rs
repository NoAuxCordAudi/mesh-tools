//! Primitive description types: [`PrimitiveMode`], [`Indices`], [`TexCoords`], [`ColorData`],
//! [`PrimitiveSpec`] (input of [`GltfBuilder::create_primitive`]) and the pure strip/fan
//! conversion helpers. See `docs/MESHES.md` and `docs/PRIMITIVE_MODES.md`.
//!
//! [`GltfBuilder::create_primitive`]: crate::GltfBuilder::create_primitive

use serde_json::Value;

use crate::builder_skin::SkinSet;
use crate::constants::primitive_mode;
use crate::error::{GltfError, Result};
use crate::models::{Accessor, Primitive};

fn invalid(msg: String) -> GltfError {
    GltfError::InvalidInput(msg)
}

/// Primitive topology (`primitive.mode`).
///
/// # Example
///
/// ```
/// use mesh_tools::PrimitiveMode;
/// assert_eq!(PrimitiveMode::from_code(5).unwrap(), PrimitiveMode::TriangleStrip);
/// assert!(PrimitiveMode::Lines.is_valid_count(4) && !PrimitiveMode::Lines.is_valid_count(3));
/// assert!(PrimitiveMode::from_code(7).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PrimitiveMode {
    Points,
    Lines,
    LineLoop,
    LineStrip,
    /// The glTF default when `mode` is absent.
    #[default]
    Triangles,
    TriangleStrip,
    TriangleFan,
}

impl PrimitiveMode {
    /// The glTF `mode` code (0 to 6).
    pub fn code(self) -> usize {
        match self {
            Self::Points => primitive_mode::POINTS,
            Self::Lines => primitive_mode::LINES,
            Self::LineLoop => primitive_mode::LINE_LOOP,
            Self::LineStrip => primitive_mode::LINE_STRIP,
            Self::Triangles => primitive_mode::TRIANGLES,
            Self::TriangleStrip => primitive_mode::TRIANGLE_STRIP,
            Self::TriangleFan => primitive_mode::TRIANGLE_FAN,
        }
    }

    /// The mode for a glTF `mode` code.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` for a code above 6.
    pub fn from_code(code: usize) -> Result<Self> {
        Ok(match code {
            primitive_mode::POINTS => Self::Points,
            primitive_mode::LINES => Self::Lines,
            primitive_mode::LINE_LOOP => Self::LineLoop,
            primitive_mode::LINE_STRIP => Self::LineStrip,
            primitive_mode::TRIANGLES => Self::Triangles,
            primitive_mode::TRIANGLE_STRIP => Self::TriangleStrip,
            primitive_mode::TRIANGLE_FAN => Self::TriangleFan,
            _ => return Err(invalid(format!("primitive mode {code} is not one of 0..=6"))),
        })
    }

    /// Fewest indices (or, non-indexed, vertices) that draw anything.
    pub fn min_count(self) -> usize {
        match self {
            Self::Points => 1,
            Self::Lines | Self::LineLoop | Self::LineStrip => 2,
            Self::Triangles | Self::TriangleStrip | Self::TriangleFan => 3,
        }
    }

    /// The count must be a multiple of this: 2 for lines, 3 for triangles, else 1.
    pub fn count_multiple(self) -> usize {
        match self {
            Self::Lines => 2,
            Self::Triangles => 3,
            _ => 1,
        }
    }

    /// Whether `count` indices (or vertices) are valid for this mode.
    pub fn is_valid_count(self, count: usize) -> bool {
        count >= self.min_count() && count % self.count_multiple() == 0
    }
}

impl Primitive {
    /// The primitive's mode; a missing `mode` is [`PrimitiveMode::Triangles`].
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if `mode` is not in `0..=6`.
    pub fn mode_enum(&self) -> Result<PrimitiveMode> {
        self.mode.map_or(Ok(PrimitiveMode::Triangles), PrimitiveMode::from_code)
    }
}

/// Convert a triangle strip to a triangle list, following the glTF winding rule: triangle `i`
/// is `(i, i+1, i+2)` for even `i` and `(i+1, i, i+2)` for odd `i`, so all keep the same
/// facing. Degenerate triangles are kept, as a renderer would draw them.
///
/// # Errors
///
/// `GltfError::InvalidInput` for fewer than 3 indices.
///
/// # Example
///
/// ```
/// use mesh_tools::strip_to_triangles;
/// assert_eq!(strip_to_triangles(&[0, 1, 2, 3, 4]).unwrap(), [0, 1, 2, 2, 1, 3, 2, 3, 4]);
/// assert!(strip_to_triangles(&[0, 1]).is_err());
/// ```
pub fn strip_to_triangles(indices: &[u32]) -> Result<Vec<u32>> {
    check_len("triangle strip", indices)?;
    let mut out = Vec::with_capacity((indices.len() - 2) * 3);
    for (i, w) in indices.windows(3).enumerate() {
        if i % 2 == 0 {
            out.extend_from_slice(&[w[0], w[1], w[2]]);
        } else {
            out.extend_from_slice(&[w[1], w[0], w[2]]);
        }
    }
    Ok(out)
}

/// Convert a triangle fan to a triangle list: triangle `i` is `(first, i+1, i+2)`.
///
/// # Errors
///
/// `GltfError::InvalidInput` for fewer than 3 indices.
///
/// # Example
///
/// ```
/// use mesh_tools::fan_to_triangles;
/// assert_eq!(fan_to_triangles(&[0, 1, 2, 3, 4]).unwrap(), [0, 1, 2, 0, 2, 3, 0, 3, 4]);
/// assert!(fan_to_triangles(&[]).is_err());
/// ```
pub fn fan_to_triangles(indices: &[u32]) -> Result<Vec<u32>> {
    check_len("triangle fan", indices)?;
    let mut out = Vec::with_capacity((indices.len() - 2) * 3);
    for w in indices[1..].windows(2) {
        out.extend_from_slice(&[indices[0], w[0], w[1]]);
    }
    Ok(out)
}

fn check_len(what: &str, indices: &[u32]) -> Result<()> {
    if indices.len() < 3 {
        return Err(invalid(format!("a {what} needs at least 3 indices, got {}", indices.len())));
    }
    Ok(())
}

/// Index data of a primitive, in the width it is written in.
///
/// The restart value of each width (255, 65535, 4294967295) is not a legal index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Indices {
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
}

impl From<Vec<u8>> for Indices {
    fn from(v: Vec<u8>) -> Self {
        Self::U8(v)
    }
}
impl From<Vec<u16>> for Indices {
    fn from(v: Vec<u16>) -> Self {
        Self::U16(v)
    }
}
impl From<Vec<u32>> for Indices {
    fn from(v: Vec<u32>) -> Self {
        Self::U32(v)
    }
}

impl Indices {
    /// `u16` unless an index is 65535 or more, then `u32` (the width the skinned-primitive and
    /// `MeshBuilder::with_indices` writers have always chosen).
    pub fn auto(indices: Vec<u32>) -> Self {
        if indices.iter().all(|&i| i < 65535) {
            Self::U16(indices.into_iter().map(|i| i as u16).collect())
        } else {
            Self::U32(indices)
        }
    }

    /// Number of indices.
    pub fn len(&self) -> usize {
        match self {
            Self::U8(v) => v.len(),
            Self::U16(v) => v.len(),
            Self::U32(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn get(&self, i: usize) -> u32 {
        match self {
            Self::U8(v) => u32::from(v[i]),
            Self::U16(v) => u32::from(v[i]),
            Self::U32(v) => v[i],
        }
    }

    /// The primitive-restart value of this width.
    pub(crate) fn restart(&self) -> u32 {
        match self {
            Self::U8(_) => u32::from(u8::MAX),
            Self::U16(_) => u32::from(u16::MAX),
            Self::U32(_) => u32::MAX,
        }
    }
}

/// One `TEXCOORD_n` set. Integer sets are written normalized (`255` / `65535` read as `1.0`).
#[derive(Debug, Clone, PartialEq)]
pub enum TexCoords {
    F32(Vec<f32>),
    U8(Vec<u8>),
    U16(Vec<u16>),
}

impl TexCoords {
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::F32(v) => v.len(),
            Self::U8(v) => v.len(),
            Self::U16(v) => v.len(),
        }
    }
}

/// One `COLOR_n` set, tightly packed. Integer sets are written normalized.
#[derive(Debug, Clone, PartialEq)]
pub enum ColorData {
    Rgb(Vec<f32>),
    Rgba(Vec<f32>),
    RgbU8(Vec<u8>),
    RgbaU8(Vec<u8>),
    RgbU16(Vec<u16>),
    RgbaU16(Vec<u16>),
}

impl ColorData {
    /// (values, components per vertex)
    pub(crate) fn shape(&self) -> (usize, usize) {
        match self {
            Self::Rgb(v) => (v.len(), 3),
            Self::Rgba(v) => (v.len(), 4),
            Self::RgbU8(v) => (v.len(), 3),
            Self::RgbaU8(v) => (v.len(), 4),
            Self::RgbU16(v) => (v.len(), 3),
            Self::RgbaU16(v) => (v.len(), 4),
        }
    }
}

/// Everything [`GltfBuilder::create_primitive`](crate::GltfBuilder::create_primitive) writes.
///
/// Build it with `..Default::default()`. Counts are per vertex: `positions`/`normals` hold 3
/// floats, `tangents` 4 (`w` is the handedness, `1.0` or `-1.0`), texcoord sets 2 values.
/// `indices: None` means non-indexed geometry.
#[derive(Debug, Clone, Default)]
pub struct PrimitiveSpec {
    pub positions: Vec<f32>,
    pub indices: Option<Indices>,
    pub normals: Option<Vec<f32>>,
    pub tangents: Option<Vec<f32>>,
    /// Set `n` becomes `TEXCOORD_n`.
    pub texcoords: Vec<TexCoords>,
    /// Set `n` becomes `COLOR_n`.
    pub colors: Vec<ColorData>,
    /// Set `n` becomes `JOINTS_n`/`WEIGHTS_n`.
    pub skin_sets: Vec<SkinSet>,
    /// Joints of the skin the primitive will be used with; when `None` only the `u16` range is
    /// enforced.
    pub joint_count: Option<usize>,
    pub material: Option<usize>,
    pub mode: PrimitiveMode,
    pub extras: Option<Value>,
}

// ---------------------------------------------------------------- attribute semantics

/// Parsed attribute semantic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Semantic {
    Position,
    Normal,
    Tangent,
    TexCoord(usize),
    Color(usize),
    Joints(usize),
    Weights(usize),
    Custom,
}

fn set_index(rest: &str) -> Option<usize> {
    (!rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())).then(|| rest.parse().unwrap_or(usize::MAX))
}

/// `None` if `name` is not a glTF semantic or an application-specific one (`_NAME`).
pub(crate) fn parse_semantic(name: &str) -> Option<Semantic> {
    match name {
        "POSITION" => return Some(Semantic::Position),
        "NORMAL" => return Some(Semantic::Normal),
        "TANGENT" => return Some(Semantic::Tangent),
        _ => {}
    }
    if name.len() > 1 && name.starts_with('_') {
        return Some(Semantic::Custom);
    }
    let (prefix, rest) = name.split_once('_')?;
    let n = set_index(rest)?;
    match prefix {
        "TEXCOORD" => Some(Semantic::TexCoord(n)),
        "COLOR" => Some(Semantic::Color(n)),
        "JOINTS" => Some(Semantic::Joints(n)),
        "WEIGHTS" => Some(Semantic::Weights(n)),
        _ => None,
    }
}

/// Why accessor `a` cannot back the attribute `name` (the semantic type table), if it cannot.
pub(crate) fn attribute_type_error(sem: Semantic, a: &Accessor) -> Option<&'static str> {
    let norm = a.normalized == Some(true);
    let float = a.component_type == 5126;
    let small_uint = matches!(a.component_type, 5121 | 5123);
    // float, or normalized unsigned byte / short
    let unit = (float && !norm) || (small_uint && norm);
    let t = a.type_.as_str();
    let ok = match sem {
        Semantic::Position | Semantic::Normal => t == "VEC3" && float,
        Semantic::Tangent => t == "VEC4" && float,
        Semantic::TexCoord(_) => t == "VEC2" && unit,
        Semantic::Color(_) => matches!(t, "VEC3" | "VEC4") && unit,
        Semantic::Joints(_) => t == "VEC4" && small_uint && !norm,
        Semantic::Weights(_) => t == "VEC4" && unit,
        Semantic::Custom => true,
    };
    (!ok).then(|| match sem {
        Semantic::Position => "must be FLOAT VEC3",
        Semantic::Normal => "must be FLOAT VEC3",
        Semantic::Tangent => "must be FLOAT VEC4",
        Semantic::TexCoord(_) => "must be VEC2 of FLOAT, normalized UNSIGNED_BYTE or normalized UNSIGNED_SHORT",
        Semantic::Color(_) => "must be VEC3/VEC4 of FLOAT, normalized UNSIGNED_BYTE or normalized UNSIGNED_SHORT",
        Semantic::Joints(_) => "must be VEC4 of UNSIGNED_BYTE or UNSIGNED_SHORT",
        Semantic::Weights(_) => "must be VEC4 of FLOAT, normalized UNSIGNED_BYTE or normalized UNSIGNED_SHORT",
        Semantic::Custom => "", // unreachable: custom attributes accept every type
    })
}

/// A finding about one primitive that needs only accessor metadata (no buffer data).
pub(crate) struct Issue {
    pub code: &'static str,
    pub warning: bool,
    pub msg: String,
}

fn err(code: &'static str, msg: String) -> Issue {
    Issue { code, warning: false, msg }
}

fn warn(code: &'static str, msg: String) -> Issue {
    Issue { code, warning: true, msg }
}

/// Every metadata-level check of a primitive, shared by `validate()` and `add_mesh_checked`.
/// Messages do not name the mesh or primitive; the caller prefixes them.
pub(crate) fn check_primitive(p: &Primitive, accessors: &[Accessor], material_count: usize) -> Vec<Issue> {
    let mut out = Vec::new();
    let mode = match p.mode_enum() {
        Ok(m) => m,
        Err(_) => {
            out.push(err("PRIMITIVE_MODE_INVALID", format!("mode {} is not one of 0..=6", p.mode.unwrap_or(0))));
            PrimitiveMode::Triangles
        }
    };
    // Sorted so messages are deterministic.
    let mut names: Vec<&String> = p.attributes.keys().collect();
    names.sort();
    let mut vertex_count: Option<usize> = None;
    for name in names {
        let acc = p.attributes[name];
        let Some(a) = accessors.get(acc) else {
            out.push(err("PRIMITIVE_ACCESSOR_INVALID", format!("{name} references accessor {acc}, which does not exist")));
            continue;
        };
        match parse_semantic(name) {
            None => out.push(err(
                "PRIMITIVE_ATTRIBUTE_NAME_INVALID",
                format!("attribute {name:?} is not POSITION, NORMAL, TANGENT, TEXCOORD_n, COLOR_n, JOINTS_n, WEIGHTS_n or an application-specific _NAME"),
            )),
            Some(sem) => {
                if sem == Semantic::Position && (a.min.is_none() || a.max.is_none()) {
                    out.push(err("ACCESSOR_BOUNDS_REQUIRED", format!("POSITION accessor {acc} lacks min/max")));
                }
                if let Some(why) = attribute_type_error(sem, a) {
                    out.push(err(
                        "PRIMITIVE_ATTRIBUTE_TYPE",
                        format!("{name} accessor {acc} ({} of component type {}) {why}", a.type_, a.component_type),
                    ));
                }
            }
        }
        match vertex_count {
            None => vertex_count = Some(a.count),
            Some(n) if n != a.count => out.push(err(
                "PRIMITIVE_ATTRIBUTE_COUNT_MISMATCH",
                format!("{name} has {} elements but other attributes have {n}", a.count),
            )),
            _ => {}
        }
    }
    let mut count = vertex_count;
    if let Some(idx) = p.indices {
        match accessors.get(idx) {
            None => out.push(err("PRIMITIVE_INDICES_INVALID", format!("indices reference accessor {idx}, which does not exist"))),
            Some(a) => {
                if a.type_ != "SCALAR" || !matches!(a.component_type, 5121 | 5123 | 5125) {
                    out.push(err("PRIMITIVE_INDICES_TYPE", format!("indices accessor {idx} must be unsigned SCALAR")));
                }
                count = Some(a.count);
            }
        }
    }
    if let Some(n) = count {
        if !mode.is_valid_count(n) {
            let what = if p.indices.is_some() { "indices" } else { "vertices" };
            out.push(err(
                "PRIMITIVE_COUNT_INVALID",
                format!("{n} {what} are not valid for mode {mode:?} (at least {}, a multiple of {})", mode.min_count(), mode.count_multiple()),
            ));
        }
    }
    if let Some(m) = p.material {
        if m >= material_count {
            out.push(err("PRIMITIVE_MATERIAL_INVALID", format!("material {m} does not exist ({material_count} materials)")));
        }
    }
    for (name, _) in p.attributes.iter().filter(|(n, _)| n.starts_with("JOINTS_")) {
        let n = &name["JOINTS_".len()..];
        if !p.attributes.contains_key(&format!("WEIGHTS_{n}")) {
            out.push(err("PRIMITIVE_JOINTS_WITHOUT_WEIGHTS", format!("has JOINTS_{n} but no WEIGHTS_{n}")));
        }
    }
    for (name, _) in p.attributes.iter().filter(|(n, _)| n.starts_with("WEIGHTS_")) {
        let n = &name["WEIGHTS_".len()..];
        if !p.attributes.contains_key(&format!("JOINTS_{n}")) {
            out.push(err("PRIMITIVE_WEIGHTS_WITHOUT_JOINTS", format!("has {name} but no JOINTS_{n}")));
        }
    }
    // Warnings.
    if !p.attributes.contains_key("POSITION") {
        out.push(warn("MESH_PRIMITIVE_NO_POSITION", "has no POSITION attribute".to_string()));
    }
    if p.attributes.contains_key("TANGENT") && !p.attributes.contains_key("NORMAL") {
        out.push(warn("PRIMITIVE_TANGENT_WITHOUT_NORMAL", "has TANGENT but no NORMAL".to_string()));
    }
    for family in ["TEXCOORD", "COLOR", "JOINTS", "WEIGHTS"] {
        let prefix = format!("{family}_");
        let mut sets: Vec<usize> = p.attributes.keys().filter_map(|k| k.strip_prefix(&prefix).and_then(set_index)).collect();
        sets.sort_unstable();
        sets.dedup();
        if let Some(missing) = (0..sets.len()).find(|&n| sets[n] != n) {
            out.push(warn("PRIMITIVE_ATTRIBUTE_SET_GAP", format!("{family}_{missing} is missing but a higher {family}_n set exists")));
        }
    }
    out
}
