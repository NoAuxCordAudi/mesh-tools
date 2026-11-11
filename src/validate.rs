//! # Document Validation
//!
//! [`GltfBuilder::validate`] checks the document for structural glTF errors that the
//! individual builder methods cannot see (because the document can also be edited through
//! the public `gltf` field, and because some rules span several objects): dangling indices,
//! hierarchy problems, skin consistency, morph targets with their default and node weights and
//! target names, and animation targets, including the output type and element count of every
//! channel (`weights` channels are checked against the morph target count of the node's mesh).
//!
//! Export ([`GltfBuilder::to_glb_bytes`], [`GltfBuilder::export_glb`]) fails with
//! `GltfError::InvalidData` listing *all* errors. Use
//! [`to_glb_bytes_unchecked`](GltfBuilder::to_glb_bytes_unchecked) /
//! [`export_glb_unchecked`](GltfBuilder::export_glb_unchecked) to skip the check deliberately.
//!
//! Every message starts with a stable upper-case code (for example `SKIN_JOINT_OUT_OF_RANGE:`)
//! so callers and tests can match on it.
//!
//! Warnings never block export.

use crate::builder::GltfBuilder;
use crate::builder_animation::{expected_output_count, interpolation_from_str, InterpolationType};
use crate::models::{Accessor, Mesh, Primitive};

/// Result of [`GltfBuilder::validate`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    /// Problems that make the file invalid or wrong. These block export.
    pub errors: Vec<String>,
    /// Suspicious but legal constructs. These never block export.
    pub warnings: Vec<String>,
}

impl ValidationReport {
    /// True if there are no errors (warnings are allowed).
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// True if any error message starts with `code`.
    pub fn has_error(&self, code: &str) -> bool {
        self.errors.iter().any(|e| e.starts_with(code))
    }

    /// True if any warning message starts with `code`.
    pub fn has_warning(&self, code: &str) -> bool {
        self.warnings.iter().any(|w| w.starts_with(code))
    }
}

const ANIMATION_PATHS: [&str; 4] = ["translation", "rotation", "scale", "weights"];

impl GltfBuilder {
    /// Validate the whole document. See the [module docs](crate::validate).
    pub fn validate(&self) -> ValidationReport {
        let mut r = ValidationReport::default();
        let doc = Doc::new(self);
        doc.check_buffers(&mut r);
        doc.check_accessors(&mut r);
        doc.check_meshes(&mut r);
        doc.check_morphs(&mut r);
        let parents = doc.check_hierarchy(&mut r);
        doc.check_scenes(&parents, &mut r);
        doc.check_skins(&parents, &mut r);
        doc.check_skinned_nodes(&mut r);
        doc.check_animations(&mut r);
        r
    }
}

fn identity_ish(node: &crate::models::Node) -> bool {
    const I16: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    node.translation.map_or(true, |t| t == [0.0; 3])
        && node.rotation.map_or(true, |r| r == [0.0, 0.0, 0.0, 1.0])
        && node.scale.map_or(true, |s| s == [1.0; 3])
        && node.matrix.map_or(true, |m| m == I16)
}

/// Borrowed view of the document with count helpers.
struct Doc<'a> {
    b: &'a GltfBuilder,
    nodes: &'a [crate::models::Node],
    meshes: &'a [Mesh],
    accessors: &'a [Accessor],
}

impl<'a> Doc<'a> {
    fn new(b: &'a GltfBuilder) -> Self {
        Doc {
            b,
            nodes: b.gltf.nodes.as_deref().unwrap_or(&[]),
            meshes: b.gltf.meshes.as_deref().unwrap_or(&[]),
            accessors: b.gltf.accessors.as_deref().unwrap_or(&[]),
        }
    }

    fn check_buffers(&self, r: &mut ValidationReport) {
        let data_len = self.b.buffer_data.len();
        match self.b.gltf.buffers.as_deref().and_then(|b| b.first()) {
            Some(buf) if buf.byte_length != data_len => r.errors.push(format!(
                "BUFFER_LENGTH_MISMATCH: buffer 0 declares {} bytes but {} are stored", buf.byte_length, data_len
            )),
            None if data_len > 0 => r.errors.push("BUFFER_MISSING: binary data exists but there is no buffer".to_string()),
            _ => {}
        }
        for (i, v) in self.b.gltf.buffer_views.iter().flatten().enumerate() {
            if v.buffer != 0 {
                r.errors.push(format!("BUFFER_VIEW_BUFFER_INVALID: buffer view {i} references buffer {}", v.buffer));
            }
            if v.byte_offset + v.byte_length > data_len {
                r.errors.push(format!(
                    "BUFFER_VIEW_RANGE: buffer view {i} covers bytes {}..{} but the buffer has {data_len}",
                    v.byte_offset, v.byte_offset + v.byte_length
                ));
            }
        }
    }

    fn check_accessors(&self, r: &mut ValidationReport) {
        let views = self.b.gltf.buffer_views.as_deref().unwrap_or(&[]);
        for (i, a) in self.accessors.iter().enumerate() {
            let Some(view) = views.get(a.buffer_view) else {
                r.errors.push(format!("ACCESSOR_BUFFER_VIEW_INVALID: accessor {i} references buffer view {}", a.buffer_view));
                continue;
            };
            let Some(elem) = element_size(a.component_type, &a.type_) else {
                r.errors.push(format!(
                    "ACCESSOR_TYPE_INVALID: accessor {i} has component type {} and type {:?}", a.component_type, a.type_
                ));
                continue;
            };
            if a.count == 0 {
                r.errors.push(format!("ACCESSOR_COUNT_ZERO: accessor {i} has no elements"));
                continue;
            }
            let stride = view.byte_stride.unwrap_or(elem);
            let end = a.byte_offset.unwrap_or(0) + stride * (a.count - 1) + elem;
            if end > view.byte_length {
                r.errors.push(format!(
                    "ACCESSOR_RANGE: accessor {i} needs {end} bytes but buffer view {} has {}", a.buffer_view, view.byte_length
                ));
            }
        }
    }

    fn check_meshes(&self, r: &mut ValidationReport) {
        let material_count = self.b.gltf.materials.as_ref().map_or(0, |m| m.len());
        for (mi, mesh) in self.meshes.iter().enumerate() {
            if mesh.primitives.is_empty() {
                r.errors.push(format!("MESH_NO_PRIMITIVES: mesh {mi} has no primitives"));
            }
            for (pi, p) in mesh.primitives.iter().enumerate() {
                for (name, &acc) in &p.attributes {
                    if acc >= self.accessors.len() {
                        r.errors.push(format!("PRIMITIVE_ACCESSOR_INVALID: mesh {mi} primitive {pi} {name} references accessor {acc}"));
                    }
                }
                if let Some(idx) = p.indices {
                    match self.accessors.get(idx) {
                        None => r.errors.push(format!("PRIMITIVE_INDICES_INVALID: mesh {mi} primitive {pi} references accessor {idx}")),
                        Some(a) if a.type_ != "SCALAR" || !matches!(a.component_type, 5121 | 5123 | 5125) => r.errors.push(format!(
                            "PRIMITIVE_INDICES_TYPE: mesh {mi} primitive {pi} indices accessor {idx} must be unsigned SCALAR"
                        )),
                        _ => {}
                    }
                }
                if let Some(m) = p.material {
                    if m >= material_count {
                        r.errors.push(format!("PRIMITIVE_MATERIAL_INVALID: mesh {mi} primitive {pi} references material {m} ({material_count} materials)"));
                    }
                }
                for n in joint_sets(p) {
                    if !p.attributes.contains_key(&format!("WEIGHTS_{n}")) {
                        r.errors.push(format!("PRIMITIVE_JOINTS_WITHOUT_WEIGHTS: mesh {mi} primitive {pi} has JOINTS_{n} but no WEIGHTS_{n}"));
                    }
                }
                for (name, _) in &p.attributes {
                    if let Some(n) = name.strip_prefix("WEIGHTS_") {
                        if !p.attributes.contains_key(&format!("JOINTS_{n}")) {
                            r.errors.push(format!("PRIMITIVE_WEIGHTS_WITHOUT_JOINTS: mesh {mi} primitive {pi} has {name} but no JOINTS_{n}"));
                        }
                    }
                }
            }
        }
    }

    /// Returns each node's parent as derived from the document's `children` lists.
    fn check_hierarchy(&self, r: &mut ValidationReport) -> Vec<Option<usize>> {
        let n = self.nodes.len();
        let mut parents: Vec<Option<usize>> = vec![None; n];
        for (i, node) in self.nodes.iter().enumerate() {
            if let Some(m) = node.mesh {
                if m >= self.meshes.len() {
                    r.errors.push(format!("NODE_MESH_INVALID: node {i} references mesh {m} ({} meshes)", self.meshes.len()));
                }
            }
            if node.matrix.is_some() && (node.translation.is_some() || node.rotation.is_some() || node.scale.is_some()) {
                r.errors.push(format!("NODE_MATRIX_AND_TRS: node {i} has both a matrix and translation/rotation/scale"));
            }
            for &c in node.children.iter().flatten() {
                if c >= n {
                    r.errors.push(format!("NODE_CHILD_INVALID: node {i} lists child {c} ({n} nodes)"));
                } else if let Some(prev) = parents[c] {
                    r.errors.push(format!("NODE_PARENT_OVERRIDE: node {c} is a child of both node {prev} and node {i}"));
                } else {
                    parents[c] = Some(i);
                }
            }
        }
        for i in 0..n {
            if root_of(&parents, i).is_none() {
                r.errors.push(format!("NODE_CYCLE: node {i} is part of a cycle in the node hierarchy"));
            }
        }
        parents
    }

    fn check_scenes(&self, parents: &[Option<usize>], r: &mut ValidationReport) {
        let scenes = self.b.gltf.scenes.as_deref().unwrap_or(&[]);
        if let Some(s) = self.b.gltf.scene {
            if s >= scenes.len() {
                r.errors.push(format!("SCENE_DEFAULT_INVALID: default scene {s} does not exist ({} scenes)", scenes.len()));
            }
        }
        for (si, scene) in scenes.iter().enumerate() {
            for &n in scene.nodes.iter().flatten() {
                if n >= self.nodes.len() {
                    r.errors.push(format!("SCENE_NODE_INVALID: scene {si} lists node {n} ({} nodes)", self.nodes.len()));
                } else if let Some(p) = parents[n] {
                    r.errors.push(format!("SCENE_ROOT_HAS_PARENT: scene {si} lists node {n}, which is a child of node {p}"));
                }
            }
        }
    }

    fn check_skins(&self, parents: &[Option<usize>], r: &mut ValidationReport) {
        for (si, skin) in self.b.gltf.skins.iter().flatten().enumerate() {
            if skin.joints.is_empty() {
                r.errors.push(format!("SKIN_NO_JOINTS: skin {si} has no joints"));
                continue;
            }
            let mut seen = std::collections::HashSet::new();
            let mut roots = std::collections::HashSet::new();
            for &j in &skin.joints {
                if j >= self.nodes.len() {
                    r.errors.push(format!("SKIN_JOINT_INVALID: skin {si} joint node {j} does not exist ({} nodes)", self.nodes.len()));
                    continue;
                }
                if !seen.insert(j) {
                    r.errors.push(format!("SKIN_JOINT_DUPLICATE: skin {si} lists joint node {j} more than once"));
                }
                if let Some(root) = root_of(parents, j) {
                    roots.insert(root);
                }
                if let Some(s) = skin.skeleton {
                    if s < self.nodes.len() && !is_ancestor_or_self(parents, s, j) {
                        r.errors.push(format!("SKIN_SKELETON_NOT_ANCESTOR: skin {si} skeleton node {s} is not an ancestor of joint node {j}"));
                    }
                }
            }
            if roots.len() > 1 {
                r.errors.push(format!("SKIN_NO_COMMON_ROOT: skin {si} joints are spread over {} separate hierarchies", roots.len()));
            }
            if let Some(s) = skin.skeleton {
                if s >= self.nodes.len() {
                    r.errors.push(format!("SKIN_SKELETON_INVALID: skin {si} skeleton node {s} does not exist"));
                }
            }
            if let Some(acc) = skin.inverse_bind_matrices {
                match self.accessors.get(acc) {
                    None => r.errors.push(format!("SKIN_IBM_INVALID: skin {si} references accessor {acc}")),
                    Some(a) if a.type_ != "MAT4" || a.component_type != 5126 => r.errors.push(format!(
                        "SKIN_IBM_TYPE: skin {si} inverse bind matrix accessor {acc} must be FLOAT MAT4"
                    )),
                    Some(a) if a.count < skin.joints.len() => r.errors.push(format!(
                        "SKIN_IBM_COUNT: skin {si} has {} joints but accessor {acc} holds {} matrices", skin.joints.len(), a.count
                    )),
                    _ => {}
                }
            }
        }
    }

    /// Output accessor type and element count of one channel, judged by that channel's own
    /// path, target node and sampler interpolation (a sampler shared by several channels is
    /// checked once per channel).
    fn check_channel_output(
        &self,
        ai: usize,
        ci: usize,
        c: &crate::models::AnimationChannel,
        samplers: &[crate::models::AnimationSampler],
        r: &mut ValidationReport,
    ) {
        let path = c.target.path.as_str();
        let Some(expected_type) = (match path {
            "translation" | "scale" => Some("VEC3"),
            "rotation" => Some("VEC4"),
            "weights" => Some("SCALAR"),
            _ => None, // reported as ANIMATION_CHANNEL_PATH_INVALID
        }) else { return };
        let at = format!("animation {ai} channel {ci} ({path} of node {})", c.target.node);
        let Some(sampler) = samplers.get(c.sampler) else { return };
        let (Some(input), Some(output)) = (self.accessors.get(sampler.input), self.accessors.get(sampler.output)) else { return };

        // Rotation and weights may legally use normalized integer outputs; translation and scale may not.
        let float = output.component_type == 5126;
        let normalized_int = matches!(output.component_type, 5120..=5123) && output.normalized == Some(true);
        let type_ok = output.type_ == expected_type && (float || (normalized_int && matches!(path, "rotation" | "weights")));
        if !type_ok {
            r.errors.push(format!(
                "ANIMATION_OUTPUT_TYPE: {at}: sampler {} output accessor {} is {} with component type {}{}, expected {expected_type} {}",
                c.sampler,
                sampler.output,
                output.type_,
                output.component_type,
                if output.normalized == Some(true) { " (normalized)" } else { "" },
                if matches!(path, "rotation" | "weights") { "FLOAT or normalized integer" } else { "FLOAT" },
            ));
            return;
        }

        let interpolation = sampler.interpolation.as_deref().and_then(interpolation_from_str).unwrap_or(InterpolationType::Linear);
        if interpolation == InterpolationType::CubicSpline && input.count < 2 {
            r.errors.push(format!("ANIMATION_OUTPUT_COUNT: {at}: CUBICSPLINE needs at least two keyframes, found {}", input.count));
        }

        let width = if path == "weights" {
            let m = self.nodes.get(c.target.node).and_then(|n| n.mesh).and_then(|mi| self.meshes.get(mi)).map_or(0, |mesh| {
                mesh.primitives.iter().map(|p| p.targets.as_ref().map_or(0, |t| t.len())).find(|&n| n > 0).unwrap_or(0)
            });
            // An invalid node is reported as ANIMATION_CHANNEL_NODE_INVALID elsewhere.
            if m == 0 && self.nodes.get(c.target.node).is_some() {
                r.errors.push(format!("ANIMATION_CHANNEL_WEIGHTS_NO_MORPHS: {at}: the node has no mesh with morph targets"));
                return;
            }
            m
        } else {
            1
        };
        if width == 0 {
            return;
        }
        let expected = expected_output_count(input.count, interpolation, width);
        if output.count != expected {
            r.errors.push(format!(
                "ANIMATION_OUTPUT_COUNT: {at}: {} {interpolation} keyframes{} need {expected} output elements but output accessor {} has {}",
                input.count,
                if path == "weights" { format!(" and {width} morph targets") } else { String::new() },
                sampler.output,
                output.count
            ));
        }
    }

    fn check_skinned_nodes(&self, r: &mut ValidationReport) {
        let skins = self.b.gltf.skins.as_deref().unwrap_or(&[]);
        for (ni, node) in self.nodes.iter().enumerate() {
            let Some(si) = node.skin else { continue };
            let Some(skin) = skins.get(si) else {
                r.errors.push(format!("NODE_SKIN_INVALID: node {ni} references skin {si} ({} skins)", skins.len()));
                continue;
            };
            let Some(mesh) = node.mesh.and_then(|m| self.meshes.get(m)) else {
                if node.mesh.is_none() {
                    r.errors.push(format!("NODE_SKIN_WITHOUT_MESH: node {ni} has skin {si} but no mesh"));
                }
                continue;
            };
            if !identity_ish(node) {
                r.warnings.push(format!(
                    "SKINNED_NODE_LOCAL_TRANSFORM: node {ni} is skinned and has a non-identity transform, which glTF viewers ignore"
                ));
            }
            for (pi, p) in mesh.primitives.iter().enumerate() {
                if !p.attributes.contains_key("JOINTS_0") || !p.attributes.contains_key("WEIGHTS_0") {
                    r.errors.push(format!(
                        "SKIN_MESH_MISSING_ATTRIBUTES: node {ni} is skinned but mesh {} primitive {pi} lacks JOINTS_0/WEIGHTS_0",
                        node.mesh.unwrap()
                    ));
                    continue;
                }
                for n in joint_sets(p) {
                    self.check_joint_values(ni, si, skin.joints.len(), node.mesh.unwrap(), pi, p, n, r);
                }
            }
        }
    }

    /// Read `JOINTS_n` back from the buffer and check each (non-zero-weight) slot is in range.
    #[allow(clippy::too_many_arguments)]
    fn check_joint_values(&self, ni: usize, si: usize, joint_count: usize, mi: usize, pi: usize, p: &Primitive, n: usize, r: &mut ValidationReport) {
        let (Some(&ja), Some(&wa)) = (p.attributes.get(&format!("JOINTS_{n}")), p.attributes.get(&format!("WEIGHTS_{n}"))) else { return };
        let (Some(joints), weights) = (self.read_joints(ja), self.read_f32(wa)) else { return };
        for (v, slots) in joints.iter().enumerate() {
            for (k, &slot) in slots.iter().enumerate() {
                let weighted = weights.as_ref().map_or(true, |w| w.get(v * 4 + k).map_or(true, |&x| x > 0.0));
                if weighted && slot >= joint_count {
                    r.errors.push(format!(
                        "SKIN_JOINT_OUT_OF_RANGE: node {ni} (skin {si}, {joint_count} joints): mesh {mi} primitive {pi} JOINTS_{n} vertex {v} slot {k} is {slot}; \
                         JOINTS_n holds skin slots, not node indices"
                    ));
                    return;
                }
            }
        }
    }

    fn accessor_bytes(&self, index: usize) -> Option<(&Accessor, &[u8])> {
        let a = self.accessors.get(index)?;
        let view = self.b.gltf.buffer_views.as_ref()?.get(a.buffer_view)?;
        let elem = element_size(a.component_type, &a.type_)?;
        let start = view.byte_offset + a.byte_offset.unwrap_or(0);
        let stride = view.byte_stride.unwrap_or(elem);
        let end = start + stride * a.count.checked_sub(1)? + elem;
        let data = self.b.buffer_data.get(start..end)?;
        Some((a, data)).filter(|_| stride == elem) // only tightly packed data is decoded
    }

    fn read_joints(&self, index: usize) -> Option<Vec<[usize; 4]>> {
        let (a, data) = self.accessor_bytes(index)?;
        if a.type_ != "VEC4" {
            return None;
        }
        let vals: Vec<usize> = match a.component_type {
            5121 => data.iter().map(|&v| v as usize).collect(),
            5123 => data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as usize).collect(),
            _ => return None,
        };
        Some(vals.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect())
    }

    fn read_f32(&self, index: usize) -> Option<Vec<f32>> {
        let (a, data) = self.accessor_bytes(index)?;
        (a.component_type == 5126).then(|| data.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
    }

    fn check_animations(&self, r: &mut ValidationReport) {
        for (ai, anim) in self.b.gltf.animations.iter().flatten().enumerate() {
            let samplers = anim.samplers.as_deref().unwrap_or(&[]);
            let channels = anim.channels.as_deref().unwrap_or(&[]);
            if channels.is_empty() || samplers.is_empty() {
                r.errors.push(format!("ANIMATION_EMPTY: animation {ai} needs at least one channel and one sampler"));
            }
            for (si, s) in samplers.iter().enumerate() {
                for (what, acc) in [("input", s.input), ("output", s.output)] {
                    if acc >= self.accessors.len() {
                        r.errors.push(format!("ANIMATION_SAMPLER_ACCESSOR_INVALID: animation {ai} sampler {si} {what} references accessor {acc}"));
                    }
                }
                if let Some(i) = s.interpolation.as_deref() {
                    if interpolation_from_str(i).is_none() {
                        r.errors.push(format!("ANIMATION_INTERPOLATION_INVALID: animation {ai} sampler {si} interpolation {i:?} is not LINEAR, STEP or CUBICSPLINE"));
                    }
                }
                if let Some(input) = self.accessors.get(s.input) {
                    if input.type_ != "SCALAR" || input.component_type != 5126 {
                        r.errors.push(format!(
                            "ANIMATION_INPUT_TYPE: animation {ai} sampler {si} input accessor {} must be FLOAT SCALAR keyframe times", s.input
                        ));
                    }
                    if input.min.is_none() || input.max.is_none() {
                        r.errors.push(format!("ANIMATION_INPUT_NO_BOUNDS: animation {ai} sampler {si} input accessor {} lacks min/max", s.input));
                    }
                }
            }
            for (ci, c) in channels.iter().enumerate() {
                if c.sampler >= samplers.len() {
                    r.errors.push(format!("ANIMATION_CHANNEL_SAMPLER_INVALID: animation {ai} channel {ci} references sampler {}", c.sampler));
                }
                if !ANIMATION_PATHS.contains(&c.target.path.as_str()) {
                    r.errors.push(format!("ANIMATION_CHANNEL_PATH_INVALID: animation {ai} channel {ci} path {:?}", c.target.path));
                }
                match self.nodes.get(c.target.node) {
                    None => r.errors.push(format!("ANIMATION_CHANNEL_NODE_INVALID: animation {ai} channel {ci} targets node {}", c.target.node)),
                    Some(n) if n.matrix.is_some() && c.target.path != "weights" => r.errors.push(format!(
                        "ANIMATION_CHANNEL_NODE_MATRIX: animation {ai} channel {ci} targets node {}, which has a matrix", c.target.node
                    )),
                    _ => {}
                }
                self.check_channel_output(ai, ci, c, samplers, r);
            }
        }
    }
}

/// Rough upper bound on vertex attributes a renderer can bind (the WebGL minimum is 16).
/// Used only for the `MORPH_ATTRIBUTE_BUDGET` warning.
const ATTRIBUTE_BUDGET: usize = 16;

impl<'a> Doc<'a> {
    /// Morph targets, default weights, target names and node weights.
    fn check_morphs(&self, r: &mut ValidationReport) {
        for (mi, mesh) in self.meshes.iter().enumerate() {
            let counts: Vec<usize> = mesh.primitives.iter().map(|p| p.targets.as_ref().map_or(0, |t| t.len())).collect();
            // Number of targets the mesh is expected to have: that of its first primitive with targets.
            let m = counts.iter().copied().find(|&c| c > 0).unwrap_or(0);

            for (pi, p) in mesh.primitives.iter().enumerate() {
                let Some(targets) = &p.targets else { continue };
                if targets.is_empty() {
                    r.errors.push(format!("MORPH_TARGETS_EMPTY: mesh {mi} primitive {pi} has an empty targets array"));
                    continue;
                }
                if counts[pi] != m && m > 0 {
                    r.errors.push(format!(
                        "MORPH_TARGETS_INCONSISTENT: mesh {mi} primitive {pi} has {} morph targets but another primitive has {m}", counts[pi]
                    ));
                }
                let mut budget = p.attributes.len();
                for (ti, target) in targets.iter().enumerate() {
                    budget += target.len();
                    if target.is_empty() {
                        r.errors.push(format!("MORPH_TARGET_EMPTY: mesh {mi} primitive {pi} target {ti} has no attributes"));
                    }
                    for (name, &acc) in target {
                        self.check_target_attribute(mi, pi, ti, name, acc, p, r);
                    }
                }
                if budget > ATTRIBUTE_BUDGET {
                    r.warnings.push(format!(
                        "MORPH_ATTRIBUTE_BUDGET: mesh {mi} primitive {pi} uses {budget} vertex attributes including morph targets; \
                         many renderers bind at most {ATTRIBUTE_BUDGET}"
                    ));
                }
            }

            if let Some(w) = &mesh.weights {
                if w.is_empty() {
                    r.errors.push(format!("MESH_WEIGHTS_EMPTY: mesh {mi} has an empty weights array"));
                } else if w.len() != m {
                    r.errors.push(format!("MESH_WEIGHTS_COUNT: mesh {mi} has {} weights but {m} morph targets", w.len()));
                }
                if let Some(i) = w.iter().position(|v| !v.is_finite()) {
                    r.errors.push(format!("MESH_WEIGHTS_NOT_FINITE: mesh {mi} weight {i} is not finite"));
                }
            }
            if let Some(names) = mesh.extras.as_ref().and_then(|e| e.get("targetNames")) {
                match names.as_array().filter(|a| a.iter().all(|n| n.is_string())) {
                    None => r.errors.push(format!("MESH_TARGET_NAMES_INVALID: mesh {mi} extras.targetNames must be an array of strings")),
                    Some(a) => {
                        if a.len() != m {
                            r.errors.push(format!("MESH_TARGET_NAMES_COUNT: mesh {mi} has {} target names but {m} morph targets", a.len()));
                        }
                        let mut seen = std::collections::HashSet::new();
                        if let Some(dup) = a.iter().filter_map(|n| n.as_str()).find(|n| !seen.insert(*n)) {
                            r.warnings.push(format!("MESH_TARGET_NAMES_DUPLICATE: mesh {mi} uses the target name {dup:?} more than once"));
                        }
                    }
                }
            }
        }

        for (ni, node) in self.nodes.iter().enumerate() {
            let Some(w) = &node.weights else { continue };
            let Some(mesh_index) = node.mesh else {
                r.errors.push(format!("NODE_WEIGHTS_WITHOUT_MESH: node {ni} has weights but no mesh"));
                continue;
            };
            // A dangling mesh index is reported as NODE_MESH_INVALID elsewhere.
            let Some(mesh) = self.meshes.get(mesh_index) else { continue };
            let m = mesh.primitives.iter().map(|p| p.targets.as_ref().map_or(0, |t| t.len())).find(|&c| c > 0).unwrap_or(0);
            if w.len() != m {
                r.errors.push(format!("NODE_WEIGHTS_COUNT: node {ni} has {} weights but its mesh {mesh_index} has {m} morph targets", w.len()));
            }
            if let Some(i) = w.iter().position(|v| !v.is_finite()) {
                r.errors.push(format!("NODE_WEIGHTS_NOT_FINITE: node {ni} weight {i} is not finite"));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_target_attribute(&self, mi: usize, pi: usize, ti: usize, name: &str, acc: usize, base: &Primitive, r: &mut ValidationReport) {
        let at = format!("mesh {mi} primitive {pi} target {ti} {name}");
        if !matches!(name, "POSITION" | "NORMAL" | "TANGENT") && !name.starts_with('_') {
            r.errors.push(format!("MORPH_TARGET_ATTRIBUTE_INVALID: {at} is not POSITION, NORMAL, TANGENT or an application-specific `_` attribute"));
        }
        let Some(&base_acc) = base.attributes.get(name) else {
            r.errors.push(format!("MORPH_TARGET_ATTRIBUTE_NOT_IN_BASE: {at}: the primitive has no base {name} attribute"));
            return;
        };
        let Some(a) = self.accessors.get(acc) else {
            r.errors.push(format!("MORPH_TARGET_ACCESSOR_INVALID: {at} references accessor {acc}"));
            return;
        };
        if matches!(name, "POSITION" | "NORMAL" | "TANGENT") && (a.type_ != "VEC3" || a.component_type != 5126) {
            r.errors.push(format!(
                "MORPH_TARGET_TYPE: {at} accessor {acc} must be FLOAT VEC3, found {} with component type {}", a.type_, a.component_type
            ));
        }
        if let Some(b) = self.accessors.get(base_acc) {
            if a.count != b.count {
                r.errors.push(format!(
                    "MORPH_TARGET_COUNT_MISMATCH: {at} accessor {acc} has {} elements but the base {name} accessor {base_acc} has {}", a.count, b.count
                ));
            }
        }
        if name == "POSITION" && (a.min.is_none() || a.max.is_none()) {
            r.warnings.push(format!("MORPH_TARGET_POSITION_NO_BOUNDS: {at} accessor {acc} lacks min/max"));
        }
    }
}

/// Attribute set numbers `n` for which the primitive has `JOINTS_n`.
fn joint_sets(p: &Primitive) -> Vec<usize> {
    let mut v: Vec<usize> = p.attributes.keys().filter_map(|k| k.strip_prefix("JOINTS_")?.parse().ok()).collect();
    v.sort_unstable();
    v
}

fn element_size(component_type: usize, type_: &str) -> Option<usize> {
    let comp = match component_type {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => return None,
    };
    let n = match type_ {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" | "MAT2" => 4,
        "MAT3" => 9,
        "MAT4" => 16,
        _ => return None,
    };
    Some(comp * n)
}

/// Topmost ancestor of `node`, or `None` if the parent chain loops.
fn root_of(parents: &[Option<usize>], node: usize) -> Option<usize> {
    let mut cur = node;
    for _ in 0..=parents.len() {
        match parents[cur] {
            None => return Some(cur),
            Some(p) => cur = p,
        }
    }
    None
}

fn is_ancestor_or_self(parents: &[Option<usize>], ancestor: usize, node: usize) -> bool {
    let mut cur = node;
    for _ in 0..=parents.len() {
        if cur == ancestor {
            return true;
        }
        match parents[cur] {
            None => return false,
            Some(p) => cur = p,
        }
    }
    false
}
