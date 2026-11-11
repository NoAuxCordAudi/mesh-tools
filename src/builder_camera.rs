//! Cameras and node camera references.
//!
//! Cameras are checked on creation with the same rules the validator applies to a whole
//! document, and a failed call leaves the document unchanged. See `docs/CAMERAS.md`.

use crate::builder::GltfBuilder;
use crate::error::{GltfError, Result};
use crate::models::{Camera, Orthographic, Perspective};

fn invalid(msg: String) -> GltfError {
    GltfError::InvalidInput(msg)
}

/// True for a finite value greater than 0.
pub(crate) fn positive(v: f32) -> bool {
    v.is_finite() && v > 0.0
}

impl GltfBuilder {
    /// Add a perspective camera and return its index.
    ///
    /// `yfov` is the vertical field of view in radians, `zfar: None` gives an infinite far plane
    /// and `aspect_ratio: None` leaves the aspect ratio to the viewport. A `yfov` of pi or more
    /// is accepted but reported by [`validate`](Self::validate) as `CAMERA_YFOV_TOO_LARGE`.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if `yfov`, `znear` or `aspect_ratio` is not finite and greater
    /// than 0, or `zfar` is not finite and greater than `znear`. The document is unchanged.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let mut b = GltfBuilder::new();
    /// let cam = b.add_perspective_camera(Some("Main".into()), 0.8, 0.1, Some(100.0), Some(1.5)).unwrap();
    /// assert_eq!(cam, 0);
    /// // zfar must lie beyond znear.
    /// assert!(b.add_perspective_camera(None, 0.8, 1.0, Some(1.0), None).is_err());
    /// assert_eq!(b.gltf.cameras.as_ref().unwrap().len(), 1);
    /// ```
    pub fn add_perspective_camera(
        &mut self,
        name: Option<String>,
        yfov: f32,
        znear: f32,
        zfar: Option<f32>,
        aspect_ratio: Option<f32>,
    ) -> Result<usize> {
        if !positive(yfov) {
            return Err(invalid(format!("CAMERA_YFOV_INVALID: yfov {yfov} must be finite and > 0")));
        }
        if !positive(znear) {
            return Err(invalid(format!("CAMERA_ZNEAR_INVALID: znear {znear} must be finite and > 0")));
        }
        if let Some(a) = aspect_ratio.filter(|&a| !positive(a)) {
            return Err(invalid(format!("CAMERA_ASPECT_RATIO_INVALID: aspect ratio {a} must be finite and > 0")));
        }
        if let Some(z) = zfar.filter(|&z| !(z.is_finite() && z > znear)) {
            return Err(invalid(format!("CAMERA_ZFAR_INVALID: zfar {z} must be finite and > znear {znear}")));
        }
        Ok(self.push_camera(Camera {
            name,
            type_: "perspective".into(),
            perspective: Some(Perspective { aspect_ratio, yfov, zfar, znear, ..Default::default() }),
            ..Default::default()
        }))
    }

    /// Add an orthographic camera and return its index.
    ///
    /// `xmag` and `ymag` are the half extents of the view volume. They must not be zero; a
    /// negative value is accepted (it mirrors the view) but reported by
    /// [`validate`](Self::validate) as `CAMERA_MAG_NEGATIVE`. `znear` may be 0.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if `xmag` or `ymag` is zero or not finite, `znear` is negative
    /// or not finite, or `zfar` is not finite and greater than `znear`. The document is unchanged.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let mut b = GltfBuilder::new();
    /// let cam = b.add_orthographic_camera(None, 2.0, 1.0, 0.0, 50.0).unwrap();
    /// assert_eq!(cam, 0);
    /// assert!(b.add_orthographic_camera(None, 0.0, 1.0, 0.0, 50.0).is_err());
    /// ```
    pub fn add_orthographic_camera(
        &mut self,
        name: Option<String>,
        xmag: f32,
        ymag: f32,
        znear: f32,
        zfar: f32,
    ) -> Result<usize> {
        for (code, what, v) in [("CAMERA_XMAG_INVALID", "xmag", xmag), ("CAMERA_YMAG_INVALID", "ymag", ymag)] {
            if !v.is_finite() || v == 0.0 {
                return Err(invalid(format!("{code}: {what} {v} must be finite and not 0")));
            }
        }
        if !(znear.is_finite() && znear >= 0.0) {
            return Err(invalid(format!("CAMERA_ZNEAR_INVALID: znear {znear} must be finite and >= 0")));
        }
        if !(zfar.is_finite() && zfar > znear) {
            return Err(invalid(format!("CAMERA_ZFAR_INVALID: zfar {zfar} must be finite and > znear {znear}")));
        }
        Ok(self.push_camera(Camera {
            name,
            type_: "orthographic".into(),
            orthographic: Some(Orthographic { xmag, ymag, zfar, znear, ..Default::default() }),
            ..Default::default()
        }))
    }

    fn push_camera(&mut self, camera: Camera) -> usize {
        let cameras = self.gltf.cameras.get_or_insert_with(Vec::new);
        cameras.push(camera);
        cameras.len() - 1
    }

    /// Attach camera `camera` to `node`.
    ///
    /// Like the other node setters this never replaces silently: a node that already has a
    /// camera is an error, and [`clear_node_camera`](Self::clear_node_camera) is the explicit
    /// way to detach it.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the node or camera does not exist, or the node already has a
    /// camera. The document is unchanged.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    ///
    /// let mut b = GltfBuilder::new();
    /// let c = b.add_perspective_camera(None, 1.0, 0.1, None, None).unwrap();
    /// let n = b.add_node(None, None, None, None, None);
    /// b.set_node_camera(n, c).unwrap();
    /// assert!(b.set_node_camera(n, c).is_err()); // already set
    /// b.clear_node_camera(n).unwrap();
    /// assert!(b.set_node_camera(n, 7).is_err()); // no such camera
    /// ```
    pub fn set_node_camera(&mut self, node: usize, camera: usize) -> Result<()> {
        let cameras = self.gltf.cameras.as_ref().map_or(0, Vec::len);
        if camera >= cameras {
            return Err(invalid(format!("camera {camera} does not exist ({cameras} present)")));
        }
        let nodes = self.gltf.nodes.as_ref().map_or(0, Vec::len);
        let n = self
            .gltf
            .nodes
            .as_mut()
            .and_then(|v| v.get_mut(node))
            .ok_or_else(|| invalid(format!("node {node} does not exist ({nodes} present)")))?;
        if let Some(existing) = n.camera {
            return Err(invalid(format!(
                "node {node} already has camera {existing}; call clear_node_camera first"
            )));
        }
        n.camera = Some(camera);
        Ok(())
    }

    /// Detach the camera from `node`. A node without a camera is left as is.
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the node does not exist.
    pub fn clear_node_camera(&mut self, node: usize) -> Result<()> {
        let nodes = self.gltf.nodes.as_ref().map_or(0, Vec::len);
        let n = self
            .gltf
            .nodes
            .as_mut()
            .and_then(|v| v.get_mut(node))
            .ok_or_else(|| invalid(format!("node {node} does not exist ({nodes} present)")))?;
        n.camera = None;
        Ok(())
    }
}
