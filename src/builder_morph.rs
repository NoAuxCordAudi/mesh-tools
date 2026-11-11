//! # Morph Target Builder Support
//!
//! Morph targets (blend shapes) store *deltas* relative to a primitive's base attributes. A
//! renderer computes `base + sum(weight_i * delta_i)` per vertex. This module attaches the
//! delta data to a primitive; weights, names and animation are handled elsewhere.
//!
//! Every target of every primitive of a mesh must be given in the same order, and all
//! primitives of one mesh must have the same number of targets.

use crate::builder::GltfBuilder;
use crate::constants::{buffer_view_target::ARRAY_BUFFER, component_type};
use crate::error::{GltfError, Result};
use crate::models::Primitive;
use std::collections::HashMap;

/// One morph target: per-vertex deltas for some of the base attributes.
///
/// Deltas are flat `x, y, z` triples, one per vertex, in the same vertex order as the base
/// primitive. A target must provide at least one attribute. Build one with
/// [`MorphTarget::new`] and the `with_*` methods.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct MorphTarget {
    /// Position deltas (`3 * vertex_count` floats).
    pub positions: Option<Vec<f32>>,
    /// Normal deltas (`3 * vertex_count` floats). Requires a base `NORMAL`.
    pub normals: Option<Vec<f32>>,
    /// Tangent deltas. Not supported yet: attaching a target that sets this is an error.
    pub tangents: Option<Vec<f32>>,
}

impl MorphTarget {
    /// An empty target. Add deltas with the `with_*` methods.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the position deltas.
    pub fn with_positions(mut self, deltas: Vec<f32>) -> Self {
        self.positions = Some(deltas);
        self
    }

    /// Set the normal deltas.
    pub fn with_normals(mut self, deltas: Vec<f32>) -> Self {
        self.normals = Some(deltas);
        self
    }

    /// Set the tangent deltas (rejected when attaching, for now).
    pub fn with_tangents(mut self, deltas: Vec<f32>) -> Self {
        self.tangents = Some(deltas);
        self
    }
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
    /// - a target has no deltas, or sets `tangents` (not supported yet)
    /// - a delta array is not exactly `3 * vertex_count` floats, or contains a non-finite value
    /// - a target has normal deltas but the primitive has no base `NORMAL`
    ///
    /// All validation runs before anything is written, so on error the document and the
    /// primitive are unchanged.
    ///
    /// Each delta accessor is FLOAT `VEC3` with an `ARRAY_BUFFER` buffer view. `POSITION`
    /// delta accessors carry `min`/`max` computed from the deltas.
    pub fn attach_morph_targets(&mut self, primitive: &mut Primitive, targets: &[MorphTarget]) -> Result<()> {
        if targets.is_empty() {
            return Err(invalid("at least one morph target is required".to_string()));
        }
        if primitive.targets.is_some() {
            return Err(invalid("the primitive already has morph targets".to_string()));
        }
        let vertex_count = self.base_position_count(primitive)?;
        let has_base_normals = primitive.attributes.contains_key("NORMAL");

        validate_targets(targets, vertex_count, has_base_normals)?;

        // ---- validated; write ----
        let mut maps = Vec::with_capacity(targets.len());
        for target in targets {
            let mut map = HashMap::new();
            if let Some(d) = &target.positions {
                let (min, max) = component_bounds(d);
                let bytes: Vec<u8> = d.iter().flat_map(|v| v.to_le_bytes()).collect();
                let acc = self.write_attribute(&bytes, ARRAY_BUFFER, component_type::FLOAT, vertex_count, "VEC3", Some(min), Some(max));
                map.insert("POSITION".to_string(), acc);
            }
            if let Some(d) = &target.normals {
                let bytes: Vec<u8> = d.iter().flat_map(|v| v.to_le_bytes()).collect();
                let acc = self.write_attribute(&bytes, ARRAY_BUFFER, component_type::FLOAT, vertex_count, "VEC3", None, None);
                map.insert("NORMAL".to_string(), acc);
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
pub(crate) fn validate_targets(targets: &[MorphTarget], vertex_count: usize, has_base_normals: bool) -> Result<()> {
    if targets.is_empty() {
        return Err(invalid("at least one morph target is required".to_string()));
    }
    for (t, target) in targets.iter().enumerate() {
        if target.positions.is_none() && target.normals.is_none() && target.tangents.is_none() {
            return Err(invalid(format!("morph target {t} has no deltas")));
        }
        if target.tangents.is_some() {
            return Err(invalid(format!(
                "morph target {t}: tangent deltas are not supported yet (base TANGENT attributes cannot be created)"
            )));
        }
        if target.normals.is_some() && !has_base_normals {
            return Err(invalid(format!(
                "morph target {t} has normal deltas but the primitive has no base NORMAL attribute"
            )));
        }
        for (name, deltas) in [("position", &target.positions), ("normal", &target.normals)] {
            let Some(d) = deltas else { continue };
            if d.len() != vertex_count * 3 {
                return Err(invalid(format!(
                    "morph target {t}: {name} deltas have {} floats, expected {} (3 per vertex, {vertex_count} vertices)",
                    d.len(),
                    vertex_count * 3
                )));
            }
            if let Some(i) = d.iter().position(|v| !v.is_finite()) {
                return Err(invalid(format!("morph target {t}: {name} delta value {i} is not finite")));
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

/// Per-component (x, y, z) minimum and maximum of flat xyz triples. Input is non-empty.
fn component_bounds(deltas: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut min = vec![f32::INFINITY; 3];
    let mut max = vec![f32::NEG_INFINITY; 3];
    for v in deltas.chunks_exact(3) {
        for c in 0..3 {
            min[c] = min[c].min(v[c]);
            max[c] = max[c].max(v[c]);
        }
    }
    (min, max)
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
