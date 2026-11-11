//! Scenes (default scene, checked roots) and node transforms (matrix or TRS).
//!
//! A node transform is either a `matrix` or translation/rotation/scale, never both. The setters
//! here refuse to mix them and never clear one form on behalf of the caller. Matrices must be
//! finite, affine and invertible; a negative determinant (mirror) is legal. Shear is allowed
//! but reported by the validator as a warning, because glTF only requires the matrix to be
//! decomposable to TRS.

use crate::builder::GltfBuilder;
use crate::error::{GltfError, Result};

/// Relative tolerance below which a matrix counts as singular (`|det| <= tol * |c0||c1||c2|`).
const SINGULAR_TOLERANCE: f64 = 1e-6;
/// Cosine of the angle between two matrix columns above which the matrix has shear.
pub(crate) const SHEAR_TOLERANCE: f64 = 1e-4;
/// Allowed deviation of a quaternion's length from 1.
pub(crate) const UNIT_TOLERANCE: f64 = 5e-6;

fn invalid(msg: String) -> GltfError {
    GltfError::InvalidInput(msg)
}

/// Columns of the upper-left 3x3 of a column-major matrix.
fn columns(m: &[f32; 16]) -> [[f64; 3]; 3] {
    let c = |i: usize| [m[i * 4] as f64, m[i * 4 + 1] as f64, m[i * 4 + 2] as f64];
    [c(0), c(1), c(2)]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// First rule a matrix breaks as `(code, message)`: finite, affine bottom row, invertible.
pub(crate) fn matrix_problem(m: &[f32; 16]) -> Option<(&'static str, String)> {
    if let Some(i) = m.iter().position(|v| !v.is_finite()) {
        return Some(("NODE_MATRIX_NOT_FINITE", format!("matrix element {i} is not finite")));
    }
    if [m[3], m[7], m[11], m[15]] != [0.0, 0.0, 0.0, 1.0] {
        return Some((
            "NODE_MATRIX_NOT_AFFINE",
            format!("matrix bottom row is [{}, {}, {}, {}], expected [0, 0, 0, 1]", m[3], m[7], m[11], m[15]),
        ));
    }
    let [c0, c1, c2] = columns(m);
    let cross = [c1[1] * c2[2] - c1[2] * c2[1], c1[2] * c2[0] - c1[0] * c2[2], c1[0] * c2[1] - c1[1] * c2[0]];
    let det = dot(c0, cross);
    let volume = dot(c0, c0).sqrt() * dot(c1, c1).sqrt() * dot(c2, c2).sqrt();
    if volume == 0.0 || det.abs() <= SINGULAR_TOLERANCE * volume {
        return Some((
            "NODE_MATRIX_NOT_DECOMPOSABLE",
            format!("matrix is singular (determinant {det:e}) and cannot be decomposed to TRS"),
        ));
    }
    None
}

/// True if two columns of the (valid) matrix are not orthogonal, i.e. it has shear.
pub(crate) fn matrix_has_shear(m: &[f32; 16]) -> bool {
    let c = columns(m);
    [(0, 1), (0, 2), (1, 2)].iter().any(|&(i, j)| {
        let d = dot(c[i], c[j]).abs();
        d > SHEAR_TOLERANCE * (dot(c[i], c[i]) * dot(c[j], c[j])).sqrt()
    })
}

fn check_matrix(m: &[f32; 16]) -> Result<()> {
    match matrix_problem(m) {
        Some((code, msg)) => Err(invalid(format!("{code}: {msg}"))),
        None => Ok(()),
    }
}

impl GltfBuilder {
    /// Make `scene` the default scene (the glTF root `scene` property).
    ///
    /// # Errors
    ///
    /// `InvalidInput` if the scene does not exist; nothing changes.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let mut b = GltfBuilder::new();
    /// let n = b.add_node(None, None, None, None, None);
    /// b.add_scene(Some("A".into()), Some(vec![n]));
    /// b.add_scene(Some("B".into()), None);
    /// b.set_default_scene(1).unwrap();
    /// assert_eq!(b.gltf.scene, Some(1));
    /// assert!(b.set_default_scene(2).is_err());
    /// ```
    pub fn set_default_scene(&mut self, scene: usize) -> Result<()> {
        let count = self.gltf.scenes.as_ref().map_or(0, Vec::len);
        if scene >= count {
            return Err(invalid(format!("scene {scene} does not exist ({count} scenes present)")));
        }
        self.gltf.scene = Some(scene);
        Ok(())
    }

    /// Remove the default scene. The validator then warns `NO_DEFAULT_SCENE` if scenes exist.
    /// Adding more scenes afterwards does not select one again.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let mut b = GltfBuilder::new();
    /// b.add_scene(None, None);
    /// b.clear_default_scene();
    /// assert_eq!(b.gltf.scene, None);
    /// ```
    pub fn clear_default_scene(&mut self) -> &mut Self {
        self.gltf.scene = None;
        self
    }

    /// Add a scene after checking its root nodes: at least one, all existing, no duplicates and
    /// none that is a child of another node. Like [`GltfBuilder::add_scene`], the first scene
    /// becomes the default.
    ///
    /// # Errors
    ///
    /// `InvalidInput` naming the offending node; nothing is added.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let mut b = GltfBuilder::new();
    /// let n = b.add_node(None, None, None, None, None);
    /// assert_eq!(b.add_scene_checked(None, vec![n]).unwrap(), 0);
    /// assert!(b.add_scene_checked(None, vec![n, n]).is_err());
    /// assert!(b.add_scene_checked(None, vec![]).is_err());
    /// ```
    pub fn add_scene_checked(&mut self, name: Option<String>, nodes: Vec<usize>) -> Result<usize> {
        if nodes.is_empty() {
            return Err(invalid("a scene needs at least one node (use add_scene for an empty scene)".into()));
        }
        for (i, &n) in nodes.iter().enumerate() {
            self.check_scene_root(n)?;
            if nodes[..i].contains(&n) {
                return Err(invalid(format!("node {n} is listed twice")));
            }
        }
        Ok(self.add_scene(name, Some(nodes)))
    }

    /// Append a root node to an existing scene.
    ///
    /// # Errors
    ///
    /// `InvalidInput` if the scene or node does not exist, the node is a child of another node,
    /// or the scene already lists it; nothing changes.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let mut b = GltfBuilder::new();
    /// let a = b.add_node(None, None, None, None, None);
    /// let c = b.add_node(None, None, None, None, None);
    /// let s = b.add_scene_checked(None, vec![a]).unwrap();
    /// b.add_node_to_scene(s, c).unwrap();
    /// assert!(b.add_node_to_scene(s, c).is_err()); // already listed
    /// ```
    pub fn add_node_to_scene(&mut self, scene: usize, node: usize) -> Result<()> {
        let count = self.gltf.scenes.as_ref().map_or(0, Vec::len);
        if scene >= count {
            return Err(invalid(format!("scene {scene} does not exist ({count} scenes present)")));
        }
        self.check_scene_root(node)?;
        let s = &mut self.gltf.scenes.as_mut().expect("scene count checked")[scene];
        if s.nodes.as_ref().is_some_and(|l| l.contains(&node)) {
            return Err(invalid(format!("scene {scene} already lists node {node}")));
        }
        s.nodes.get_or_insert_with(Vec::new).push(node);
        Ok(())
    }

    fn check_scene_root(&self, node: usize) -> Result<()> {
        self.check_node(node)?;
        match self.node_parents.get(node).copied().flatten() {
            Some(p) => Err(invalid(format!("node {node} is a child of node {p} and cannot be a scene root"))),
            None => Ok(()),
        }
    }

    fn check_node(&self, node: usize) -> Result<()> {
        let count = self.gltf.nodes.as_ref().map_or(0, Vec::len);
        if node >= count {
            return Err(invalid(format!("node {node} does not exist ({count} nodes present)")));
        }
        Ok(())
    }

    /// Local matrix `T * R * S` in glTF column-major order (translation in elements 12..15).
    /// The quaternion `[x, y, z, w]` is normalized first; a zero quaternion becomes identity.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let m = GltfBuilder::matrix_from_trs([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], [2.0, 2.0, 2.0]);
    /// assert_eq!(m, [2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 1.0, 2.0, 3.0, 1.0]);
    /// ```
    pub fn matrix_from_trs(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> [f32; 16] {
        let [x, y, z, w] = Self::normalize_quaternion(q);
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

    /// Add a node whose transform is a column-major `matrix` (no TRS).
    ///
    /// # Errors
    ///
    /// `InvalidInput` if the mesh does not exist or the matrix is not finite
    /// (`NODE_MATRIX_NOT_FINITE`), not affine (`NODE_MATRIX_NOT_AFFINE`) or singular
    /// (`NODE_MATRIX_NOT_DECOMPOSABLE`); nothing is added. A negative determinant is legal.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let mut b = GltfBuilder::new();
    /// let m = GltfBuilder::matrix_from_trs([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], [2.0; 3]);
    /// let n = b.add_node_with_matrix(None, None, m).unwrap();
    /// assert_eq!(b.gltf.nodes.as_ref().unwrap()[n].matrix, Some(m));
    /// assert!(b.add_node_with_matrix(None, None, [0.0; 16]).is_err());
    /// ```
    pub fn add_node_with_matrix(&mut self, name: Option<String>, mesh: Option<usize>, matrix: [f32; 16]) -> Result<usize> {
        if let Some(m) = mesh {
            let count = self.gltf.meshes.as_ref().map_or(0, Vec::len);
            if m >= count {
                return Err(invalid(format!("mesh {m} does not exist ({count} meshes present)")));
            }
        }
        check_matrix(&matrix)?;
        let index = self.add_node(name, mesh, None, None, None);
        self.gltf.nodes.as_mut().expect("node just added")[index].matrix = Some(matrix);
        Ok(index)
    }

    /// Give an existing node a `matrix`.
    ///
    /// # Errors
    ///
    /// `InvalidInput` if the node does not exist, already has translation/rotation/scale, is
    /// the target of a translation/rotation/scale animation channel (weights channels are fine),
    /// or the matrix is invalid (see [`GltfBuilder::add_node_with_matrix`]). Nothing is changed
    /// or cleared on error.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let mut b = GltfBuilder::new();
    /// let plain = b.add_node(None, None, None, None, None);
    /// let moved = b.add_node(None, None, Some([1.0, 0.0, 0.0]), None, None);
    /// let m = GltfBuilder::matrix_from_trs([0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
    /// b.set_node_matrix(plain, m).unwrap();
    /// assert!(b.set_node_matrix(moved, m).is_err()); // has TRS
    /// ```
    pub fn set_node_matrix(&mut self, node: usize, matrix: [f32; 16]) -> Result<()> {
        self.check_node(node)?;
        let n = &self.gltf.nodes.as_ref().expect("node checked")[node];
        if n.translation.is_some() || n.rotation.is_some() || n.scale.is_some() {
            return Err(invalid(format!("node {node} has translation/rotation/scale; a node cannot have both a matrix and TRS")));
        }
        for (ai, a) in self.gltf.animations.iter().flatten().enumerate() {
            for c in a.channels.iter().flatten() {
                if c.target.node == node && c.target.path != "weights" {
                    return Err(invalid(format!(
                        "animation {ai} animates the {} of node {node}; a matrix node cannot be TRS animated",
                        c.target.path
                    )));
                }
            }
        }
        check_matrix(&matrix)?;
        self.gltf.nodes.as_mut().expect("node checked")[node].matrix = Some(matrix);
        Ok(())
    }

    /// Remove a node's `matrix` (the node then has the identity transform). A node without a
    /// matrix is left as is.
    ///
    /// # Errors
    ///
    /// `InvalidInput` if the node does not exist.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let mut b = GltfBuilder::new();
    /// let n = b.add_node_with_matrix(None, None, [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, 0.0, 0.0, 1.0]).unwrap();
    /// b.clear_node_matrix(n).unwrap();
    /// assert!(b.gltf.nodes.as_ref().unwrap()[n].matrix.is_none());
    /// assert!(b.clear_node_matrix(9).is_err());
    /// ```
    pub fn clear_node_matrix(&mut self, node: usize) -> Result<()> {
        self.check_node(node)?;
        self.gltf.nodes.as_mut().expect("node checked")[node].matrix = None;
        Ok(())
    }

    /// Set translation, rotation and scale of an existing node. The quaternion is normalized.
    ///
    /// # Errors
    ///
    /// `InvalidInput` if the node does not exist, has a `matrix` (call
    /// [`GltfBuilder::clear_node_matrix`] first), or a value is not finite or the rotation is
    /// the zero quaternion (tiny or huge quaternions are normalized exactly). Nothing is changed on error.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let mut b = GltfBuilder::new();
    /// let n = b.add_node(None, None, None, None, None);
    /// b.set_node_transform(n, [1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 2.0], [1.0; 3]).unwrap();
    /// assert_eq!(b.gltf.nodes.as_ref().unwrap()[n].rotation, Some([0.0, 0.0, 0.0, 1.0]));
    /// assert!(b.set_node_transform(n, [0.0; 3], [0.0; 4], [1.0; 3]).is_err());
    /// ```
    pub fn set_node_transform(&mut self, node: usize, translation: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> Result<()> {
        self.check_node(node)?;
        if self.gltf.nodes.as_ref().expect("node checked")[node].matrix.is_some() {
            return Err(invalid(format!("node {node} has a matrix; call clear_node_matrix first")));
        }
        if !translation.iter().all(|v| v.is_finite()) {
            return Err(invalid(format!("translation {translation:?} is not finite")));
        }
        if !scale.iter().all(|v| v.is_finite()) {
            return Err(invalid(format!("scale {scale:?} is not finite")));
        }
        if !rotation.iter().all(|v| v.is_finite()) || rotation.iter().all(|&v| v == 0.0) {
            return Err(invalid(format!("rotation {rotation:?} must be finite and non-zero")));
        }
        // Normalize in f64: f32 squares underflow/overflow for tiny or huge (but valid) quaternions.
        let mag = rotation.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>().sqrt();
        let rotation = rotation.map(|v| (v as f64 / mag) as f32);
        let n = &mut self.gltf.nodes.as_mut().expect("node checked")[node];
        n.translation = Some(translation);
        n.rotation = Some(rotation);
        n.scale = Some(scale);
        Ok(())
    }
}
