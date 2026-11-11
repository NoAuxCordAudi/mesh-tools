//! # Skin Builder Support
//!
//! This module provides skin-related functionality for the GltfBuilder.
//! Skins are used for skeletal animation in glTF, where a mesh is deformed
//! by a hierarchy of joints (bones).

use crate::accessor::{AccessorSpec, AccessorType};
use crate::builder::GltfBuilder;
use crate::models::{Primitive, Skin};
use crate::error::{GltfError, Result};
use mint::{ColumnMatrix4};

impl GltfBuilder {
    /// Add a skin to the glTF document
    ///
    /// A skin contains a list of joint node indices and their inverse bind matrices.
    /// The inverse bind matrices transform vertices from model space to joint space.
    ///
    /// `JOINTS_0` values in a skinned primitive index into `joints` (the *slot*), not
    /// into the node list.
    ///
    /// # Arguments
    ///
    /// * `name` - Optional name for the skin
    /// * `joints` - Node indices of the joints/bones; non-empty and unique
    /// * `inverse_bind_matrices_accessor` - Optional accessor of FLOAT `MAT4` inverse bind
    ///   matrices with at least one entry per joint
    /// * `skeleton` - Optional skeleton root. Must be a joint or an ancestor of every joint
    ///
    /// # Errors
    ///
    /// Returns `GltfError::InvalidInput` (with a message naming the offending index) if:
    /// - `joints` is empty or contains duplicates or nonexistent nodes
    /// - the joints do not share a common root node
    /// - `skeleton` does not exist or is not an ancestor-or-self of every joint
    /// - the accessor does not exist, is not FLOAT `MAT4`, or has fewer entries than joints
    ///
    /// Nothing is added to the document on error.
    ///
    /// # Returns
    ///
    /// The index of the created skin in the glTF document
    pub fn add_skin(
        &mut self,
        name: Option<String>,
        joints: Vec<usize>,
        inverse_bind_matrices_accessor: Option<usize>,
        skeleton: Option<usize>,
    ) -> Result<usize> {
        self.validate_skin_joints(&joints, skeleton)?;

        if let Some(acc_index) = inverse_bind_matrices_accessor {
            let accessor = self
                .gltf
                .accessors
                .as_ref()
                .and_then(|a| a.get(acc_index))
                .ok_or_else(|| GltfError::InvalidInput(format!(
                    "inverse bind matrix accessor {acc_index} does not exist"
                )))?;
            if accessor.type_ != "MAT4" || accessor.component_type != 5126 {
                return Err(GltfError::InvalidInput(format!(
                    "inverse bind matrix accessor {acc_index} must be FLOAT MAT4, found type {} component {}",
                    accessor.type_, accessor.component_type
                )));
            }
            if accessor.count < joints.len() {
                return Err(GltfError::InvalidInput(format!(
                    "inverse bind matrix accessor {acc_index} has {} entries for {} joints",
                    accessor.count,
                    joints.len()
                )));
            }
        }

        let skin = Skin {
            name,
            joints,
            inverse_bind_matrices: inverse_bind_matrices_accessor,
            skeleton,
            ..Default::default()
        };

        let skins = self.gltf.skins.get_or_insert_with(Vec::new);
        skins.push(skin);
        Ok(skins.len() - 1)
    }

    /// Add a skin and compute its inverse bind matrices from the current node hierarchy.
    ///
    /// The nodes are assumed to be in their bind pose right now. Build the joint hierarchy
    /// (parenting and transforms) *before* calling this. See
    /// [`compute_inverse_bind_matrices`](Self::compute_inverse_bind_matrices) for limits.
    ///
    /// Validates before writing, so nothing is added on error.
    pub fn add_skin_from_joints(
        &mut self,
        name: Option<String>,
        joints: Vec<usize>,
        skeleton: Option<usize>,
    ) -> Result<usize> {
        self.validate_skin_joints(&joints, skeleton)?;
        let matrices = self.compute_inverse_bind_matrices(&joints)?;
        let accessor = self.create_inverse_bind_matrices_accessor_from_f32(&matrices)?;
        self.add_skin(name, joints, Some(accessor), skeleton)
    }

    /// Compute the inverse bind matrix of each joint: the inverse of the joint's world
    /// transform in the current node hierarchy.
    ///
    /// Each matrix is returned as 16 floats in glTF column-major order (translation in
    /// elements 12..15). A node's local transform is its `matrix` if set, otherwise
    /// `translation * rotation * scale`. Matrices are assumed affine (bottom row 0,0,0,1).
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if a joint does not exist, the hierarchy contains a cycle,
    /// or a world transform is singular (for example a zero scale).
    pub fn compute_inverse_bind_matrices(&self, joints: &[usize]) -> Result<Vec<[f32; 16]>> {
        joints
            .iter()
            .map(|&joint| {
                let world = self.world_matrix(joint)?;
                invert_affine(&world).ok_or_else(|| GltfError::InvalidInput(format!(
                    "world transform of joint node {joint} is singular and cannot be inverted"
                )))
            })
            .collect()
    }

    /// World matrix of a node: product of local matrices from the root down to the node.
    fn world_matrix(&self, node: usize) -> Result<[f32; 16]> {
        let nodes = self.gltf.nodes.as_deref().unwrap_or(&[]);
        let mut world = IDENTITY;
        let mut current = Some(node);
        let mut steps = 0;
        while let Some(i) = current {
            let n = nodes.get(i).ok_or_else(|| {
                GltfError::InvalidInput(format!("node {i} does not exist"))
            })?;
            steps += 1;
            if steps > nodes.len() {
                return Err(GltfError::InvalidInput(format!(
                    "node hierarchy above node {node} contains a cycle"
                )));
            }
            let local = n.matrix.unwrap_or_else(|| {
                local_trs_matrix(
                    n.translation.unwrap_or([0.0; 3]),
                    n.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]),
                    n.scale.unwrap_or([1.0; 3]),
                )
            });
            // Walking up: world = parent_local * (world so far)
            world = mat_mul(&local, &world);
            current = self.node_parents.get(i).copied().flatten();
        }
        Ok(world)
    }

    /// Chain of nodes from `node` up to its root, `node` first. Errors on cycles.
    fn ancestors_or_self(&self, node: usize) -> Result<Vec<usize>> {
        let mut chain = vec![node];
        let mut current = self.node_parents.get(node).copied().flatten();
        while let Some(p) = current {
            if chain.len() > self.node_parents.len() {
                return Err(GltfError::InvalidInput(format!(
                    "node hierarchy above node {node} contains a cycle"
                )));
            }
            chain.push(p);
            current = self.node_parents.get(p).copied().flatten();
        }
        Ok(chain)
    }

    /// Validate the joint list and skeleton root of a skin (no mutation).
    fn validate_skin_joints(&self, joints: &[usize], skeleton: Option<usize>) -> Result<()> {
        if joints.is_empty() {
            return Err(GltfError::InvalidInput("skin must have at least one joint".to_string()));
        }
        let node_count = self.gltf.nodes.as_ref().map_or(0, |n| n.len());
        let mut seen = std::collections::HashSet::new();
        for &j in joints {
            if j >= node_count {
                return Err(GltfError::InvalidInput(format!(
                    "joint node {j} does not exist ({node_count} nodes)"
                )));
            }
            if !seen.insert(j) {
                return Err(GltfError::InvalidInput(format!("joint node {j} is listed more than once")));
            }
        }
        if let Some(s) = skeleton {
            if s >= node_count {
                return Err(GltfError::InvalidInput(format!(
                    "skeleton node {s} does not exist ({node_count} nodes)"
                )));
            }
        }

        let mut common_root = None;
        for &j in joints {
            let chain = self.ancestors_or_self(j)?;
            let root = *chain.last().expect("chain is never empty");
            match common_root {
                None => common_root = Some(root),
                Some(r) if r != root => {
                    return Err(GltfError::InvalidInput(format!(
                        "joints do not share a common root: joint node {j} is under root {root}, others under {r}"
                    )));
                }
                _ => {}
            }
            if let Some(s) = skeleton {
                if !chain.contains(&s) {
                    return Err(GltfError::InvalidInput(format!(
                        "skeleton node {s} is not an ancestor of (or equal to) joint node {j}"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Add a joint (bone) node to the glTF document
    ///
    /// Joint nodes are typically used in skeletal animation systems.
    /// They can have transforms and be organized in hierarchies.
    ///
    /// # Arguments
    ///
    /// * `name` - Optional name for the joint
    /// * `translation` - Optional translation relative to parent
    /// * `rotation` - Optional rotation quaternion [x, y, z, w]
    /// * `scale` - Optional scale factors
    ///
    /// # Returns
    ///
    /// The index of the created joint node
    pub fn add_joint_node(
        &mut self,
        name: Option<String>,
        translation: Option<[f32; 3]>,
        rotation: Option<[f32; 4]>,
        scale: Option<[f32; 3]>,
    ) -> usize {
        self.add_node(name, None, translation, rotation, scale)
    }

    /// Create an inverse bind matrices accessor from `mint` column-major matrices.
    ///
    /// `mint::ColumnMatrix4` fields `x`, `y`, `z`, `w` are the four *columns*, so the
    /// translation of a transform goes in `w.x`, `w.y`, `w.z` (not in each column's `.w`).
    /// See [`create_inverse_bind_matrices_accessor_from_f32`](Self::create_inverse_bind_matrices_accessor_from_f32).
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if `matrices` is empty.
    pub fn create_inverse_bind_matrices_accessor(&mut self, matrices: &[ColumnMatrix4<f32>]) -> Result<usize> {
        let flat: Vec<[f32; 16]> = matrices
            .iter()
            .map(|m| {
                let c = [m.x, m.y, m.z, m.w];
                let mut out = [0.0f32; 16];
                for (i, col) in c.iter().enumerate() {
                    out[i * 4..i * 4 + 4].copy_from_slice(&[col.x, col.y, col.z, col.w]);
                }
                out
            })
            .collect();
        self.create_inverse_bind_matrices_accessor_from_f32(&flat)
    }

    /// Create an inverse bind matrices accessor from raw glTF-layout matrices.
    ///
    /// Each matrix is 16 floats in column-major order: elements 0..4 are the first
    /// column, and the translation is elements 12..15.
    ///
    /// The buffer view has no `target`, as the spec forbids one on inverse bind matrix data.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if `matrices` is empty or contains a non-finite value.
    pub fn create_inverse_bind_matrices_accessor_from_f32(&mut self, matrices: &[[f32; 16]]) -> Result<usize> {
        if matrices.is_empty() {
            return Err(GltfError::InvalidInput("at least one inverse bind matrix is required".to_string()));
        }
        if let Some(i) = matrices.iter().position(|m| m.iter().any(|v| !v.is_finite())) {
            return Err(GltfError::InvalidInput(format!("inverse bind matrix {i} contains a non-finite value")));
        }

        let flat: Vec<f32> = matrices.iter().flatten().copied().collect();
        // No buffer view target: the spec forbids one on inverse bind matrix data.
        Ok(self.write_slice(&flat, AccessorType::Mat4, &AccessorSpec::new()))
    }

    /// Add a node with an associated skin
    ///
    /// This creates a node that references a mesh and applies skinning using the specified skin.
    /// The node goes through [`GltfBuilder::add_node`], so its rotation is normalized and it is
    /// registered for parent tracking like any other node.
    ///
    /// Note: per the glTF spec, the local transform of a skinned node is ignored by
    /// renderers; the joints' transforms drive the deformation. Pass `None` for
    /// `translation`, `rotation` and `scale` unless you have a reason not to.
    ///
    /// # Arguments
    ///
    /// * `name` - Optional name for the node
    /// * `mesh` - Index of the mesh to render
    /// * `skin` - Index of the skin to apply for deformation
    /// * `translation` - Optional translation
    /// * `rotation` - Optional rotation quaternion [x, y, z, w]
    /// * `scale` - Optional scale factors
    ///
    /// # Errors
    ///
    /// Returns `GltfError::InvalidIndex` if `mesh` or `skin` does not exist.
    ///
    /// # Returns
    ///
    /// The index of the created node
    pub fn add_skinned_node(
        &mut self,
        name: Option<String>,
        mesh: usize,
        skin: usize,
        translation: Option<[f32; 3]>,
        rotation: Option<[f32; 4]>,
        scale: Option<[f32; 3]>,
    ) -> crate::error::Result<usize> {
        let mesh_count = self.gltf.meshes.as_ref().map_or(0, |m| m.len());
        let skin_count = self.gltf.skins.as_ref().map_or(0, |s| s.len());
        if mesh >= mesh_count || skin >= skin_count {
            return Err(crate::error::GltfError::InvalidIndex);
        }

        let index = self.add_node(name, Some(mesh), translation, rotation, scale);
        if let Some(node) = self.gltf.nodes.as_mut().and_then(|n| n.get_mut(index)) {
            node.skin = Some(skin);
        }
        Ok(index)
    }

    /// Create a simple linear bone chain
    ///
    /// This utility method creates a simple chain of joints connected linearly.
    /// Useful for creating simple skeletal structures like arms, legs, or spines.
    ///
    /// # Arguments
    ///
    /// * `joint_names` - Names for each joint in the chain
    /// * `joint_positions` - Positions for each joint relative to parent
    ///
    /// # Returns
    ///
    /// Vector of joint node indices
    pub fn create_bone_chain(
        &mut self,
        joint_names: &[String],
        joint_positions: &[[f32; 3]],
    ) -> Result<Vec<usize>> {
        if joint_names.len() != joint_positions.len() {
            return Err(crate::error::GltfError::InvalidInput(
                "Joint names and positions must have the same length".to_string()
            ));
        }

        let mut joint_indices = Vec::new();

        for (i, (name, position)) in joint_names.iter().zip(joint_positions.iter()).enumerate() {
            let joint_index = self.add_joint_node(
                Some(name.clone()),
                Some(*position),
                None, // No rotation
                None, // No scale
            );

            // Add this joint as a child of the previous joint (except for the first one)
            if i > 0 {
                self.add_child_to_node(joint_indices[i - 1], joint_index)?;
            }

            joint_indices.push(joint_index);
        }

        Ok(joint_indices)
    }

    /// Create a skinned mesh with a single `JOINTS_0`/`WEIGHTS_0` set.
    ///
    /// Convenience wrapper over [`create_skinned_primitive`](Self::create_skinned_primitive)
    /// for `u16` joints and indices. Because no skin is known here, joint indices are not
    /// range-checked against a joint count; the other validation still applies.
    ///
    /// `joints` hold skin *slots* (positions in the skin's `joints` list), not node indices.
    ///
    /// # Errors
    ///
    /// See [`create_skinned_primitive`](Self::create_skinned_primitive). Nothing is added on error.
    ///
    /// # Returns
    ///
    /// The index of the created mesh
    pub fn create_skinned_mesh(
        &mut self,
        name: Option<String>,
        positions: &[f32],
        indices: &[u16],
        normals: Option<&[f32]>,
        texcoords: Option<&[f32]>,
        joints: &[[u16; 4]],
        weights: &[[f32; 4]],
        material: Option<usize>,
    ) -> Result<usize> {
        let indices: Vec<u32> = indices.iter().map(|&i| u32::from(i)).collect();
        let set = SkinSet {
            joints: JointIndices::U16(joints.to_vec()),
            weights: weights.to_vec(),
        };
        let primitive = self.create_skinned_primitive(
            positions,
            Some(&indices),
            normals,
            texcoords,
            &[set],
            u16::MAX as usize + 1,
            material,
        )?;
        Ok(self.add_mesh(name, vec![primitive]))
    }

    /// Create a triangle primitive with skinning attributes (`JOINTS_n` / `WEIGHTS_n`).
    ///
    /// # Arguments
    ///
    /// * `positions` - `x, y, z` per vertex
    /// * `indices` - triangle indices, or `None` for non-indexed geometry
    /// * `normals` / `texcoords` - optional, 3 / 2 floats per vertex
    /// * `skin_sets` - one or more influence sets; set `n` becomes `JOINTS_n`/`WEIGHTS_n`
    /// * `joint_count` - number of joints in the skin this primitive will be used with;
    ///   every joint index with a non-zero weight must be below it
    /// * `material` - optional material index
    ///
    /// Index width is chosen automatically: `u16` unless an index is 65535 (the primitive
    /// restart value, which glTF forbids as an index) or larger, then `u32`.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` (the message names the vertex or index at fault) if:
    /// - positions are empty, not a multiple of 3, or non-finite
    /// - normals/texcoords/sets don't match the vertex count
    /// - indices are out of range or not a multiple of 3 (or, non-indexed, the vertex count
    ///   is not a multiple of 3)
    /// - a weight is negative or non-finite, or a vertex's weights across all sets do not
    ///   sum to 1 within `1e-5` (see [`normalize_weights`])
    /// - a non-zero weight references a joint `>= joint_count`, or the same joint twice
    ///   on one vertex
    ///
    /// All validation runs before anything is written, so a failed call leaves the
    /// document untouched.
    pub fn create_skinned_primitive(
        &mut self,
        positions: &[f32],
        indices: Option<&[u32]>,
        normals: Option<&[f32]>,
        texcoords: Option<&[f32]>,
        skin_sets: &[SkinSet],
        joint_count: usize,
        material: Option<usize>,
    ) -> Result<Primitive> {
        if skin_sets.is_empty() {
            return Err(invalid("at least one skin set (JOINTS_0/WEIGHTS_0) is required".to_string()));
        }
        let texcoord_sets: Vec<&[f32]> = texcoords.into_iter().collect();
        self.create_primitive_checked(positions, indices, normals, &texcoord_sets, skin_sets, joint_count, material)
    }

    /// Validate, then write, a triangle primitive. `skin_sets` may be empty for an
    /// unskinned primitive; any number of texcoord sets (`TEXCOORD_n`) is supported.
    /// Shared by [`create_skinned_primitive`](Self::create_skinned_primitive) and
    /// `MeshBuilder::build_into`. Validates everything before writing anything.
    pub(crate) fn create_primitive_checked(
        &mut self,
        positions: &[f32],
        indices: Option<&[u32]>,
        normals: Option<&[f32]>,
        texcoord_sets: &[&[f32]],
        skin_sets: &[SkinSet],
        joint_count: usize,
        material: Option<usize>,
    ) -> Result<Primitive> {
        use crate::accessor::{AccessorSpec, AccessorType as T, Bounds, BufferViewTarget::{ArrayBuffer, ElementArrayBuffer}};
        use std::collections::HashMap;

        validate_skinned_geometry(positions, indices, normals, texcoord_sets, skin_sets, joint_count)?;

        // ---- All input is valid; from here on we only write. ----
        let mut attributes = HashMap::new();
        let vertex = AccessorSpec::new().with_target(ArrayBuffer);

        attributes.insert(
            "POSITION".to_string(),
            self.write_slice(positions, T::Vec3, &vertex.clone().with_bounds(Bounds::Compute)),
        );
        if let Some(n) = normals {
            attributes.insert("NORMAL".to_string(), self.write_slice(n, T::Vec3, &vertex));
        }
        for (n, t) in texcoord_sets.iter().enumerate() {
            attributes.insert(format!("TEXCOORD_{n}"), self.write_slice(t, T::Vec2, &vertex));
        }
        for (n, set) in skin_sets.iter().enumerate() {
            let joints = match &set.joints {
                JointIndices::U8(j) => {
                    let flat: Vec<u8> = j.iter().flatten().copied().collect();
                    self.write_slice(&flat, T::Vec4, &vertex)
                }
                JointIndices::U16(j) => {
                    let flat: Vec<u16> = j.iter().flatten().copied().collect();
                    self.write_slice(&flat, T::Vec4, &vertex)
                }
            };
            attributes.insert(format!("JOINTS_{n}"), joints);
            let weights: Vec<f32> = set.weights.iter().flatten().copied().collect();
            attributes.insert(format!("WEIGHTS_{n}"), self.write_slice(&weights, T::Vec4, &vertex));
        }

        let index_spec = AccessorSpec::new().with_target(ElementArrayBuffer);
        let index_accessor = indices.map(|idx| {
            // 65535 is the primitive-restart value and may not be used as an index.
            if idx.iter().all(|&i| i < 65535) {
                let narrow: Vec<u16> = idx.iter().map(|&i| i as u16).collect();
                self.write_slice(&narrow, T::Scalar, &index_spec)
            } else {
                self.write_slice(idx, T::Scalar, &index_spec)
            }
        });

        Ok(Primitive {
            attributes,
            indices: index_accessor,
            material,
            mode: Some(crate::constants::primitive_mode::TRIANGLES),
            ..Default::default()
        })
    }
}

/// Joint indices of one influence set (`JOINTS_n`): four *skin slots* per vertex.
///
/// A slot is a position in the skin's `joints` list, not a node index.
#[derive(Debug, Clone, PartialEq)]
pub enum JointIndices {
    U8(Vec<[u8; 4]>),
    U16(Vec<[u16; 4]>),
}

impl JointIndices {
    fn len(&self) -> usize {
        match self {
            JointIndices::U8(v) => v.len(),
            JointIndices::U16(v) => v.len(),
        }
    }

    fn get(&self, vertex: usize) -> [usize; 4] {
        match self {
            JointIndices::U8(v) => v[vertex].map(usize::from),
            JointIndices::U16(v) => v[vertex].map(usize::from),
        }
    }
}

/// One set of four joint influences per vertex (`JOINTS_n` + `WEIGHTS_n`).
#[derive(Debug, Clone, PartialEq)]
pub struct SkinSet {
    pub joints: JointIndices,
    /// Weight for each joint above, as `f32`. Across all sets, a vertex's weights sum to 1.
    pub weights: Vec<[f32; 4]>,
}

/// Maximum allowed deviation of a vertex's weight sum from 1.0.
const WEIGHT_SUM_TOLERANCE: f32 = 1e-5;

fn invalid(msg: String) -> GltfError {
    GltfError::InvalidInput(msg)
}

/// Validate everything about a skinned primitive without mutating anything.
/// Returns the vertex count.
fn validate_skinned_geometry(
    positions: &[f32],
    indices: Option<&[u32]>,
    normals: Option<&[f32]>,
    texcoord_sets: &[&[f32]],
    skin_sets: &[SkinSet],
    joint_count: usize,
) -> Result<usize> {
    if positions.is_empty() || positions.len() % 3 != 0 {
        return Err(invalid(format!(
            "positions must be a non-empty multiple of 3 floats, got {}", positions.len()
        )));
    }
    if let Some(i) = positions.iter().position(|v| !v.is_finite()) {
        return Err(invalid(format!("position value {i} is not finite")));
    }
    let vertex_count = positions.len() / 3;

    if let Some(n) = normals {
        if n.len() != positions.len() {
            return Err(invalid(format!("normals have {} floats, expected {}", n.len(), positions.len())));
        }
    }
    for (n, t) in texcoord_sets.iter().enumerate() {
        if t.len() != vertex_count * 2 {
            return Err(invalid(format!("texcoord set {n} has {} floats, expected {}", t.len(), vertex_count * 2)));
        }
    }

    match indices {
        Some(idx) => {
            if idx.is_empty() || idx.len() % 3 != 0 {
                return Err(invalid(format!("index count must be a non-empty multiple of 3, got {}", idx.len())));
            }
            if let Some((pos, &bad)) = idx.iter().enumerate().find(|(_, &i)| i as usize >= vertex_count) {
                return Err(invalid(format!("index {pos} is {bad} but there are only {vertex_count} vertices")));
            }
        }
        None if vertex_count % 3 != 0 => {
            return Err(invalid(format!("non-indexed geometry needs a multiple of 3 vertices, got {vertex_count}")));
        }
        None => {}
    }

    if skin_sets.is_empty() {
        return Ok(vertex_count); // unskinned primitive: nothing more to check
    }
    if joint_count == 0 {
        return Err(invalid("joint_count must be at least 1".to_string()));
    }
    for (n, set) in skin_sets.iter().enumerate() {
        if set.joints.len() != vertex_count || set.weights.len() != vertex_count {
            return Err(invalid(format!(
                "skin set {n}: {} joint entries and {} weight entries for {vertex_count} vertices",
                set.joints.len(), set.weights.len()
            )));
        }
    }

    for v in 0..vertex_count {
        let mut sum = 0.0f32;
        let mut used: Vec<usize> = Vec::new();
        for (n, set) in skin_sets.iter().enumerate() {
            let joints = set.joints.get(v);
            for (k, &w) in set.weights[v].iter().enumerate() {
                if !w.is_finite() || w < 0.0 {
                    return Err(invalid(format!("vertex {v}: weight {w} in set {n} slot {k} must be finite and >= 0")));
                }
                sum += w;
                if w > 0.0 {
                    let j = joints[k];
                    if j >= joint_count {
                        return Err(invalid(format!(
                            "vertex {v}: joint slot {j} in set {n} is out of range for a skin with {joint_count} joints \
                             (JOINTS_n holds skin slots, not node indices)"
                        )));
                    }
                    if used.contains(&j) {
                        return Err(invalid(format!("vertex {v}: joint slot {j} is influenced more than once")));
                    }
                    used.push(j);
                }
            }
        }
        if (sum - 1.0).abs() > WEIGHT_SUM_TOLERANCE {
            return Err(invalid(format!(
                "vertex {v}: weights sum to {sum}, expected 1.0 (see normalize_weights)"
            )));
        }
    }
    Ok(vertex_count)
}

/// Normalize each vertex's four weights so they sum to exactly 1.
///
/// This is an explicit opt-in: the skinned-primitive functions reject bad sums rather than
/// silently altering data.
///
/// # Errors
///
/// `GltfError::InvalidInput` naming the vertex if a weight is negative or non-finite, or
/// the weights sum to zero (there is nothing to normalize).
pub fn normalize_weights(weights: &mut [[f32; 4]]) -> Result<()> {
    for (v, w) in weights.iter().enumerate() {
        if w.iter().any(|x| !x.is_finite() || *x < 0.0) {
            return Err(invalid(format!("vertex {v}: weights must be finite and >= 0, got {w:?}")));
        }
        if w.iter().sum::<f32>() <= 0.0 {
            return Err(invalid(format!("vertex {v}: weights sum to zero and cannot be normalized")));
        }
    }
    for w in weights.iter_mut() {
        let sum: f32 = w.iter().sum();
        for x in w.iter_mut() {
            *x /= sum;
        }
    }
    Ok(())
}

/// Reduce arbitrarily many `(joint slot, weight)` influences per vertex to the four
/// strongest and renormalize them, as `JOINTS_0`/`WEIGHTS_0` data.
///
/// Influences are sorted by descending weight (ties keep the lower joint first); unused
/// slots are `(0, 0.0)`. This discards the weaker influences: it is lossy by design.
///
/// # Errors
///
/// `GltfError::InvalidInput` naming the vertex if a weight is negative or non-finite, or all
/// of its weights are zero (or it has no influences).
pub fn limit_influences(influences: &[Vec<(u16, f32)>]) -> Result<(Vec<[u16; 4]>, Vec<[f32; 4]>)> {
    let mut joints = Vec::with_capacity(influences.len());
    let mut weights = Vec::with_capacity(influences.len());
    for (v, list) in influences.iter().enumerate() {
        if list.iter().any(|(_, w)| !w.is_finite() || *w < 0.0) {
            return Err(invalid(format!("vertex {v}: weights must be finite and >= 0")));
        }
        let mut sorted: Vec<(u16, f32)> = list.iter().copied().filter(|(_, w)| *w > 0.0).collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
        sorted.truncate(4);
        let sum: f32 = sorted.iter().map(|(_, w)| w).sum();
        if sorted.is_empty() || sum <= 0.0 {
            return Err(invalid(format!("vertex {v}: has no positive influence weights")));
        }
        let mut j = [0u16; 4];
        let mut w = [0.0f32; 4];
        for (k, (joint, weight)) in sorted.iter().enumerate() {
            j[k] = *joint;
            w[k] = weight / sum;
        }
        joints.push(j);
        weights.push(w);
    }
    Ok((joints, weights))
}

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// Column-major 4x4 product `a * b`.
fn mat_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            out[col * 4 + row] = sum;
        }
    }
    out
}

/// Local matrix `T * R * S` in column-major order. `q` is normalized here.
fn local_trs_matrix(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> [f32; 16] {
    let [x, y, z, w] = GltfBuilder::normalize_quaternion(q);
    let r = [
        1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + z * w), 2.0 * (x * z - y * w),
        2.0 * (x * y - z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + x * w),
        2.0 * (x * z + y * w), 2.0 * (y * z - x * w), 1.0 - 2.0 * (x * x + y * y),
    ]; // three columns of the rotation matrix
    [
        r[0] * s[0], r[1] * s[0], r[2] * s[0], 0.0,
        r[3] * s[1], r[4] * s[1], r[5] * s[1], 0.0,
        r[6] * s[2], r[7] * s[2], r[8] * s[2], 0.0,
        t[0], t[1], t[2], 1.0,
    ]
}

/// Inverse of an affine column-major matrix, or `None` if it is singular or non-finite.
fn invert_affine(m: &[f32; 16]) -> Option<[f32; 16]> {
    // Upper-left 3x3 (columns c0,c1,c2), computed in f64 for accuracy.
    let a = |col: usize, row: usize| m[col * 4 + row] as f64;
    let (a00, a10, a20) = (a(0, 0), a(0, 1), a(0, 2));
    let (a01, a11, a21) = (a(1, 0), a(1, 1), a(1, 2));
    let (a02, a12, a22) = (a(2, 0), a(2, 1), a(2, 2));

    let c00 = a11 * a22 - a21 * a12;
    let c01 = a20 * a12 - a10 * a22;
    let c02 = a10 * a21 - a20 * a11;
    let det = a00 * c00 + a01 * c01 + a02 * c02;
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    // inverse[row][col] of the 3x3 (adjugate / det)
    let i00 = c00 * inv;
    let i01 = (a02 * a21 - a01 * a22) * inv;
    let i02 = (a01 * a12 - a02 * a11) * inv;
    let i10 = c01 * inv;
    let i11 = (a00 * a22 - a02 * a20) * inv;
    let i12 = (a02 * a10 - a00 * a12) * inv;
    let i20 = c02 * inv;
    let i21 = (a01 * a20 - a00 * a21) * inv;
    let i22 = (a00 * a11 - a01 * a10) * inv;

    let (tx, ty, tz) = (a(3, 0), a(3, 1), a(3, 2));
    let out = [
        i00, i10, i20, 0.0,
        i01, i11, i21, 0.0,
        i02, i12, i22, 0.0,
        -(i00 * tx + i01 * ty + i02 * tz),
        -(i10 * tx + i11 * ty + i12 * tz),
        -(i20 * tx + i21 * ty + i22 * tz),
        1.0,
    ];
    let out: [f32; 16] = out.map(|v| v as f32);
    out.iter().all(|v| v.is_finite()).then_some(out)
}
