//! Primitive and mesh creation: [`GltfBuilder::create_primitive`], the per-mode helpers,
//! [`GltfBuilder::add_mesh_checked`] and mesh/primitive extras.

use serde_json::Value;

use crate::accessor::{AccessorSpec, AccessorType as T, Bounds, BufferViewTarget, Component};
use crate::builder::GltfBuilder;
use crate::builder_accessor::encode_elements;
use crate::builder_extensions::ExtensionTarget;
use crate::builder_skin::{validate_skin_sets, JointIndices, SkinWeights};
use crate::error::{GltfError, Result};
use crate::models::{Accessor, Primitive};
use crate::primitive_spec::*;

fn invalid(msg: String) -> GltfError {
    GltfError::InvalidInput(msg)
}

fn finite(what: &str, data: &[f32]) -> Result<()> {
    match data.iter().position(|v| !v.is_finite()) {
        Some(i) => Err(invalid(format!("{what} value {i} is not finite"))),
        None => Ok(()),
    }
}

impl GltfBuilder {
    /// Validate `spec` and write it as a primitive (the caller adds it to a mesh with
    /// [`add_mesh_checked`](Self::add_mesh_checked) or `add_mesh`).
    ///
    /// Accessors are written in the order `POSITION`, `NORMAL`, `TANGENT`, `TEXCOORD_n`,
    /// `COLOR_n`, `JOINTS_n`/`WEIGHTS_n`, indices. `POSITION` carries `min`/`max`. Integer
    /// texcoord and color sets are written normalized; elements that are not a multiple of 4
    /// bytes (for example RGB `u8`) go into a view with a 4-byte-aligned `byteStride`, as glTF
    /// requires for vertex attributes.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput`, naming the vertex or index at fault, if:
    /// - positions are empty, not a multiple of 3, or any float input is NaN or infinite
    /// - normals, tangents, texcoord or color sets do not match the vertex count, or a tangent
    ///   `w` is not `1.0` or `-1.0`
    /// - the index (or, non-indexed, vertex) count is invalid for the mode (see
    ///   [`PrimitiveMode::is_valid_count`]), an index is `>=` the vertex count, or an index is
    ///   the restart value of its width
    /// - skinning data is invalid (see
    ///   [`create_skinned_primitive`](Self::create_skinned_primitive))
    /// - the material does not exist
    ///
    /// All validation runs before anything is written, so a failed call leaves the document
    /// unchanged.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::{GltfBuilder, Indices, PrimitiveMode, PrimitiveSpec};
    /// let mut b = GltfBuilder::new();
    /// let p = b
    ///     .create_primitive(PrimitiveSpec {
    ///         positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
    ///         indices: Some(Indices::U16(vec![0, 1, 2])),
    ///         mode: PrimitiveMode::Triangles,
    ///         ..Default::default()
    ///     })
    ///     .unwrap();
    /// assert_eq!(p.mode, Some(4));
    /// // Index 3 is out of range for 3 vertices; nothing is written.
    /// let n = b.gltf.accessors.as_ref().unwrap().len();
    /// let bad = PrimitiveSpec {
    ///     positions: vec![0.0; 9],
    ///     indices: Some(Indices::U16(vec![0, 1, 3])),
    ///     ..Default::default()
    /// };
    /// assert!(b.create_primitive(bad).is_err());
    /// assert_eq!(b.gltf.accessors.as_ref().unwrap().len(), n);
    /// ```
    pub fn create_primitive(&mut self, spec: PrimitiveSpec) -> Result<Primitive> {
        self.validate_spec(&spec)?;
        // ---- All input is valid; from here on we only write. ----
        let mut attributes = std::collections::HashMap::new();
        let pos_spec = AccessorSpec::new().with_target(BufferViewTarget::ArrayBuffer).with_bounds(Bounds::Compute);
        attributes.insert("POSITION".to_string(), self.write_slice(&spec.positions, T::Vec3, &pos_spec));
        if let Some(n) = &spec.normals {
            attributes.insert("NORMAL".to_string(), self.write_vertex_attribute(n, T::Vec3, false));
        }
        if let Some(t) = &spec.tangents {
            attributes.insert("TANGENT".to_string(), self.write_vertex_attribute(t, T::Vec4, false));
        }
        for (n, t) in spec.texcoords.iter().enumerate() {
            let acc = match t {
                TexCoords::F32(v) => self.write_vertex_attribute(v, T::Vec2, false),
                TexCoords::U8(v) => self.write_vertex_attribute(v, T::Vec2, true),
                TexCoords::U16(v) => self.write_vertex_attribute(v, T::Vec2, true),
            };
            attributes.insert(format!("TEXCOORD_{n}"), acc);
        }
        for (n, c) in spec.colors.iter().enumerate() {
            let acc = match c {
                ColorData::Rgb(v) => self.write_vertex_attribute(v, T::Vec3, false),
                ColorData::Rgba(v) => self.write_vertex_attribute(v, T::Vec4, false),
                ColorData::RgbU8(v) => self.write_vertex_attribute(v, T::Vec3, true),
                ColorData::RgbaU8(v) => self.write_vertex_attribute(v, T::Vec4, true),
                ColorData::RgbU16(v) => self.write_vertex_attribute(v, T::Vec3, true),
                ColorData::RgbaU16(v) => self.write_vertex_attribute(v, T::Vec4, true),
            };
            attributes.insert(format!("COLOR_{n}"), acc);
        }
        for (n, set) in spec.skin_sets.iter().enumerate() {
            let joints = match &set.joints {
                JointIndices::U8(j) => {
                    let flat: Vec<u8> = j.iter().flatten().copied().collect();
                    self.write_vertex_attribute(&flat, T::Vec4, false)
                }
                JointIndices::U16(j) => {
                    let flat: Vec<u16> = j.iter().flatten().copied().collect();
                    self.write_vertex_attribute(&flat, T::Vec4, false)
                }
            };
            attributes.insert(format!("JOINTS_{n}"), joints);
            let weights = match &set.weights {
                SkinWeights::F32(w) => {
                    let flat: Vec<f32> = w.iter().flatten().copied().collect();
                    self.write_vertex_attribute(&flat, T::Vec4, false)
                }
                SkinWeights::U8(w) => {
                    let flat: Vec<u8> = w.iter().flatten().copied().collect();
                    self.write_vertex_attribute(&flat, T::Vec4, true)
                }
                SkinWeights::U16(w) => {
                    let flat: Vec<u16> = w.iter().flatten().copied().collect();
                    self.write_vertex_attribute(&flat, T::Vec4, true)
                }
            };
            attributes.insert(format!("WEIGHTS_{n}"), weights);
        }
        let index_spec = AccessorSpec::new().with_target(BufferViewTarget::ElementArrayBuffer);
        let indices = spec.indices.as_ref().map(|i| match i {
            Indices::U8(v) => self.write_slice(v, T::Scalar, &index_spec),
            Indices::U16(v) => self.write_slice(v, T::Scalar, &index_spec),
            Indices::U32(v) => self.write_slice(v, T::Scalar, &index_spec),
        });
        Ok(Primitive {
            attributes,
            indices,
            material: spec.material,
            mode: Some(spec.mode.code()),
            extras: spec.extras,
            ..Default::default()
        })
    }

    /// Write one vertex attribute in its own `ARRAY_BUFFER` view. Elements that are not a
    /// multiple of 4 bytes get a `byteStride` rounded up to 4 (glTF: vertex attribute elements
    /// are 4-byte aligned).
    fn write_vertex_attribute<C: Component>(&mut self, data: &[C], ty: T, normalized: bool) -> usize {
        let spec = AccessorSpec::new().with_target(BufferViewTarget::ArrayBuffer).with_normalized(normalized);
        let comps = ty.components();
        let elem = comps * C::TYPE.size();
        if elem % 4 == 0 {
            return self.write_slice(data, ty, &spec);
        }
        let stride = (elem + 3) & !3;
        let count = data.len() / comps;
        let tight = encode_elements(data, ty);
        let mut padded = vec![0u8; stride * count];
        for (i, e) in tight.chunks_exact(elem).enumerate() {
            padded[i * stride..i * stride + elem].copy_from_slice(e);
        }
        let (offset, length) = self.add_buffer_data(&padded);
        let view = self.add_buffer_view(offset, length, Some(BufferViewTarget::ArrayBuffer.code()));
        if let Some(v) = self.gltf.buffer_views.as_mut().and_then(|v| v.last_mut()) {
            v.byte_stride = Some(stride);
        }
        self.push_accessor(Accessor {
            buffer_view: Some(view),
            component_type: C::TYPE.code(),
            count,
            type_: ty.as_str().to_string(),
            normalized: normalized.then_some(true),
            ..Default::default()
        })
    }

    fn validate_spec(&self, spec: &PrimitiveSpec) -> Result<()> {
        let positions = &spec.positions;
        if positions.is_empty() || positions.len() % 3 != 0 {
            return Err(invalid(format!("positions must be a non-empty multiple of 3 floats, got {}", positions.len())));
        }
        finite("position", positions)?;
        let n = positions.len() / 3;

        if let Some(v) = &spec.normals {
            if v.len() != positions.len() {
                return Err(invalid(format!("normals have {} floats, expected {}", v.len(), positions.len())));
            }
            finite("normal", v)?;
        }
        if let Some(t) = &spec.tangents {
            if t.len() != n * 4 {
                return Err(invalid(format!("tangents have {} floats, expected {} (4 per vertex)", t.len(), n * 4)));
            }
            finite("tangent", t)?;
            if let Some(v) = (0..n).find(|&v| t[v * 4 + 3].abs() != 1.0) {
                return Err(invalid(format!("tangent {v} has w = {}, expected 1 or -1 (the handedness)", t[v * 4 + 3])));
            }
        }
        for (s, t) in spec.texcoords.iter().enumerate() {
            if t.len() != n * 2 {
                return Err(invalid(format!("texcoord set {s} has {} values, expected {}", t.len(), n * 2)));
            }
            if let TexCoords::F32(v) = t {
                finite(&format!("texcoord set {s}"), v)?;
            }
        }
        for (s, c) in spec.colors.iter().enumerate() {
            let (len, comps) = c.shape();
            if len != n * comps {
                return Err(invalid(format!("color set {s} has {len} values, expected {} ({comps} per vertex)", n * comps)));
            }
            if let ColorData::Rgb(v) | ColorData::Rgba(v) = c {
                finite(&format!("color set {s}"), v)?;
            }
        }

        let mode = spec.mode;
        let count = spec.indices.as_ref().map_or(n, |i| i.len());
        if !mode.is_valid_count(count) {
            let (what, what_n) = if spec.indices.is_some() { ("index count", "") } else { ("non-indexed geometry", " vertices") };
            let m = mode.count_multiple();
            let need = if m > 1 {
                format!("a non-empty multiple of {m}")
            } else {
                format!("at least {}", mode.min_count())
            };
            return Err(invalid(format!("{what} must be {need}{what_n} for {mode:?}, got {count}")));
        }
        if let Some(idx) = &spec.indices {
            let restart = idx.restart();
            for i in 0..idx.len() {
                let v = idx.get(i);
                if v == restart {
                    return Err(invalid(format!("index {i} is {v}, the primitive restart value, which glTF forbids")));
                }
                if v as usize >= n {
                    return Err(invalid(format!("index {i} is {v} but there are only {n} vertices")));
                }
            }
        }
        if let Some(m) = spec.material {
            let have = self.gltf.materials.as_ref().map_or(0, |m| m.len());
            if m >= have {
                return Err(invalid(format!("material {m} does not exist ({have} materials)")));
            }
        }
        if !spec.skin_sets.is_empty() {
            validate_skin_sets(n, &spec.skin_sets, spec.joint_count.unwrap_or(u16::MAX as usize + 1))?;
        }
        Ok(())
    }

    /// A [`PrimitiveMode::Points`] primitive: one point per vertex. `positions` is `x, y, z` per
    /// vertex. The `create_*` mode helpers are shortcuts for [`create_primitive`](Self::create_primitive)
    /// and share its errors and atomicity; indices are written as `u16`, or `u32` if needed.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// let points = b.create_points(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0], None).unwrap();
    /// assert_eq!(points.mode, Some(0));
    /// assert!(b.create_points(&[], None).is_err());
    /// ```
    pub fn create_points(&mut self, positions: &[f32], material: Option<usize>) -> Result<Primitive> {
        self.create_mode_primitive(PrimitiveMode::Points, positions, None, material)
    }

    /// A [`PrimitiveMode::Lines`] primitive: vertices (or indices) are paired into segments, so
    /// the count must be even.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// let pos = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0];
    /// assert!(b.create_lines(&pos, Some(&[0, 1, 1, 2]), None).is_ok());
    /// assert!(b.create_lines(&pos, None, None).is_err()); // 3 vertices cannot pair up
    /// ```
    pub fn create_lines(&mut self, positions: &[f32], indices: Option<&[u32]>, material: Option<usize>) -> Result<Primitive> {
        self.create_mode_primitive(PrimitiveMode::Lines, positions, indices, material)
    }

    /// A [`PrimitiveMode::LineStrip`] primitive, or a [`PrimitiveMode::LineLoop`] when `closed`
    /// (the last point connects back to the first). At least 2 vertices or indices.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// let pos = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0];
    /// assert_eq!(b.create_line_strip(&pos, true, None, None).unwrap().mode, Some(2));
    /// assert!(b.create_line_strip(&pos[..3], false, None, None).is_err());
    /// ```
    pub fn create_line_strip(&mut self, positions: &[f32], closed: bool, indices: Option<&[u32]>, material: Option<usize>) -> Result<Primitive> {
        let mode = if closed { PrimitiveMode::LineLoop } else { PrimitiveMode::LineStrip };
        self.create_mode_primitive(mode, positions, indices, material)
    }

    /// A [`PrimitiveMode::TriangleStrip`] primitive (at least 3 vertices or indices). Triangle
    /// winding alternates; see [`strip_to_triangles`] for the rule.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// let quad = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0];
    /// assert_eq!(b.create_triangle_strip(&quad, None, None).unwrap().mode, Some(5));
    /// assert!(b.create_triangle_strip(&quad, Some(&[0, 1]), None).is_err());
    /// ```
    pub fn create_triangle_strip(&mut self, positions: &[f32], indices: Option<&[u32]>, material: Option<usize>) -> Result<Primitive> {
        self.create_mode_primitive(PrimitiveMode::TriangleStrip, positions, indices, material)
    }

    /// A [`PrimitiveMode::TriangleFan`] primitive (at least 3 vertices or indices); every
    /// triangle shares the first vertex. See [`fan_to_triangles`].
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// let quad = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
    /// assert_eq!(b.create_triangle_fan(&quad, None, None).unwrap().mode, Some(6));
    /// assert!(b.create_triangle_fan(&quad[..6], None, None).is_err());
    /// ```
    pub fn create_triangle_fan(&mut self, positions: &[f32], indices: Option<&[u32]>, material: Option<usize>) -> Result<Primitive> {
        self.create_mode_primitive(PrimitiveMode::TriangleFan, positions, indices, material)
    }

    fn create_mode_primitive(&mut self, mode: PrimitiveMode, positions: &[f32], indices: Option<&[u32]>, material: Option<usize>) -> Result<Primitive> {
        self.create_primitive(PrimitiveSpec {
            positions: positions.to_vec(),
            indices: indices.map(|i| Indices::auto(i.to_vec())),
            material,
            mode,
            ..Default::default()
        })
    }

    /// Add a mesh made of `primitives`, checking each one against the document first, and set
    /// its `extras`.
    ///
    /// Unlike the unchecked `add_mesh`, this rejects an empty primitive list and any primitive
    /// with a dangling accessor or material, an invalid mode, an unknown attribute name, a wrong
    /// attribute type, mismatching attribute counts, an index count that does not fit the mode,
    /// or indices whose accessor `max` reaches the vertex count. The error names the primitive.
    /// Index *data* is checked by [`validate`](Self::validate). The document is unchanged on
    /// error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::{GltfBuilder, PrimitiveSpec};
    /// use serde_json::json;
    /// let mut b = GltfBuilder::new();
    /// let p = b.create_primitive(PrimitiveSpec { positions: vec![0.0; 9], ..Default::default() }).unwrap();
    /// let mesh = b.add_mesh_checked(Some("M".into()), vec![p], Some(json!({"k": 1}))).unwrap();
    /// assert_eq!(mesh, 0);
    /// assert!(b.add_mesh_checked(None, vec![], None).is_err());
    /// ```
    pub fn add_mesh_checked(&mut self, name: Option<String>, primitives: Vec<Primitive>, extras: Option<Value>) -> Result<usize> {
        if primitives.is_empty() {
            return Err(invalid("a mesh needs at least one primitive".to_string()));
        }
        let accessors = self.gltf.accessors.as_deref().unwrap_or(&[]);
        let material_count = self.gltf.materials.as_ref().map_or(0, |m| m.len());
        for (i, p) in primitives.iter().enumerate() {
            if let Some(issue) = check_primitive(p, accessors, material_count).into_iter().find(|x| !x.warning) {
                return Err(invalid(format!("{}: primitive {i} {}", issue.code, issue.msg)));
            }
            // Cheap data check from the accessor bounds: indices must stay below the vertex count.
            let vertices = p.attributes.values().next().and_then(|&a| accessors.get(a)).map(|a| a.count);
            let max = p.indices.and_then(|a| accessors.get(a)).and_then(|a| a.max.as_ref()).and_then(|m| m.first());
            if let (Some(v), Some(&m)) = (vertices, max) {
                if m >= v as f64 {
                    return Err(invalid(format!("PRIMITIVE_INDEX_OUT_OF_RANGE: primitive {i} has an index {m} but only {v} vertices")));
                }
            }
            for target in p.targets.iter().flatten().flat_map(|t| t.values()) {
                if *target >= accessors.len() {
                    return Err(invalid(format!("PRIMITIVE_ACCESSOR_INVALID: primitive {i} morph target references accessor {target}")));
                }
            }
        }
        let index = self.add_mesh(name, primitives);
        if let Some(e) = extras {
            self.set_extras(ExtensionTarget::Mesh(index), e)?;
        }
        Ok(index)
    }

    /// Set (replace) `mesh.extras`; a wrapper over [`set_extras`](Self::set_extras) with
    /// [`ExtensionTarget::Mesh`], including its `targetNames` handling.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the mesh does not exist. The document is unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// use serde_json::json;
    /// let mut b = GltfBuilder::new();
    /// let m = b.create_box(1.0);
    /// b.set_mesh_extras(m, json!({"layer": 3})).unwrap();
    /// assert!(b.set_mesh_extras(m + 1, json!({})).is_err());
    /// ```
    pub fn set_mesh_extras(&mut self, mesh: usize, extras: Value) -> Result<()> {
        self.set_extras(ExtensionTarget::Mesh(mesh), extras)
    }

    /// Set (replace) `extras` of primitive `primitive` of `mesh`; a wrapper over
    /// [`set_extras`](Self::set_extras) with [`ExtensionTarget::Primitive`].
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the mesh or primitive does not exist. The document is
    /// unchanged on error.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// use serde_json::json;
    /// let mut b = GltfBuilder::new();
    /// let m = b.create_box(1.0);
    /// b.set_primitive_extras(m, 0, json!({"lod": 0})).unwrap();
    /// assert!(b.set_primitive_extras(m, 1, json!({})).is_err());
    /// ```
    pub fn set_primitive_extras(&mut self, mesh: usize, primitive: usize, extras: Value) -> Result<()> {
        self.set_extras(ExtensionTarget::Primitive { mesh, primitive }, extras)
    }
}
