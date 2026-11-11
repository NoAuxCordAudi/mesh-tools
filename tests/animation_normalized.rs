//! Normalized-integer rotation and weights outputs, and sampler reuse across channels.
//! Expected integers are derived by hand from the spec formulas: signed `c = round(f * max)`
//! (so `-128`/`-32768` never occur), unsigned `c = round(f * max)`.

mod common;

use gltf::animation::util::{MorphTargetWeights, ReadOutputs, Rotations};
use mesh_tools::{AnimationPath, CubicKeyframe, GltfBuilder, GltfError, InterpolationType, JointIndices, MorphTarget, NormalizedComponent, SkinSet};

const POS: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
const LIN: InterpolationType = InterpolationType::Linear;

fn err_msg<T: std::fmt::Debug>(r: mesh_tools::Result<T>) -> String {
    match r {
        Err(GltfError::InvalidInput(m)) => m,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

/// Builder, animation, a plain node and a node whose mesh has `m` morph targets.
fn setup(m: usize) -> (GltfBuilder, usize, usize, usize) {
    let mut b = GltfBuilder::new();
    let plain = b.add_node(None, None, None, None, None);
    let set = SkinSet::new(JointIndices::U8(vec![[0; 4]; 3]), vec![[1.0, 0.0, 0.0, 0.0]; 3]);
    let mut p = b.create_skinned_primitive(&POS, Some(&[0, 1, 2]), None, None, &[set], 1, None).unwrap();
    let targets: Vec<MorphTarget> = (0..m).map(|_| MorphTarget::new().with_positions(vec![0.0; 9])).collect();
    b.attach_morph_targets(&mut p, &targets).unwrap();
    let mesh = b.add_morphable_mesh(None, vec![p], None, None).unwrap();
    let morph = b.add_node(None, Some(mesh), None, None, None);
    let joint = b.add_joint_node(None, None, None, None);
    let skin = b.add_skin(None, vec![joint], None, None).unwrap();
    b.set_node_skin(morph, skin).unwrap();
    b.add_scene(None, Some(vec![plain, morph]));
    let a = b.add_animation(None);
    (b, a, plain, morph)
}

fn counts(b: &GltfBuilder) -> (usize, usize, usize) {
    let a = &b.gltf.animations.as_ref().unwrap()[0];
    (b.gltf.accessors.as_ref().unwrap().len(), a.samplers.as_ref().map_or(0, Vec::len), a.channels.as_ref().map_or(0, Vec::len))
}

fn rotations(b: &GltfBuilder) -> Rotations<'static> {
    // Leak the parsed document so the reader's borrow can outlive this helper.
    let glb: &'static gltf::Gltf = Box::leak(Box::new(common::parse_glb(b)));
    let chan = glb.document.animations().next().unwrap().channels().next().unwrap();
    match chan.reader(|_| glb.blob.as_deref()).read_outputs().unwrap() {
        ReadOutputs::Rotations(r) => r,
        _ => panic!("expected rotations"),
    }
}

fn weights(b: &GltfBuilder) -> MorphTargetWeights<'static> {
    let glb: &'static gltf::Gltf = Box::leak(Box::new(common::parse_glb(b)));
    let chan = glb.document.animations().next().unwrap().channels().next().unwrap();
    match chan.reader(|_| glb.blob.as_deref()).read_outputs().unwrap() {
        ReadOutputs::MorphTargetWeights(w) => w,
        _ => panic!("expected weights"),
    }
}

const R: [[f32; 4]; 3] = [[0.0, 0.0, 0.0, 1.0], [-1.0, 0.0, 0.0, 0.0], [0.0, 0.70710678, 0.0, 0.70710678]];

#[test]
fn rotation_i8_uses_round_times_127_and_never_minus_128() {
    let (mut b, a, plain, _) = setup(1);
    b.add_rotation_track_normalized(a, plain, &[0.0, 1.0, 2.0], &R, LIN, NormalizedComponent::I8).unwrap();
    // 0.70710678 * 127 = 89.8 -> 90; -1 -> -127
    let Rotations::I8(it) = rotations(&b) else { panic!("expected I8") };
    assert_eq!(it.collect::<Vec<_>>(), [[0, 0, 0, 127], [-127, 0, 0, 0], [0, 90, 0, 90]]);
    let out = b.gltf.animations.as_ref().unwrap()[0].samplers.as_ref().unwrap()[0].output;
    let acc = &b.gltf.accessors.as_ref().unwrap()[out];
    assert_eq!((acc.component_type, acc.normalized, acc.type_.as_str()), (5120, Some(true), "VEC4"));
    assert!(b.validate().is_valid(), "{:#?}", b.validate());
}

#[test]
fn rotation_i16_u8_u16() {
    let (mut b, a, plain, _) = setup(1);
    b.add_rotation_track_normalized(a, plain, &[0.0, 1.0, 2.0], &R, LIN, NormalizedComponent::I16).unwrap();
    // 0.70710678 * 32767 = 23169.6 -> 23170
    let Rotations::I16(it) = rotations(&b) else { panic!() };
    assert_eq!(it.collect::<Vec<_>>(), [[0, 0, 0, 32767], [-32767, 0, 0, 0], [0, 23170, 0, 23170]]);

    // Unsigned types hold only non-negative components.
    let pos = [[0.0, 0.0, 0.0, 1.0], [0.0, 0.70710678, 0.0, 0.70710678]];
    let (mut b, a, plain, _) = setup(1);
    b.add_rotation_track_normalized(a, plain, &[0.0, 1.0], &pos, LIN, NormalizedComponent::U8).unwrap();
    let Rotations::U8(it) = rotations(&b) else { panic!() };
    assert_eq!(it.collect::<Vec<_>>(), [[0, 0, 0, 255], [0, 180, 0, 180]]); // 0.7071*255 = 180.3
    let (mut b, a, plain, _) = setup(1);
    b.add_rotation_track_normalized(a, plain, &[0.0, 1.0], &pos, LIN, NormalizedComponent::U16).unwrap();
    let Rotations::U16(it) = rotations(&b) else { panic!() };
    assert_eq!(it.collect::<Vec<_>>(), [[0, 0, 0, 65535], [0, 46340, 0, 46340]]); // 0.7071*65535 = 46340.4
    assert!(b.validate().is_valid(), "{:#?}", b.validate());
}

#[test]
fn rotation_input_is_normalized_before_quantizing() {
    let (mut b, a, plain, _) = setup(1);
    b.add_rotation_track_normalized(a, plain, &[0.0], &[[0.0, 0.0, 0.0, 2.0]], LIN, NormalizedComponent::I8).unwrap();
    let Rotations::I8(it) = rotations(&b) else { panic!() };
    assert_eq!(it.collect::<Vec<_>>(), [[0, 0, 0, 127]]);
}

#[test]
fn unsigned_rotation_rejects_negative_components_atomically() {
    let (mut b, a, plain, _) = setup(1);
    let before = counts(&b);
    let m = err_msg(b.add_rotation_track_normalized(a, plain, &[0.0], &[[0.0, 0.0, 0.0, -1.0]], LIN, NormalizedComponent::U8));
    assert!(m.contains("negative") && m.contains("keyframe 0"), "{m}");
    assert_eq!(counts(&b), before);
}

#[test]
fn normalized_rotation_keeps_all_the_float_track_rules() {
    let (mut b, a, plain, _) = setup(1);
    let before = counts(&b);
    for (times, vals) in [(vec![1.0, 1.0], vec![[0.0, 0.0, 0.0, 1.0]; 2]), (vec![0.0], vec![[0.0; 4]]), (vec![0.0], vec![[f32::NAN, 0.0, 0.0, 1.0]])] {
        err_msg(b.add_rotation_track_normalized(a, plain, &times, &vals, LIN, NormalizedComponent::I8));
    }
    err_msg(b.add_rotation_track_normalized(a, plain, &[0.0], &[[0.0, 0.0, 0.0, 1.0]], InterpolationType::CubicSpline, NormalizedComponent::I8));
    assert_eq!(counts(&b), before);
    b.add_rotation_track_normalized(a, plain, &[0.0], &[[0.0, 0.0, 0.0, 1.0]], LIN, NormalizedComponent::I8).unwrap();
    let m = err_msg(b.add_rotation_track_normalized(a, plain, &[0.0], &[[0.0, 0.0, 0.0, 1.0]], LIN, NormalizedComponent::I16));
    assert!(m.contains("already animates"), "{m}");
}

#[test]
fn cubic_rotation_quantizes_values_and_tangents_in_in_value_out_order() {
    let (mut b, a, plain, _) = setup(1);
    let kf = |t: f32, v: [f32; 4]| CubicKeyframe { in_tangent: [t; 4], value: v, out_tangent: [-t; 4] };
    b.add_cubic_rotation_track_normalized(
        a, plain, &[0.0, 1.0], &[kf(0.5, [0.0, 0.0, 0.0, 1.0]), kf(0.0, [0.0, 1.0, 0.0, 0.0])], NormalizedComponent::I8,
    )
    .unwrap();
    // 0.5 * 127 = 63.5 -> 64 (half away from zero)
    let Rotations::I8(it) = rotations(&b) else { panic!() };
    assert_eq!(
        it.collect::<Vec<_>>(),
        [[64; 4], [0, 0, 0, 127], [-64; 4], [0; 4], [0, 127, 0, 0], [0; 4]]
    );
    assert!(b.validate().is_valid(), "{:#?}", b.validate());
}

#[test]
fn cubic_tangents_outside_the_representable_range_are_rejected() {
    let (mut b, a, plain, _) = setup(1);
    let before = counts(&b);
    let kf = |t: f32| CubicKeyframe { in_tangent: [t, 0.0, 0.0, 0.0], value: [0.0, 0.0, 0.0, 1.0], out_tangent: [0.0; 4] };
    let m = err_msg(b.add_cubic_rotation_track_normalized(a, plain, &[0.0, 1.0], &[kf(1.5), kf(0.0)], NormalizedComponent::I16));
    assert!(m.contains("tangent") && m.contains("[-1, 1]"), "{m}");
    assert_eq!(counts(&b), before);
}

#[test]
fn weights_formulas_and_ranges() {
    let (mut b, a, _, morph) = setup(2);
    b.add_weights_track_normalized(a, morph, &[0.0, 1.0], &[vec![0.0, 1.0], vec![0.2, 0.5]], LIN, NormalizedComponent::U8).unwrap();
    // 0.2*255 = 51, 0.5*255 = 127.5 -> 128
    let MorphTargetWeights::U8(it) = weights(&b) else { panic!() };
    assert_eq!(it.collect::<Vec<_>>(), [0, 255, 51, 128]);

    let (mut b, a, _, morph) = setup(2);
    b.add_weights_track_normalized(a, morph, &[0.0, 1.0], &[vec![-1.0, 1.0], vec![0.25, -0.5]], LIN, NormalizedComponent::I8).unwrap();
    // 0.25*127 = 31.75 -> 32, -0.5*127 = -63.5 -> -64
    let MorphTargetWeights::I8(it) = weights(&b) else { panic!() };
    assert_eq!(it.collect::<Vec<_>>(), [-127, 127, 32, -64]);

    let (mut b, a, _, morph) = setup(1);
    b.add_weights_track_normalized(a, morph, &[0.0, 1.0], &[vec![-1.0], vec![0.5]], LIN, NormalizedComponent::I16).unwrap();
    let MorphTargetWeights::I16(it) = weights(&b) else { panic!() };
    assert_eq!(it.collect::<Vec<_>>(), [-32767, 16384]); // 16383.5 -> 16384
    assert!(b.validate().is_valid(), "{:#?}", b.validate());

    let (mut b, a, _, morph) = setup(1);
    b.add_weights_track_normalized(a, morph, &[0.0, 1.0], &[vec![0.5], vec![1.0]], LIN, NormalizedComponent::U16).unwrap();
    let MorphTargetWeights::U16(it) = weights(&b) else { panic!() };
    assert_eq!(it.collect::<Vec<_>>(), [32768, 65535]); // 32767.5 -> 32768
}

#[test]
fn weights_outside_the_integer_range_are_rejected_atomically() {
    let (mut b, a, _, morph) = setup(2);
    let before = counts(&b);
    let m = err_msg(b.add_weights_track_normalized(a, morph, &[0.0], &[vec![0.0, 1.5]], LIN, NormalizedComponent::U8));
    assert!(m.contains("keyframe 0") && m.contains("[0, 1]"), "{m}");
    err_msg(b.add_weights_track_normalized(a, morph, &[0.0], &[vec![-0.1, 0.0]], LIN, NormalizedComponent::U16));
    err_msg(b.add_weights_track_normalized(a, morph, &[0.0], &[vec![0.0, -1.1]], LIN, NormalizedComponent::I8));
    err_msg(b.add_weights_track_normalized(a, morph, &[0.0], &[vec![0.0]], LIN, NormalizedComponent::I8)); // wrong width
    assert_eq!(counts(&b), before);
}

#[test]
fn cubic_weights_normalized() {
    let (mut b, a, _, morph) = setup(2);
    let kf = |v: f32| CubicKeyframe { in_tangent: vec![0.0; 2], value: vec![v; 2], out_tangent: vec![v / 2.0; 2] };
    b.add_cubic_weights_track_normalized(a, morph, &[0.0, 1.0], &[kf(0.0), kf(1.0)], NormalizedComponent::U8).unwrap();
    let MorphTargetWeights::U8(it) = weights(&b) else { panic!() };
    // out tangent of keyframe 1 is 0.5 -> 128
    assert_eq!(it.collect::<Vec<_>>(), [0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 128, 128]);
    assert!(b.validate().is_valid(), "{:#?}", b.validate());
    let (mut b, a, _, morph) = setup(2);
    assert!(err_msg(b.add_cubic_weights_track_normalized(a, morph, &[0.0], &[kf(0.0)], NormalizedComponent::U8)).contains("two keyframes"));
}

#[test]
fn low_level_channels_reject_integer_translation_and_scale_outputs() {
    let (mut b, a, plain, _) = setup(1);
    let input = b.add_accessor_from_slice(&[0.0f32, 1.0], mesh_tools::AccessorType::Scalar, mesh_tools::AccessorSpec::new().with_bounds(mesh_tools::Bounds::Compute)).unwrap();
    let out = b
        .add_accessor_from_slice(&[0i8, 0, 0, 127, 0, 0], mesh_tools::AccessorType::Vec3, mesh_tools::AccessorSpec::new().with_normalized(true))
        .unwrap();
    let s = b.add_animation_sampler(a, input, out, LIN).unwrap();
    let before = counts(&b);
    for path in [AnimationPath::Translation, AnimationPath::Scale] {
        let m = err_msg(b.add_animation_channel(a, s, plain, path));
        assert!(m.contains("FLOAT"), "{m}");
    }
    assert_eq!(counts(&b), before);
    // A non-normalized integer rotation output is also illegal.
    let raw = b.add_accessor_from_slice(&[0i8; 8], mesh_tools::AccessorType::Vec4, mesh_tools::AccessorSpec::new()).unwrap();
    let s2 = b.add_animation_sampler(a, input, raw, LIN).unwrap();
    assert!(err_msg(b.add_animation_channel(a, s2, plain, AnimationPath::Rotation)).contains("normalized"));
    // Normalized rotation is fine.
    let ok = b.add_accessor_from_slice(&[0i8, 0, 0, 127, 0, 0, 0, 127], mesh_tools::AccessorType::Vec4, mesh_tools::AccessorSpec::new().with_normalized(true)).unwrap();
    let s3 = b.add_animation_sampler(a, input, ok, LIN).unwrap();
    b.add_animation_channel(a, s3, plain, AnimationPath::Rotation).unwrap();
}

#[test]
fn one_sampler_can_drive_channels_on_several_nodes() {
    let mut b = GltfBuilder::new();
    let n1 = b.add_node(None, None, None, None, None);
    let n2 = b.add_node(None, None, None, None, None);
    let n3 = b.add_node(None, None, None, None, None);
    b.add_scene(None, Some(vec![n1, n2, n3]));
    let a = b.add_animation(None);
    let (_, sampler) = b.add_translation_track(a, n1, &[0.0, 1.0], &[[0.0; 3], [1.0, 2.0, 3.0]], LIN).unwrap();
    let accessors = b.gltf.accessors.as_ref().unwrap().len();
    let c2 = b.add_channel_with_sampler(a, sampler, n2, AnimationPath::Translation).unwrap();
    let c3 = b.add_channel_with_sampler(a, sampler, n3, AnimationPath::Translation).unwrap();
    assert_eq!((c2, c3), (1, 2));
    assert_eq!(b.gltf.accessors.as_ref().unwrap().len(), accessors, "no new data written");
    assert!(b.validate().is_valid());
    let glb = common::parse_glb(&b);
    let anim = glb.document.animations().next().unwrap();
    assert_eq!(anim.samplers().count(), 1);
    assert_eq!(anim.channels().count(), 3);
    for c in anim.channels() {
        let r = c.reader(|_| glb.blob.as_deref());
        let ReadOutputs::Translations(t) = r.read_outputs().unwrap() else { panic!() };
        assert_eq!(t.collect::<Vec<_>>(), [[0.0; 3], [1.0, 2.0, 3.0]]);
    }
    // Errors: duplicate target, wrong path type, bad sampler; each leaves the document unchanged.
    let before = counts(&b);
    assert!(err_msg(b.add_channel_with_sampler(a, sampler, n2, AnimationPath::Translation)).contains("already animates"));
    assert!(err_msg(b.add_channel_with_sampler(a, sampler, n2, AnimationPath::Rotation)).contains("VEC3"));
    assert!(err_msg(b.add_channel_with_sampler(a, 9, n2, AnimationPath::Scale)).contains("no sampler 9"));
    assert_eq!(counts(&b), before);
}

#[test]
fn a_sampler_with_unordered_stored_times_is_rejected() {
    let (mut b, a, _, _) = setup(1);
    let bad = b.add_accessor_from_slice(&[1.0f32, 0.5], mesh_tools::AccessorType::Scalar, mesh_tools::AccessorSpec::new().with_bounds(mesh_tools::Bounds::Compute)).unwrap();
    let out = b.add_accessor_from_slice(&[0.0f32, 1.0], mesh_tools::AccessorType::Scalar, mesh_tools::AccessorSpec::new()).unwrap();
    let neg = b.add_accessor_from_slice(&[-1.0f32, 0.5], mesh_tools::AccessorType::Scalar, mesh_tools::AccessorSpec::new().with_bounds(mesh_tools::Bounds::Compute)).unwrap();
    let before = counts(&b);
    let m = err_msg(b.add_animation_sampler(a, bad, out, LIN));
    assert!(m.contains("strictly increasing") || m.contains("greater than the previous"), "{m}");
    assert!(err_msg(b.add_animation_sampler(a, neg, out, LIN)).contains(">= 0"));
    assert_eq!(counts(&b), before);
}
