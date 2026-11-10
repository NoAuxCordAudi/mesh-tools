//! # Animation Builder Support
//!
//! Keyframe animation for glTF nodes: translation, rotation and scale tracks with
//! `LINEAR`, `STEP` or `CUBICSPLINE` interpolation.
//!
//! Every function that can be given bad input returns a [`Result`] with a message naming
//! the offending animation, node, keyframe or index; nothing panics. Each call validates
//! everything *before* it writes anything, so a failed call leaves the document unchanged.
//!
//! ## Rules enforced
//!
//! - Keyframe times are non-empty, finite, `>= 0` and strictly increasing.
//! - The time accessor carries `min`/`max`, as the glTF spec requires for animation input.
//! - Rotations are normalized (for cubic splines, only the value entries, not tangents).
//! - Within one animation, each `(node, path)` pair is animated by at most one channel.
//! - A node with a `matrix` cannot be animated (the spec forbids mixing it with TRS animation).
//! - `LINEAR`/`STEP` outputs have one element per keyframe; `CUBICSPLINE` outputs have three
//!   (in-tangent, value, out-tangent) per keyframe and at least two keyframes. Use the
//!   `add_cubic_*_track` functions, which take [`CubicKeyframe`]s, for the latter.
//! - Animation data buffer views have no `target`.

use crate::builder::GltfBuilder;
use crate::error::{GltfError, Result};
use crate::models::{Animation, AnimationChannel, AnimationChannelTarget, AnimationSampler};
use std::fmt;

/// Animation path type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationPath {
    Translation,
    Rotation,
    Scale,
    /// Morph target weights. Not supported yet: channels with this path are rejected.
    Weights,
}

impl fmt::Display for AnimationPath {
    /// The glTF spelling (`translation`, `rotation`, `scale`, `weights`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AnimationPath::Translation => "translation",
            AnimationPath::Rotation => "rotation",
            AnimationPath::Scale => "scale",
            AnimationPath::Weights => "weights",
        })
    }
}

impl AnimationPath {
    /// Accessor `type` the sampler output must have for this path.
    fn output_type(self) -> &'static str {
        match self {
            AnimationPath::Translation | AnimationPath::Scale => "VEC3",
            AnimationPath::Rotation => "VEC4",
            AnimationPath::Weights => "SCALAR",
        }
    }
}

/// Interpolation method for animation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterpolationType {
    Linear,
    Step,
    CubicSpline,
}

impl fmt::Display for InterpolationType {
    /// The glTF spelling (`LINEAR`, `STEP`, `CUBICSPLINE`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            InterpolationType::Linear => "LINEAR",
            InterpolationType::Step => "STEP",
            InterpolationType::CubicSpline => "CUBICSPLINE",
        })
    }
}

/// One cubic-spline keyframe: value plus its incoming and outgoing tangents.
///
/// For rotations, `value` is normalized on insertion; tangents are stored as given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicKeyframe<T> {
    pub in_tangent: T,
    pub value: T,
    pub out_tangent: T,
}

fn invalid(msg: String) -> GltfError {
    GltfError::InvalidInput(msg)
}

impl GltfBuilder {
    /// Add an animation to the glTF
    ///
    /// The animation starts with no channels or samplers; they are created by the track
    /// functions below.
    ///
    /// # Returns
    ///
    /// The index of the created animation
    pub fn add_animation(&mut self, name: Option<String>) -> usize {
        let animation = Animation {
            name,
            channels: None,
            samplers: None,
        };
        let animations = self.gltf.animations.get_or_insert_with(Vec::new);
        animations.push(animation);
        animations.len() - 1
    }

    /// Add a sampler to an animation (low level; prefer the track functions).
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the animation or either accessor does not exist, the
    /// input is not a FLOAT `SCALAR` accessor, or the output has the wrong element count
    /// for `interpolation` (`N` for linear/step, `3N` with `N >= 2` for cubic spline).
    ///
    /// # Returns
    ///
    /// The index of the created sampler within the animation
    pub fn add_animation_sampler(
        &mut self,
        animation_index: usize,
        input_accessor: usize,
        output_accessor: usize,
        interpolation: InterpolationType,
    ) -> Result<usize> {
        self.check_animation(animation_index)?;
        let input = self.accessor_info(input_accessor)?;
        let output = self.accessor_info(output_accessor)?;
        if input.type_ != "SCALAR" || input.component_type != 5126 {
            return Err(invalid(format!(
                "sampler input accessor {input_accessor} must be FLOAT SCALAR keyframe times"
            )));
        }
        check_output_count(input.count, output.count, interpolation)
            .map_err(|m| invalid(format!("sampler output accessor {output_accessor}: {m}")))?;
        Ok(self.push_sampler(animation_index, input_accessor, output_accessor, interpolation))
    }

    /// Add a channel to an animation (low level; prefer the track functions).
    ///
    /// # Errors
    ///
    /// `GltfError::InvalidInput` if the animation, sampler or node does not exist, the node
    /// has a `matrix`, `(node, path)` already has a channel in this animation, the sampler's
    /// output accessor type does not match `target_path`, or `target_path` is
    /// [`AnimationPath::Weights`] (not supported yet).
    ///
    /// # Returns
    ///
    /// The index of the created channel within the animation
    pub fn add_animation_channel(
        &mut self,
        animation_index: usize,
        sampler_index: usize,
        target_node: usize,
        target_path: AnimationPath,
    ) -> Result<usize> {
        self.check_animation(animation_index)?;
        // Report the unsupported path before any accessor-type mismatch it would cause.
        self.check_channel_target(animation_index, target_node, target_path)?;
        let sampler = self.gltf.animations.as_ref().unwrap()[animation_index]
            .samplers
            .as_ref()
            .and_then(|s| s.get(sampler_index))
            .ok_or_else(|| invalid(format!(
                "animation {animation_index} has no sampler {sampler_index}"
            )))?;
        let output = self.accessor_info(sampler.output)?;
        if output.type_ != target_path.output_type() {
            return Err(invalid(format!(
                "sampler {sampler_index} outputs {} but the {target_path} path needs {}",
                output.type_,
                target_path.output_type()
            )));
        }
        Ok(self.push_channel(animation_index, sampler_index, target_node, target_path))
    }

    /// Animate a node's translation.
    ///
    /// `values` holds one `[x, y, z]` per keyframe (`LINEAR` or `STEP`). For `CUBICSPLINE`
    /// use [`add_cubic_translation_track`](Self::add_cubic_translation_track).
    ///
    /// # Errors
    ///
    /// See the module docs for the rules enforced. Nothing is added on error.
    ///
    /// # Returns
    ///
    /// The indices of the created channel and sampler
    pub fn add_translation_track(
        &mut self,
        animation_index: usize,
        node_index: usize,
        times: &[f32],
        values: &[[f32; 3]],
        interpolation: InterpolationType,
    ) -> Result<(usize, usize)> {
        reject_cubic(interpolation, "add_cubic_translation_track")?;
        let flat: Vec<f32> = values.iter().flatten().copied().collect();
        self.add_track(animation_index, node_index, AnimationPath::Translation, times, &flat, values.len(), interpolation)
    }

    /// Animate a node's rotation with unit quaternions `[x, y, z, w]` (normalized on insertion).
    ///
    /// For `CUBICSPLINE` use [`add_cubic_rotation_track`](Self::add_cubic_rotation_track).
    ///
    /// # Errors
    ///
    /// See the module docs for the rules enforced. A zero quaternion is rejected.
    ///
    /// # Returns
    ///
    /// The indices of the created channel and sampler
    pub fn add_rotation_track(
        &mut self,
        animation_index: usize,
        node_index: usize,
        times: &[f32],
        values: &[[f32; 4]],
        interpolation: InterpolationType,
    ) -> Result<(usize, usize)> {
        reject_cubic(interpolation, "add_cubic_rotation_track")?;
        let mut flat = Vec::with_capacity(values.len() * 4);
        for (k, q) in values.iter().enumerate() {
            flat.extend_from_slice(&normalize_rotation(*q, k)?);
        }
        self.add_track(animation_index, node_index, AnimationPath::Rotation, times, &flat, values.len(), interpolation)
    }

    /// Animate a node's scale.
    ///
    /// For `CUBICSPLINE` use [`add_cubic_scale_track`](Self::add_cubic_scale_track).
    ///
    /// # Errors
    ///
    /// See the module docs for the rules enforced. Nothing is added on error.
    ///
    /// # Returns
    ///
    /// The indices of the created channel and sampler
    pub fn add_scale_track(
        &mut self,
        animation_index: usize,
        node_index: usize,
        times: &[f32],
        values: &[[f32; 3]],
        interpolation: InterpolationType,
    ) -> Result<(usize, usize)> {
        reject_cubic(interpolation, "add_cubic_scale_track")?;
        let flat: Vec<f32> = values.iter().flatten().copied().collect();
        self.add_track(animation_index, node_index, AnimationPath::Scale, times, &flat, values.len(), interpolation)
    }

    /// Animate translation with cubic-spline keyframes (at least two).
    ///
    /// # Errors
    ///
    /// See the module docs. Returns the channel and sampler indices on success.
    pub fn add_cubic_translation_track(
        &mut self,
        animation_index: usize,
        node_index: usize,
        times: &[f32],
        keyframes: &[CubicKeyframe<[f32; 3]>],
    ) -> Result<(usize, usize)> {
        let flat = flatten_cubic(keyframes, |v| *v);
        self.add_track(animation_index, node_index, AnimationPath::Translation, times, &flat, keyframes.len(), InterpolationType::CubicSpline)
    }

    /// Animate rotation with cubic-spline keyframes (at least two). Only the `value` of each
    /// keyframe is normalized, not the tangents.
    ///
    /// # Errors
    ///
    /// See the module docs. Returns the channel and sampler indices on success.
    pub fn add_cubic_rotation_track(
        &mut self,
        animation_index: usize,
        node_index: usize,
        times: &[f32],
        keyframes: &[CubicKeyframe<[f32; 4]>],
    ) -> Result<(usize, usize)> {
        let mut normalized = Vec::with_capacity(keyframes.len());
        for (k, kf) in keyframes.iter().enumerate() {
            normalized.push(CubicKeyframe { value: normalize_rotation(kf.value, k)?, ..*kf });
        }
        let flat = flatten_cubic(&normalized, |v| *v);
        self.add_track(animation_index, node_index, AnimationPath::Rotation, times, &flat, keyframes.len(), InterpolationType::CubicSpline)
    }

    /// Animate scale with cubic-spline keyframes (at least two).
    ///
    /// # Errors
    ///
    /// See the module docs. Returns the channel and sampler indices on success.
    pub fn add_cubic_scale_track(
        &mut self,
        animation_index: usize,
        node_index: usize,
        times: &[f32],
        keyframes: &[CubicKeyframe<[f32; 3]>],
    ) -> Result<(usize, usize)> {
        let flat = flatten_cubic(keyframes, |v| *v);
        self.add_track(animation_index, node_index, AnimationPath::Scale, times, &flat, keyframes.len(), InterpolationType::CubicSpline)
    }

    /// Create translation keyframes for an animation.
    ///
    /// Thin wrapper over [`add_translation_track`](Self::add_translation_track) that takes
    /// owned vectors; `timestamps` and `translations` must have the same length.
    pub fn create_translation_animation(
        &mut self,
        animation_index: usize,
        node_index: usize,
        timestamps: Vec<f32>,
        translations: Vec<[f32; 3]>,
        interpolation: InterpolationType,
    ) -> Result<(usize, usize)> {
        self.add_translation_track(animation_index, node_index, &timestamps, &translations, interpolation)
    }

    /// Create rotation keyframes for an animation.
    ///
    /// Thin wrapper over [`add_rotation_track`](Self::add_rotation_track).
    pub fn create_rotation_animation(
        &mut self,
        animation_index: usize,
        node_index: usize,
        timestamps: Vec<f32>,
        rotations: Vec<[f32; 4]>,
        interpolation: InterpolationType,
    ) -> Result<(usize, usize)> {
        self.add_rotation_track(animation_index, node_index, &timestamps, &rotations, interpolation)
    }

    /// Create scale keyframes for an animation.
    ///
    /// Thin wrapper over [`add_scale_track`](Self::add_scale_track).
    pub fn create_scale_animation(
        &mut self,
        animation_index: usize,
        node_index: usize,
        timestamps: Vec<f32>,
        scales: Vec<[f32; 3]>,
        interpolation: InterpolationType,
    ) -> Result<(usize, usize)> {
        self.add_scale_track(animation_index, node_index, &timestamps, &scales, interpolation)
    }

    // ---------------------------------------------------------------- internals

    /// Validate fully, then write the time accessor, output accessor, sampler and channel.
    /// `data` is the flattened output; `keyframes` the number of keyframes it represents.
    fn add_track(
        &mut self,
        animation_index: usize,
        node_index: usize,
        path: AnimationPath,
        times: &[f32],
        data: &[f32],
        keyframes: usize,
        interpolation: InterpolationType,
    ) -> Result<(usize, usize)> {
        self.check_animation(animation_index)?;
        self.check_channel_target(animation_index, node_index, path)?;
        validate_times(times)?;

        let plain = interpolation != InterpolationType::CubicSpline;
        if keyframes != times.len() {
            return Err(invalid(format!(
                "{keyframes} keyframe values for {} times: they must have the same length", times.len()
            )));
        }
        if !plain && times.len() < 2 {
            return Err(invalid("CUBICSPLINE needs at least two keyframes".to_string()));
        }
        let components = match path.output_type() {
            "VEC4" => 4,
            _ => 3,
        };
        let expected = keyframes * components * if plain { 1 } else { 3 };
        if data.len() != expected {
            return Err(invalid(format!(
                "{path} output has {} floats, expected {expected}",
                data.len()
            )));
        }
        if let Some(i) = data.iter().position(|v| !v.is_finite()) {
            return Err(invalid(format!("{path} output value {i} is not finite")));
        }

        // ---- validated; write ----
        let input_accessor = self.write_f32_accessor(times, times.len(), "SCALAR", Some(vec![times[0]]), Some(vec![times[times.len() - 1]]));
        let elements = if plain { keyframes } else { keyframes * 3 };
        let output_accessor = self.write_f32_accessor(data, elements, path.output_type(), None, None);
        let sampler = self.push_sampler(animation_index, input_accessor, output_accessor, interpolation);
        let channel = self.push_channel(animation_index, sampler, node_index, path);
        Ok((channel, sampler))
    }

    fn write_f32_accessor(
        &mut self,
        data: &[f32],
        count: usize,
        type_: &str,
        min: Option<Vec<f32>>,
        max: Option<Vec<f32>>,
    ) -> usize {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let (offset, length) = self.add_buffer_data(&bytes);
        // Animation data is not vertex/index data, so the buffer view has no target.
        let view = self.add_buffer_view(offset, length, None);
        self.add_accessor(view, 5126, count, type_.to_string(), None, min, max)
    }

    fn push_sampler(&mut self, animation: usize, input: usize, output: usize, interpolation: InterpolationType) -> usize {
        let samplers = self.gltf.animations.as_mut().unwrap()[animation].samplers.get_or_insert_with(Vec::new);
        samplers.push(AnimationSampler {
            input,
            interpolation: Some(interpolation.to_string()),
            output,
        });
        samplers.len() - 1
    }

    fn push_channel(&mut self, animation: usize, sampler: usize, node: usize, path: AnimationPath) -> usize {
        let channels = self.gltf.animations.as_mut().unwrap()[animation].channels.get_or_insert_with(Vec::new);
        channels.push(AnimationChannel {
            sampler,
            target: AnimationChannelTarget { node, path: path.to_string() },
        });
        channels.len() - 1
    }

    fn check_animation(&self, animation_index: usize) -> Result<()> {
        let count = self.gltf.animations.as_ref().map_or(0, |a| a.len());
        if animation_index >= count {
            return Err(invalid(format!("animation {animation_index} does not exist ({count} animations)")));
        }
        Ok(())
    }

    /// Node exists, has no `matrix`, path is supported, and `(node, path)` is not already animated.
    fn check_channel_target(&self, animation_index: usize, node: usize, path: AnimationPath) -> Result<()> {
        if path == AnimationPath::Weights {
            return Err(invalid(
                "animating morph target weights is not supported yet".to_string(),
            ));
        }
        let nodes = self.gltf.nodes.as_deref().unwrap_or(&[]);
        let n = nodes.get(node).ok_or_else(|| invalid(format!(
            "animation target node {node} does not exist ({} nodes)", nodes.len()
        )))?;
        if n.matrix.is_some() {
            return Err(invalid(format!(
                "node {node} has a matrix and cannot be animated; use translation/rotation/scale"
            )));
        }
        let duplicate = self.gltf.animations.as_ref().unwrap()[animation_index]
            .channels
            .iter()
            .flatten()
            .any(|c| c.target.node == node && c.target.path == path.to_string());
        if duplicate {
            return Err(invalid(format!(
                "animation {animation_index} already animates the {path} of node {node}"
            )));
        }
        Ok(())
    }

    fn accessor_info(&self, index: usize) -> Result<AccessorInfo> {
        let a = self.gltf.accessors.as_ref().and_then(|a| a.get(index))
            .ok_or_else(|| invalid(format!("accessor {index} does not exist")))?;
        Ok(AccessorInfo { type_: a.type_.clone(), component_type: a.component_type, count: a.count })
    }
}

struct AccessorInfo {
    type_: String,
    component_type: usize,
    count: usize,
}

/// `LINEAR`/`STEP`: output count == input count. `CUBICSPLINE`: output == 3 * input, input >= 2.
fn check_output_count(input: usize, output: usize, interpolation: InterpolationType) -> std::result::Result<(), String> {
    match interpolation {
        InterpolationType::CubicSpline => {
            if input < 2 {
                Err("CUBICSPLINE needs at least two keyframes".to_string())
            } else if output != input * 3 {
                Err(format!("CUBICSPLINE needs 3 outputs per keyframe: expected {}, found {output}", input * 3))
            } else {
                Ok(())
            }
        }
        _ if output != input => Err(format!("expected {input} outputs (one per keyframe), found {output}")),
        _ => Ok(()),
    }
}

/// Plain value tracks can't express cubic-spline tangents; never invent them silently.
fn reject_cubic(interpolation: InterpolationType, cubic_fn: &str) -> Result<()> {
    if interpolation == InterpolationType::CubicSpline {
        return Err(invalid(format!(
            "CUBICSPLINE needs in/out tangents per keyframe: use {cubic_fn} with CubicKeyframe values"
        )));
    }
    Ok(())
}

fn validate_times(times: &[f32]) -> Result<()> {
    if times.is_empty() {
        return Err(invalid("an animation track needs at least one keyframe".to_string()));
    }
    for (i, &t) in times.iter().enumerate() {
        if !t.is_finite() || t < 0.0 {
            return Err(invalid(format!("keyframe {i}: time {t} must be finite and >= 0")));
        }
        if i > 0 && t <= times[i - 1] {
            return Err(invalid(format!(
                "keyframe {i}: time {t} must be greater than the previous time {}", times[i - 1]
            )));
        }
    }
    Ok(())
}

fn normalize_rotation(q: [f32; 4], keyframe: usize) -> Result<[f32; 4]> {
    let len = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if !len.is_finite() || len == 0.0 {
        return Err(invalid(format!("keyframe {keyframe}: rotation {q:?} cannot be normalized")));
    }
    Ok(q.map(|v| v / len))
}

/// `[in, value, out]` per keyframe, flattened.
fn flatten_cubic<const N: usize>(keyframes: &[CubicKeyframe<[f32; N]>], f: impl Fn(&[f32; N]) -> [f32; N]) -> Vec<f32> {
    keyframes
        .iter()
        .flat_map(|k| [f(&k.in_tangent), f(&k.value), f(&k.out_tangent)].into_iter().flatten())
        .collect()
}
