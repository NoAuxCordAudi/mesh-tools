mod common;

use gltf::animation::util::ReadOutputs;
use gltf::animation::Interpolation;
use mesh_tools::{AnimationPath, CubicKeyframe, GltfBuilder, GltfError, InterpolationType, JointIndices, MorphTarget, SkinSet};

const POS: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// A morphable mesh with `m` targets, a node using it, and one empty animation.
/// Returns (builder, animation, node, mesh).
fn setup(m: usize) -> (GltfBuilder, usize, usize, usize) {
    let mut b = GltfBuilder::new();
    let set = SkinSet { joints: JointIndices::U8(vec![[0; 4]; 3]), weights: vec![[1.0, 0.0, 0.0, 0.0]; 3] };
    let mut p = b.create_skinned_primitive(&POS, Some(&[0, 1, 2]), None, None, &[set], 1, None).unwrap();
    let targets: Vec<MorphTarget> = (0..m).map(|_| MorphTarget::new().with_positions(vec![0.1; 9])).collect();
    b.attach_morph_targets(&mut p, &targets).unwrap();
    let mesh = b.add_morphable_mesh(None, vec![p], None, None).unwrap();
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));
    let anim = b.add_animation(Some("A".into()));
    (b, anim, node, mesh)
}

fn counts(b: &GltfBuilder) -> (usize, usize, usize, usize, usize) {
    let a = b.gltf.animations.as_ref().map(|a| &a[0]);
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffers.as_ref().map_or(0, |a| a.first().map_or(0, |b| b.byte_length)),
        a.and_then(|a| a.samplers.as_ref()).map_or(0, |s| s.len()),
        a.and_then(|a| a.channels.as_ref()).map_or(0, |s| s.len()),
    )
}

fn msg<T: std::fmt::Debug>(r: mesh_tools::Result<T>) -> String {
    match r {
        Err(GltfError::InvalidInput(m)) => m,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

/// Read channel 0's weights output and interpolation back through the reference loader.
fn read_back(b: &GltfBuilder) -> (Interpolation, Vec<f32>, usize) {
    let glb = common::parse_glb(b);
    let chan = glb.document.animations().next().unwrap().channels().next().unwrap();
    let out = chan.sampler().output();
    assert_eq!(out.dimensions(), gltf::accessor::Dimensions::Scalar, "weights output is SCALAR");
    assert!(out.view().unwrap().target().is_none(), "animation data view has no target");
    let reader = chan.reader(|_| glb.blob.as_deref());
    let ReadOutputs::MorphTargetWeights(w) = reader.read_outputs().unwrap() else { panic!("expected weights output") };
    (chan.sampler().interpolation(), w.into_f32().collect(), out.count())
}

#[test]
fn linear_weights_round_trip_with_per_keyframe_layout() {
    let (mut b, anim, node, _) = setup(2);
    b.add_weights_track(anim, node, &[0.0, 1.0, 2.0], &[vec![0.0, 0.0], vec![1.0, 0.0], vec![0.5, 0.25]], InterpolationType::Linear).unwrap();
    let (interp, w, count) = read_back(&b);
    assert_eq!(interp, Interpolation::Linear);
    assert_eq!(w, vec![0.0, 0.0, 1.0, 0.0, 0.5, 0.25], "N=3 keyframes x M=2 targets, one row per keyframe");
    assert_eq!(count, 6, "accessor count is N*M");
    assert!(b.validate().is_valid(), "{:#?}", b.validate().errors);
}

#[test]
fn step_with_a_single_keyframe_and_a_single_target() {
    let (mut b, anim, node, _) = setup(1);
    b.add_weights_track(anim, node, &[0.0], &[vec![0.75]], InterpolationType::Step).unwrap();
    let (interp, w, count) = read_back(&b);
    assert_eq!((interp, w, count), (Interpolation::Step, vec![0.75], 1));
}

#[test]
fn weights_outside_zero_one_are_not_clamped() {
    let (mut b, anim, node, _) = setup(2);
    b.add_weights_track(anim, node, &[0.0, 1.0], &[vec![-1.0, 2.0], vec![3.5, -0.5]], InterpolationType::Linear).unwrap();
    assert_eq!(read_back(&b).1, vec![-1.0, 2.0, 3.5, -0.5]);
}

#[test]
fn cubic_weights_are_laid_out_in_value_out_per_keyframe() {
    let (mut b, anim, node, _) = setup(2);
    let kf = |i: [f32; 2], v: [f32; 2], o: [f32; 2]| CubicKeyframe { in_tangent: i.to_vec(), value: v.to_vec(), out_tangent: o.to_vec() };
    b.add_cubic_weights_track(
        anim,
        node,
        &[0.0, 1.0],
        &[kf([0.0, 0.0], [0.0, 0.5], [1.0, 2.0]), kf([3.0, 4.0], [1.0, 0.25], [0.0, 0.0])],
    )
    .unwrap();
    let (interp, w, count) = read_back(&b);
    assert_eq!(interp, Interpolation::CubicSpline);
    assert_eq!(count, 12, "3 * N * M");
    // keyframe 0: in(0,0) value(0,0.5) out(1,2); keyframe 1: in(3,4) value(1,0.25) out(0,0)
    assert_eq!(w, vec![0.0, 0.0, 0.0, 0.5, 1.0, 2.0, 3.0, 4.0, 1.0, 0.25, 0.0, 0.0]);
    assert!(b.validate().is_valid());
}

#[test]
fn plain_api_with_cubic_interpolation_points_to_the_cubic_api() {
    let (mut b, anim, node, _) = setup(1);
    let before = counts(&b);
    let m = msg(b.add_weights_track(anim, node, &[0.0, 1.0], &[vec![0.0], vec![1.0]], InterpolationType::CubicSpline));
    assert!(m.contains("add_cubic_weights_track"), "{m}");
    assert_eq!(counts(&b), before);
}

#[test]
fn the_target_count_comes_from_the_mesh_and_bad_rows_are_rejected_atomically() {
    let (mut b, anim, node, _) = setup(3);
    let before = counts(&b);
    let lin = InterpolationType::Linear;

    let m = msg(b.add_weights_track(anim, node, &[0.0, 1.0], &[vec![0.0; 3], vec![0.0; 2]], lin));
    assert!(m.contains("keyframe 1") && m.contains("2 weights") && m.contains("3 morph targets"), "ragged row: {m}");
    assert!(msg(b.add_weights_track(anim, node, &[0.0], &[vec![0.0; 4]], lin)).contains("4 weights"), "too wide");
    assert!(msg(b.add_weights_track(anim, node, &[0.0], &[vec![0.0, f32::NAN, 0.0]], lin)).contains("not finite"));
    assert!(msg(b.add_weights_track(anim, node, &[0.0, 1.0], &[vec![0.0; 3]], lin)).contains("same length"));
    assert!(msg(b.add_weights_track(anim, node, &[], &[], lin)).contains("at least one keyframe"));
    assert!(msg(b.add_weights_track(anim, node, &[1.0, 0.5], &[vec![0.0; 3], vec![0.0; 3]], lin)).contains("greater than the previous"));
    assert!(b.add_weights_track(9, node, &[0.0], &[vec![0.0; 3]], lin).is_err(), "bad animation index");

    let kf = |n: usize| CubicKeyframe { in_tangent: vec![0.0; n], value: vec![0.0; n], out_tangent: vec![0.0; n] };
    assert!(msg(b.add_cubic_weights_track(anim, node, &[0.0], &[kf(3)])).contains("at least two"));
    let bad = CubicKeyframe { in_tangent: vec![0.0; 3], value: vec![0.0; 3], out_tangent: vec![0.0; 2] };
    assert!(msg(b.add_cubic_weights_track(anim, node, &[0.0, 1.0], &[kf(3), bad])).contains("keyframe 1: out_tangent has 2"));
    assert_eq!(counts(&b), before, "failed calls must not write anything");
}

#[test]
fn nodes_without_a_morphable_mesh_are_rejected() {
    let (mut b, anim, _, _) = setup(2);
    let lin = InterpolationType::Linear;
    let bare = b.add_node(None, None, None, None, None);
    let plain_mesh = b.create_box(1.0);
    let plain = b.add_node(None, Some(plain_mesh), None, None, None);
    let dangling = b.add_node(None, Some(99), None, None, None);
    let before = counts(&b); // after all setup: only the failing calls below are measured

    assert!(msg(b.add_weights_track(anim, bare, &[0.0], &[vec![0.0; 2]], lin)).contains("has no mesh"));
    assert!(msg(b.add_weights_track(anim, plain, &[0.0], &[vec![0.0]], lin)).contains("cannot be animated"));
    assert!(msg(b.add_weights_track(anim, dangling, &[0.0], &[vec![0.0]], lin)).contains("mesh 99"));
    assert!(b.add_weights_track(anim, 999, &[0.0], &[vec![0.0]], lin).is_err(), "missing node");
    assert_eq!(counts(&b), before);
}

#[test]
fn one_weights_channel_per_node_and_animation() {
    let (mut b, anim, node, _) = setup(1);
    b.add_weights_track(anim, node, &[0.0], &[vec![0.0]], InterpolationType::Linear).unwrap();
    let before = counts(&b);
    assert!(msg(b.add_weights_track(anim, node, &[0.0], &[vec![1.0]], InterpolationType::Linear)).contains("already animates"));
    assert_eq!(counts(&b), before);
    let other = b.add_animation(None);
    assert!(b.add_weights_track(other, node, &[0.0], &[vec![1.0]], InterpolationType::Linear).is_ok());
}

#[test]
fn a_matrix_node_accepts_weights_but_not_trs_animation() {
    let (mut b, anim, node, _) = setup(1);
    b.gltf.nodes.as_mut().unwrap()[node].matrix = Some([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    b.add_weights_track(anim, node, &[0.0], &[vec![0.5]], InterpolationType::Linear).unwrap();
    assert!(msg(b.add_translation_track(anim, node, &[0.0], &[[0.0; 3]], InterpolationType::Linear)).contains("matrix"));
    assert!(b.validate().is_valid(), "validator must agree with the builder: {:#?}", b.validate().errors);
    common::parse_glb(&b);
}

#[test]
fn a_skinned_node_can_have_weights_animated() {
    let (mut b, anim, _, mesh) = setup(2);
    let joint = b.add_joint_node(None, None, None, None);
    let skin = b.add_skin_from_joints(None, vec![joint], Some(joint)).unwrap();
    let skinned = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.add_weights_track(anim, skinned, &[0.0, 1.0], &[vec![0.0, 1.0], vec![1.0, 0.0]], InterpolationType::Linear).unwrap();
    common::parse_glb(&b);
}

#[test]
fn low_level_api_checks_the_exact_count_when_the_channel_is_added() {
    let (mut b, anim, node, _) = setup(2);
    b.add_weights_track(anim, node, &[0.0, 1.0, 2.0], &[vec![0.0; 2], vec![0.5; 2], vec![1.0; 2]], InterpolationType::Linear).unwrap();
    let s = &b.gltf.animations.as_ref().unwrap()[0].samplers.as_ref().unwrap()[0];
    let (input, output) = (s.input, s.output); // N=3 inputs, 6 SCALAR outputs

    // Sampler: a SCALAR output may be any multiple of N because M is unknown at this level...
    let other = b.add_animation(None);
    let lin = b.add_animation_sampler(other, input, output, InterpolationType::Linear).unwrap();
    // ...but 6 is not a multiple of 3*N = 9, so it cannot drive a cubic spline.
    assert!(msg(b.add_animation_sampler(other, input, output, InterpolationType::CubicSpline)).contains("multiple of 9"));

    // Channel: node M=2 -> 6 values is exactly right.
    assert!(b.add_animation_channel(other, lin, node, AnimationPath::Weights).is_ok());

    // A node whose mesh has M=3 needs 9 values; the same sampler is wrong for it.
    let (m3_mesh_node, _) = {
        let set = SkinSet { joints: JointIndices::U8(vec![[0; 4]; 3]), weights: vec![[1.0, 0.0, 0.0, 0.0]; 3] };
        let mut p = b.create_skinned_primitive(&POS, Some(&[0, 1, 2]), None, None, &[set], 1, None).unwrap();
        let targets: Vec<MorphTarget> = (0..3).map(|_| MorphTarget::new().with_positions(vec![0.0; 9])).collect();
        b.attach_morph_targets(&mut p, &targets).unwrap();
        let mesh = b.add_morphable_mesh(None, vec![p], None, None).unwrap();
        (b.add_node(None, Some(mesh), None, None, None), mesh)
    };
    let third = b.add_animation(None);
    let s3 = b.add_animation_sampler(third, input, output, InterpolationType::Linear).unwrap();
    let m = msg(b.add_animation_channel(third, s3, m3_mesh_node, AnimationPath::Weights));
    assert!(m.contains("6 weight values for 3 keyframes") && m.contains("3 morph targets") && m.contains("9 expected"), "{m}");
}

#[test]
fn low_level_channel_count_accounts_for_cubic_interpolation() {
    let (mut b, anim, node, mesh) = setup(2);
    let kf = |v: f32| CubicKeyframe { in_tangent: vec![0.0; 2], value: vec![v; 2], out_tangent: vec![0.0; 2] };
    b.add_cubic_weights_track(anim, node, &[0.0, 1.0], &[kf(0.0), kf(1.0)]).unwrap();
    let s = &b.gltf.animations.as_ref().unwrap()[0].samplers.as_ref().unwrap()[0];
    let (input, output) = (s.input, s.output); // N=2, 12 SCALAR outputs = 3*N*M with M=2

    let other_node = b.add_node(None, Some(mesh), None, None, None);
    let other = b.add_animation(None);
    let cubic = b.add_animation_sampler(other, input, output, InterpolationType::CubicSpline).unwrap();
    assert!(b.add_animation_channel(other, cubic, other_node, AnimationPath::Weights).is_ok(), "3*N*M is right for cubic");

    // The same 12 values under a LINEAR sampler would need N*M = 4, so the channel must refuse.
    let third = b.add_animation(None);
    let linear = b.add_animation_sampler(third, input, output, InterpolationType::Linear).unwrap();
    let m = msg(b.add_animation_channel(third, linear, other_node, AnimationPath::Weights));
    assert!(m.contains("12 weight values for 2 keyframes") && m.contains("4 expected"), "{m}");
}
