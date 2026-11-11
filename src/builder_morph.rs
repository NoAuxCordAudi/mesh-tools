//! # Morph Target Builder Support
//!
//! Morph targets (blend shapes) store *deltas* relative to a primitive's base attributes. A
//! renderer computes `base + sum(weight_i * delta_i)` per vertex. This module attaches the
//! delta data to a primitive; weights, names and animation are handled elsewhere.
//!
//! Every target of every primitive of a mesh must be given in the same order, and all
//! primitives of one mesh must have the same number of targets.

use crate::builder::GltfBuilder;
use crate::accessor::{AccessorSpec, AccessorType, Bounds, BufferViewTarget, SparseSpec};
use crate::error::{GltfError, Result};
use crate::models::Primitive;
use std::collections::HashMap;

/// Deltas of one morph attribute: a full array, or only the vertices that move.
///
/// Both forms decode to the same dense `3 * vertex_count` floats; sparse storage writes a glTF
/// sparse accessor (no `bufferView`, so unlisted vertices read as zero).
#[derive(Debug, Clone, PartialEq)]
pub enum MorphDelta {
    /// `x, y, z` per vertex.
    Dense(Vec<f32>),
    /// `values` holds `x, y, z` for each of the strictly increasing vertex `indices`.
    Sparse {
        /// Vertex indices that have a non-zero delta.
        indices: Vec<u32>,
        /// Three floats per index.
        values: Vec<f32>,
    },
}

impl MorphDelta {
    /// Absolute-to-delta helper: `morphed - base`, per float.
    ///
    /// `base` and `morphed` are flat `x, y, z` arrays of equal length. For tangents pass the
    /// `xyz` of the base tangent (drop `w`).
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the lengths differ, are not a multiple of 3, an input is not
    /// finite, or a difference overflows `f32`.
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::MorphDelta;
    /// let d = MorphDelta::from_absolute(&[1.0, 2.0, 3.0], &[1.5, 2.0, 1.0]).unwrap();
    /// assert_eq!(d, vec![0.5, 0.0, -2.0]);
    /// assert!(MorphDelta::from_absolute(&[0.0; 3], &[0.0; 6]).is_err());
    /// ```
    pub fn from_absolute(base: &[f32], morphed: &[f32]) -> Result<Vec<f32>> {
        if base.len() != morphed.len() {
            return Err(invalid(format!("base has {} floats but morphed has {}", base.len(), morphed.len())));
        }
        if base.len() % 3 != 0 {
            return Err(invalid(format!("{} floats is not a multiple of 3", base.len())));
        }
        let mut out = Vec::with_capacity(base.len());
        for (i, (b, m)) in base.iter().zip(morphed).enumerate() {
            let d = (*m as f64 - *b as f64) as f32;
            if !b.is_finite() || !m.is_finite() || !d.is_finite() {
                return Err(invalid(format!("value {i}: delta of {b} to {m} is not finite")));
            }
            out.push(d);
        }
        Ok(out)
    }

    /// Convert dense deltas to sparse by dropping vertices whose delta is zero.
    ///
    /// With `epsilon: None` only exact zeros (all three components) are dropped, so the result
    /// decodes to exactly `deltas`. With `Some(e)` a vertex is dropped when every component has
    /// `|c| <= e`: lossy, and only because the caller asked for it.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if `deltas.len()` is not a multiple of 3, a value is not finite,
    /// `epsilon` is negative or not finite, or nothing is left (a sparse accessor needs at least
    /// one entry; use no delta for that attribute instead).
    ///
    /// # Example
    ///
    /// ```
    /// use mesh_tools::MorphDelta;
    /// let dense = [0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.01, 0.0, 0.0];
    /// let exact = MorphDelta::sparse_from_dense(&dense, None).unwrap();
    /// assert_eq!(exact, MorphDelta::Sparse { indices: vec![1, 2], values: dense[3..].to_vec() });
    /// let lossy = MorphDelta::sparse_from_dense(&dense, Some(0.01)).unwrap();
    /// assert_eq!(lossy, MorphDelta::Sparse { indices: vec![1], values: vec![0.5, 0.0, 0.0] });
    /// assert!(MorphDelta::sparse_from_dense(&[0.0; 3], None).is_err());
    /// ```
    pub fn sparse_from_dense(deltas: &[f32], epsilon: Option<f32>) -> Result<MorphDelta> {
        if deltas.len() % 3 != 0 {
            return Err(invalid(format!("{} floats is not a multiple of 3", deltas.len())));
        }
        if let Some(i) = deltas.iter().position(|v| !v.is_finite()) {
            return Err(invalid(format!("delta value {i} is not finite")));
        }
        let eps = match epsilon {
            Some(e) if !(e.is_finite() && e >= 0.0) => {
                return Err(invalid(format!("epsilon {e} must be finite and >= 0")));
            }
            e => e.unwrap_or(0.0),
        };
        let (mut indices, mut values) = (Vec::new(), Vec::new());
        for (v, e) in deltas.chunks_exact(3).enumerate() {
            if e.iter().any(|c| c.abs() > eps) {
                indices.push(v as u32);
                values.extend_from_slice(e);
            }
        }
        if indices.is_empty() {
            return Err(invalid("all deltas are zero (or within epsilon): nothing to store sparsely".to_string()));
        }
        Ok(MorphDelta::Sparse { indices, values })
    }
}

/// One morph target: per-vertex deltas for some of the base attributes.
///
/// Deltas are `x, y, z` triples, one per vertex, in the same vertex order as the base
/// primitive; tangent deltas have no `w`. A target must provide at least one attribute. Build
/// one with [`MorphTarget::new`] and the `with_*` methods.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct MorphTarget {
    /// Position deltas.
    pub positions: Option<MorphDelta>,
    /// Normal deltas. Requires a base `NORMAL`.
    pub normals: Option<MorphDelta>,
    /// Tangent deltas (`xyz` only). Requires a base `TANGENT`.
    pub tangents: Option<MorphDelta>,
}

impl MorphTarget {
    /// An empty target. Add deltas with the `with_*` methods.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set dense position deltas (`3 * vertex_count` floats).
    pub fn with_positions(self, deltas: Vec<f32>) -> Self {
        self.with_position_delta(MorphDelta::Dense(deltas))
    }

    /// Set dense normal deltas.
    pub fn with_normals(self, deltas: Vec<f32>) -> Self {
        self.with_normal_delta(MorphDelta::Dense(deltas))
    }

    /// Set dense tangent deltas (VEC3; requires a base `TANGENT`).
    pub fn with_tangents(self, deltas: Vec<f32>) -> Self {
        self.with_tangent_delta(MorphDelta::Dense(deltas))
    }

    /// Set sparse position deltas: `values` is `x, y, z` per strictly increasing `indices` entry.
    pub fn with_sparse_positions(self, indices: Vec<u32>, values: Vec<f32>) -> Self {
        self.with_position_delta(MorphDelta::Sparse { indices, values })
    }

    /// Set sparse normal deltas (see [`with_sparse_positions`](Self::with_sparse_positions)).
    pub fn with_sparse_normals(self, indices: Vec<u32>, values: Vec<f32>) -> Self {
        self.with_normal_delta(MorphDelta::Sparse { indices, values })
    }

    /// Set sparse tangent deltas (see [`with_sparse_positions`](Self::with_sparse_positions)).
    pub fn with_sparse_tangents(self, indices: Vec<u32>, values: Vec<f32>) -> Self {
        self.with_tangent_delta(MorphDelta::Sparse { indices, values })
    }

    /// Set position deltas from a [`MorphDelta`], for example from
    /// [`MorphDelta::sparse_from_dense`].
    pub fn with_position_delta(mut self, delta: MorphDelta) -> Self {
        self.positions = Some(delta);
        self
    }

    /// Set normal deltas from a [`MorphDelta`].
    pub fn with_normal_delta(mut self, delta: MorphDelta) -> Self {
        self.normals = Some(delta);
        self
    }

    /// Set tangent deltas from a [`MorphDelta`].
    pub fn with_tangent_delta(mut self, delta: MorphDelta) -> Self {
        self.tangents = Some(delta);
        self
    }
}

/// What the base primitive offers to morph, for validation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MorphBase {
    pub vertex_count: usize,
    pub has_normals: bool,
    pub has_tangents: bool,
}

fn invalid(msg: String) -> GltfError {
    GltfError::InvalidInput(msg)
}

impl GltfBuilder {
    /// Attach morph targets to a primitive that has not been added to a mesh yet.
    ///
    /// Works on any primitive with a `POSITION` attribute, including those from
    /// [`create_skinned_primitive`](GltfBuilder::create_skinned_primitive), so a primitive can
    /// be both skinned and morphed. Add the primitive to a mesh afterwards with
    /// [`add_mesh`](GltfBuilder::add_mesh).
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` (naming the target at fault) if:
    /// - `targets` is empty, or the primitive already has targets
    /// - the primitive has no valid `POSITION` accessor
    /// - a target has no deltas
    /// - a dense delta array is not exactly `3 * vertex_count` floats, or contains a non-finite
    ///   value; sparse indices are empty, not strictly increasing, out of range, or the values
    ///   are not three per index or not finite
    /// - a target has normal (tangent) deltas but the primitive has no base `NORMAL` (`TANGENT`)
    ///
    /// All validation runs before anything is written, so on error the document and the
    /// primitive are unchanged.
    ///
    /// Each dense delta accessor is FLOAT `VEC3` with an `ARRAY_BUFFER` buffer view. A sparse
    /// one is a FLOAT `VEC3` sparse accessor without `bufferView`. `POSITION` delta accessors
    /// carry `min`/`max` of the dense (zero-filled) result.
    pub fn attach_morph_targets(&mut self, primitive: &mut Primitive, targets: &[MorphTarget]) -> Result<()> {
        if targets.is_empty() {
            return Err(invalid("at least one morph target is required".to_string()));
        }
        if primitive.targets.is_some() {
            return Err(invalid("the primitive already has morph targets".to_string()));
        }
        let vertex_count = self.base_position_count(primitive)?;
        let base = MorphBase {
            vertex_count,
            has_normals: primitive.attributes.contains_key("NORMAL"),
            has_tangents: primitive.attributes.contains_key("TANGENT"),
        };

        validate_targets(targets, base)?;

        // ---- validated; write ----
        let mut maps = Vec::with_capacity(targets.len());
        for target in targets {
            let mut map = HashMap::new();
            let spec = AccessorSpec::new().with_target(BufferViewTarget::ArrayBuffer);
            let attrs = [
                ("POSITION", &target.positions, spec.clone().with_bounds(Bounds::Compute)),
                ("NORMAL", &target.normals, spec.clone()),
                ("TANGENT", &target.tangents, spec),
            ];
            for (name, delta, spec) in attrs {
                let Some(delta) = delta else { continue };
                let acc = match delta {
                    MorphDelta::Dense(d) => self.write_slice(d, AccessorType::Vec3, &spec),
                    MorphDelta::Sparse { indices, values } => self.add_sparse_accessor(
                        vertex_count,
                        AccessorType::Vec3,
                        None,
                        SparseSpec::new(indices.clone(), values.clone()),
                        spec,
                    )?,
                };
                map.insert(name.to_string(), acc);
            }
            maps.push(map);
        }
        primitive.targets = Some(maps);
        Ok(())
    }

    /// Add a mesh whose primitives carry morph targets (see
    /// [`attach_morph_targets`](Self::attach_morph_targets)).
    ///
    /// - `default_weights`: initial weight of each target (`mesh.weights`); one per target.
    ///   Not range-checked: negative values and values above 1 are legal. When `None`, nothing
    ///   is written and viewers use zeros.
    /// - `target_names`: one name per target, stored under `mesh.extras.targetNames` (the
    ///   convention understood by Blender, three.js and others).
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if there are no primitives, a primitive has no morph targets,
    /// primitives disagree on the number of targets, or the weights/names count differs from the
    /// target count (or a weight is non-finite). The document is unchanged on error.
    ///
    /// # Returns
    ///
    /// The index of the created mesh
    pub fn add_morphable_mesh(
        &mut self,
        name: Option<String>,
        primitives: Vec<Primitive>,
        default_weights: Option<Vec<f32>>,
        target_names: Option<Vec<String>>,
    ) -> Result<usize> {
        let m = morph_target_count(&primitives)?;
        validate_weights_and_names(m, default_weights.as_deref(), target_names.as_deref())?;

        let index = self.add_mesh(name, primitives);
        let mesh = &mut self.gltf.meshes.as_mut().expect("add_mesh created the meshes array")[index];
        mesh.weights = default_weights;
        if let Some(n) = target_names {
            crate::builder_extensions::merge_target_names(&mut mesh.extras, n)
                .expect("a new mesh has no extras to conflict with");
        }
        Ok(index)
    }

    /// Set a node's morph target weights (`node.weights`), which override the mesh's default
    /// weights for this node.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the node does not exist, has no mesh, its mesh has no morph
    /// targets, `weights.len()` differs from the target count, or a weight is non-finite. Weights
    /// outside `[0, 1]` are allowed. The node is unchanged on error.
    pub fn set_node_weights(&mut self, node: usize, weights: Vec<f32>) -> Result<()> {
        let node_count = self.gltf.nodes.as_ref().map_or(0, |n| n.len());
        let mesh_index = self
            .gltf
            .nodes
            .as_ref()
            .and_then(|n| n.get(node))
            .ok_or_else(|| invalid(format!("node {node} does not exist ({node_count} nodes)")))?
            .mesh
            .ok_or_else(|| invalid(format!("node {node} has no mesh, so it has no morph targets")))?;
        let mesh = self
            .gltf
            .meshes
            .as_ref()
            .and_then(|m| m.get(mesh_index))
            .ok_or_else(|| invalid(format!("node {node} references mesh {mesh_index}, which does not exist")))?;
        let m = morph_target_count(&mesh.primitives)
            .map_err(|e| invalid(format!("node {node}'s mesh {mesh_index}: {e}")))?;
        validate_weights_and_names(m, Some(&weights), None).map_err(|e| invalid(format!("node {node}: {e}")))?;
        self.gltf.nodes.as_mut().unwrap()[node].weights = Some(weights);
        Ok(())
    }

    /// Vertex count of the primitive's base `POSITION` accessor.
    fn base_position_count(&self, primitive: &Primitive) -> Result<usize> {
        let acc = *primitive
            .attributes
            .get("POSITION")
            .ok_or_else(|| invalid("the primitive has no POSITION attribute to morph".to_string()))?;
        self.gltf
            .accessors
            .as_ref()
            .and_then(|a| a.get(acc))
            .map(|a| a.count)
            .ok_or_else(|| invalid(format!("the primitive's POSITION accessor {acc} does not exist")))
    }
}

/// Validate morph targets against a base primitive's vertex count and normals, without
/// touching any document. Shared by `attach_morph_targets` and `MeshBuilder::build_into` so the
/// latter can validate before writing the base primitive.
pub(crate) fn validate_targets(targets: &[MorphTarget], base: MorphBase) -> Result<()> {
    let vertex_count = base.vertex_count;
    // A hostile accessor count must not overflow the expected length.
    let dense_len = vertex_count.checked_mul(3).ok_or_else(|| invalid(format!("vertex count {vertex_count} is too large")))?;
    if targets.is_empty() {
        return Err(invalid("at least one morph target is required".to_string()));
    }
    for (t, target) in targets.iter().enumerate() {
        if target.positions.is_none() && target.normals.is_none() && target.tangents.is_none() {
            return Err(invalid(format!("morph target {t} has no deltas")));
        }
        if target.normals.is_some() && !base.has_normals {
            return Err(invalid(format!(
                "morph target {t} has normal deltas but the primitive has no base NORMAL attribute"
            )));
        }
        if target.tangents.is_some() && !base.has_tangents {
            return Err(invalid(format!(
                "morph target {t} has tangent deltas but the primitive has no base TANGENT attribute"
            )));
        }
        let attrs = [("position", &target.positions), ("normal", &target.normals), ("tangent", &target.tangents)];
        for (name, delta) in attrs {
            match delta {
                None => {}
                Some(MorphDelta::Dense(d)) => {
                    if d.len() != dense_len {
                        return Err(invalid(format!(
                            "morph target {t}: {name} deltas have {} floats, expected {} (3 per vertex, {vertex_count} vertices)",
                            d.len(),
                            dense_len
                        )));
                    }
                    if let Some(i) = d.iter().position(|v| !v.is_finite()) {
                        return Err(invalid(format!("morph target {t}: {name} delta value {i} is not finite")));
                    }
                }
                Some(MorphDelta::Sparse { indices, values }) => {
                    let at = format!("morph target {t}: sparse {name} deltas");
                    if indices.is_empty() {
                        return Err(invalid(format!("{at} need at least one entry")));
                    }
                    if let Some(w) = indices.windows(2).position(|w| w[0] >= w[1]) {
                        return Err(invalid(format!(
                            "{at}: indices must be strictly increasing, but index {} ({}) is not above {}",
                            w + 1, indices[w + 1], indices[w]
                        )));
                    }
                    let last = indices[indices.len() - 1];
                    if last as usize >= vertex_count {
                        return Err(invalid(format!("{at}: index {last} is out of range for {vertex_count} vertices")));
                    }
                    if values.len() != indices.len() * 3 {
                        return Err(invalid(format!(
                            "{at}: {} values for {} indices, expected {}", values.len(), indices.len(), indices.len() * 3
                        )));
                    }
                    if let Some(i) = values.iter().position(|v| !v.is_finite()) {
                        return Err(invalid(format!("{at}: value {i} is not finite")));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Validate default weights and target names against the target count `m`.
/// Weights are not range-checked: negative values and values above 1 are legal glTF.
pub(crate) fn validate_weights_and_names(m: usize, weights: Option<&[f32]>, names: Option<&[String]>) -> Result<()> {
    if let Some(w) = weights {
        if w.len() != m {
            return Err(invalid(format!("{} default weights for {m} morph targets", w.len())));
        }
        if let Some(i) = w.iter().position(|v| !v.is_finite()) {
            return Err(invalid(format!("default weight {i} is not finite")));
        }
    }
    if let Some(n) = names {
        if n.len() != m {
            return Err(invalid(format!("{} target names for {m} morph targets", n.len())));
        }
    }
    Ok(())
}

/// Number of morph targets shared by all primitives. Errors if there are no primitives, a
/// primitive has no targets, or primitives disagree.
pub(crate) fn morph_target_count(primitives: &[Primitive]) -> Result<usize> {
    if primitives.is_empty() {
        return Err(invalid("a morphable mesh needs at least one primitive".to_string()));
    }
    let mut count = None;
    for (i, p) in primitives.iter().enumerate() {
        let n = p.targets.as_ref().map_or(0, |t| t.len());
        if n == 0 {
            return Err(invalid(format!("primitive {i} has no morph targets")));
        }
        match count {
            None => count = Some(n),
            Some(c) if c != n => {
                return Err(invalid(format!(
                    "primitives disagree on the number of morph targets: primitive 0 has {c}, primitive {i} has {n}"
                )));
            }
            _ => {}
        }
    }
    Ok(count.expect("primitives is non-empty"))
}
