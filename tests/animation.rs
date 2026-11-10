mod common;

use gltf::animation::util::ReadOutputs;
use gltf::animation::Interpolation;
use mesh_tools::{AnimationPath, CubicKeyframe, GltfBuilder, GltfError, InterpolationType};

const EPS: f32 = 1e-6;

/// Builder with one animation and `n` plain nodes. Returns (builder, animation, nodes).
fn setup(n: usize) -> (GltfBuilder, usize, Vec<usize>) {
    let mut b = GltfBuilder::new();
    let nodes = (0..n).map(|_| b.add_node(None, None, None, None, None)).collect();
    let anim = b.add_animation(Some("A".into()));
    (b, anim, nodes)
}

fn counts(b: &GltfBuilder) -> (usize, usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.gltf.animations.as_ref().map_or(0, |a| a[0].samplers.as_ref().map_or(0, |s| s.len())),
        b.gltf.animations.as_ref().map_or(0, |a| a[0].channels.as_ref().map_or(0, |s| s.len())),
    )
}

fn err_msg<T: std::fmt::Debug>(r: mesh_tools::Result<T>) -> String {
    match r {
        Err(GltfError::InvalidInput(m)) => m,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[test]
fn translation_track_round_trips_with_input_bounds_and_no_view_target() {
    let (mut b, anim, nodes) = setup(1);
    let times = [0.0, 0.5, 2.0];
    let values = [[0.0, 0.0, 0.0], [1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
    let (channel, sampler) = b.add_translation_track(anim, nodes[0], &times, &values, InterpolationType::Linear).unwrap();
    assert_eq!((channel, sampler), (0, 0));

    let glb = common::parse_glb(&b);
    let chan = glb.document.animations().next().unwrap().channels().next().unwrap();
    assert_eq!(chan.sampler().interpolation(), Interpolation::Linear);
    let input = chan.sampler().input();
    let as_f = |v: gltf::json::Value| v.as_array().unwrap()[0].as_f64().unwrap();
    assert_eq!(as_f(input.min().unwrap()), 0.0);
    assert_eq!(as_f(input.max().unwrap()), 2.0);
    assert!(input.view().unwrap().target().is_none(), "time view must have no target");
    assert!(chan.sampler().output().view().unwrap().target().is_none());

    let reader = chan.reader(|_| glb.blob.as_deref());
    assert_eq!(reader.read_inputs().unwrap().collect::<Vec<_>>(), times);
    match reader.read_outputs().unwrap() {
        ReadOutputs::Translations(it) => assert_eq!(it.collect::<Vec<_>>(), values),
        _ => panic!("expected translations"),
    }
}

#[test]
fn rotations_are_normalized_and_scale_round_trips() {
    let (mut b, anim, nodes) = setup(2);
    b.add_rotation_track(anim, nodes[0], &[0.0, 1.0], &[[0.0, 0.0, 0.0, 2.0], [0.0, 3.0, 0.0, 4.0]], InterpolationType::Step).unwrap();
    b.add_scale_track(anim, nodes[1], &[0.0, 1.0], &[[1.0; 3], [2.0; 3]], InterpolationType::Linear).unwrap();

    let glb = common::parse_glb(&b);
    let mut chans = glb.document.animations().next().unwrap().channels();
    let rot = chans.next().unwrap();
    assert_eq!(rot.sampler().interpolation(), Interpolation::Step);
    match rot.reader(|_| glb.blob.as_deref()).read_outputs().unwrap() {
        ReadOutputs::Rotations(r) => {
            let q: Vec<[f32; 4]> = r.into_f32().collect();
            assert_eq!(q[0], [0.0, 0.0, 0.0, 1.0]);
            assert!((q[1][1] - 0.6).abs() < EPS && (q[1][3] - 0.8).abs() < EPS);
        }
        _ => panic!("expected rotations"),
    }
    match chans.next().unwrap().reader(|_| glb.blob.as_deref()).read_outputs().unwrap() {
        ReadOutputs::Scales(s) => assert_eq!(s.collect::<Vec<_>>(), vec![[1.0; 3], [2.0; 3]]),
        _ => panic!("expected scales"),
    }
}

#[test]
fn cubic_track_writes_in_value_out_per_keyframe() {
    let (mut b, anim, nodes) = setup(2);
    let kf = |a: f32| CubicKeyframe { in_tangent: [a, 0.0, 0.0], value: [a, 1.0, 0.0], out_tangent: [a, 2.0, 0.0] };
    b.add_cubic_translation_track(anim, nodes[0], &[0.0, 1.0], &[kf(1.0), kf(2.0)]).unwrap();
    let rq = |w: f32| CubicKeyframe { in_tangent: [0.0, 0.0, 0.0, 5.0], value: [0.0, 0.0, 0.0, w], out_tangent: [0.0, 0.0, 0.0, 5.0] };
    b.add_cubic_rotation_track(anim, nodes[1], &[0.0, 1.0], &[rq(2.0), rq(4.0)]).unwrap();

    let glb = common::parse_glb(&b);
    let mut chans = glb.document.animations().next().unwrap().channels();
    let t = chans.next().unwrap();
    assert_eq!(t.sampler().interpolation(), Interpolation::CubicSpline);
    assert_eq!(t.sampler().output().count(), 6, "3 outputs per keyframe");
    match t.reader(|_| glb.blob.as_deref()).read_outputs().unwrap() {
        ReadOutputs::Translations(it) => assert_eq!(
            it.collect::<Vec<_>>(),
            vec![[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 2.0, 0.0], [2.0, 0.0, 0.0], [2.0, 1.0, 0.0], [2.0, 2.0, 0.0]]
        ),
        _ => panic!(),
    }
    match chans.next().unwrap().reader(|_| glb.blob.as_deref()).read_outputs().unwrap() {
        ReadOutputs::Rotations(r) => {
            let q: Vec<[f32; 4]> = r.into_f32().collect();
            assert_eq!(q[1], [0.0, 0.0, 0.0, 1.0], "value normalized");
            assert_eq!(q[0], [0.0, 0.0, 0.0, 5.0], "tangents untouched");
        }
        _ => panic!(),
    }
}

#[test]
fn plain_api_with_cubic_interpolation_points_to_the_cubic_api() {
    let (mut b, anim, nodes) = setup(1);
    let before = counts(&b);
    let m = err_msg(b.add_translation_track(anim, nodes[0], &[0.0, 1.0], &[[0.0; 3], [1.0; 3]], InterpolationType::CubicSpline));
    assert!(m.contains("add_cubic_translation_track"), "{m}");
    assert!(err_msg(b.add_rotation_track(anim, nodes[0], &[0.0, 1.0], &[[0.0, 0.0, 0.0, 1.0]; 2], InterpolationType::CubicSpline)).contains("add_cubic_rotation_track"));
    assert!(err_msg(b.add_scale_track(anim, nodes[0], &[0.0, 1.0], &[[1.0; 3]; 2], InterpolationType::CubicSpline)).contains("add_cubic_scale_track"));
    assert_eq!(counts(&b), before);
}

#[test]
fn cubic_needs_two_keyframes_and_matching_times() {
    let (mut b, anim, nodes) = setup(1);
    let kf = CubicKeyframe { in_tangent: [0.0; 3], value: [0.0; 3], out_tangent: [0.0; 3] };
    assert!(err_msg(b.add_cubic_translation_track(anim, nodes[0], &[0.0], &[kf])).contains("at least two"));
    assert!(b.add_cubic_translation_track(anim, nodes[0], &[0.0, 1.0], &[kf]).is_err());
}

#[test]
fn rejects_bad_times_and_values_without_writing() {
    let (mut b, anim, nodes) = setup(1);
    let before = counts(&b);
    let v = |n: usize| vec![[0.0f32; 3]; n];
    let lin = InterpolationType::Linear;
    let mut t = |times: &[f32], vals: &[[f32; 3]]| err_msg(b.add_translation_track(anim, nodes[0], times, vals, lin));

    assert!(t(&[], &[]).contains("at least one keyframe"));
    assert!(t(&[0.0, 0.0], &v(2)).contains("keyframe 1"), "equal times");
    assert!(t(&[1.0, 0.5], &v(2)).contains("greater than the previous"));
    assert!(t(&[-1.0, 0.0], &v(2)).contains("keyframe 0"));
    assert!(t(&[0.0, f32::NAN], &v(2)).contains("keyframe 1"));
    assert!(t(&[0.0, f32::INFINITY], &v(2)).contains("keyframe 1"));
    assert!(t(&[0.0, 1.0], &v(3)).contains("same length"));
    assert!(t(&[0.0, 1.0], &[[0.0; 3], [f32::NAN, 0.0, 0.0]]).contains("not finite"));

    assert!(err_msg(b.add_rotation_track(anim, nodes[0], &[0.0], &[[0.0; 4]], lin)).contains("cannot be normalized"));
    assert!(err_msg(b.add_rotation_track(anim, nodes[0], &[0.0], &[[f32::NAN, 0.0, 0.0, 1.0]], lin)).contains("cannot be normalized"));
    assert_eq!(counts(&b), before, "failed calls must not write anything");
}

#[test]
fn single_keyframe_is_allowed() {
    let (mut b, anim, nodes) = setup(1);
    b.add_translation_track(anim, nodes[0], &[0.0], &[[1.0, 2.0, 3.0]], InterpolationType::Step).unwrap();
    common::parse_glb(&b);
}

#[test]
fn bad_indices_are_errors_not_panics() {
    let (mut b, anim, nodes) = setup(1);
    let before = counts(&b);
    let lin = InterpolationType::Linear;
    assert!(err_msg(b.add_translation_track(9, nodes[0], &[0.0], &[[0.0; 3]], lin)).contains("animation 9"));
    assert!(err_msg(b.add_translation_track(anim, 9, &[0.0], &[[0.0; 3]], lin)).contains("node 9"));
    assert!(b.add_animation_sampler(9, 0, 0, lin).is_err());
    assert!(b.add_animation_channel(9, 0, 0, AnimationPath::Translation).is_err());
    assert!(b.add_animation_channel(anim, 0, nodes[0], AnimationPath::Translation).is_err(), "no sampler 0 yet");
    assert!(b.add_animation_sampler(anim, 77, 78, lin).is_err(), "missing accessors");
    assert_eq!(counts(&b), before);

    // A fresh builder with no animations at all must not panic either.
    let mut empty = GltfBuilder::new();
    assert!(empty.add_translation_track(0, 0, &[0.0], &[[0.0; 3]], lin).is_err());
}

#[test]
fn duplicate_node_path_in_one_animation_is_rejected_but_other_animations_are_fine() {
    let (mut b, anim, nodes) = setup(1);
    let lin = InterpolationType::Linear;
    b.add_translation_track(anim, nodes[0], &[0.0], &[[0.0; 3]], lin).unwrap();
    let before = counts(&b);
    assert!(err_msg(b.add_translation_track(anim, nodes[0], &[0.0], &[[1.0; 3]], lin)).contains("already animates"));
    assert_eq!(counts(&b), before);
    // A different path on the same node, and the same path in another animation, are valid.
    b.add_scale_track(anim, nodes[0], &[0.0], &[[1.0; 3]], lin).unwrap();
    let other = b.add_animation(None);
    b.add_translation_track(other, nodes[0], &[0.0], &[[1.0; 3]], lin).unwrap();
}

#[test]
fn nodes_with_a_matrix_cannot_be_animated() {
    let (mut b, anim, nodes) = setup(1);
    b.gltf.nodes.as_mut().unwrap()[nodes[0]].matrix = Some([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    assert!(err_msg(b.add_translation_track(anim, nodes[0], &[0.0], &[[0.0; 3]], InterpolationType::Linear)).contains("matrix"));
}

#[test]
fn weights_path_is_rejected_explicitly() {
    let (mut b, anim, nodes) = setup(1);
    let (_, sampler) = b.add_translation_track(anim, nodes[0], &[0.0], &[[0.0; 3]], InterpolationType::Linear).unwrap();
    assert!(err_msg(b.add_animation_channel(anim, sampler, nodes[0], AnimationPath::Weights)).contains("not supported"));
}

#[test]
fn low_level_sampler_and_channel_validate_types_and_counts() {
    let (mut b, anim, nodes) = setup(2);
    let lin = InterpolationType::Linear;
    // Build a valid VEC3 track, then misuse its accessors through the low-level API.
    b.add_translation_track(anim, nodes[0], &[0.0, 1.0], &[[0.0; 3], [1.0; 3]], lin).unwrap();
    let samplers = b.gltf.animations.as_ref().unwrap()[0].samplers.as_ref().unwrap();
    let (input, output) = (samplers[0].input, samplers[0].output);

    // Output used as input: not SCALAR.
    assert!(err_msg(b.add_animation_sampler(anim, output, output, lin)).contains("SCALAR"));
    // 2 inputs cannot drive a cubic sampler with only 2 outputs (needs 6).
    assert!(err_msg(b.add_animation_sampler(anim, input, output, InterpolationType::CubicSpline)).contains("3 outputs per keyframe"));
    // Valid second sampler over the same accessors, but a VEC3 output cannot drive rotation.
    let s = b.add_animation_sampler(anim, input, output, lin).unwrap();
    assert!(err_msg(b.add_animation_channel(anim, s, nodes[1], AnimationPath::Rotation)).contains("VEC3"));
    assert!(b.add_animation_channel(anim, s, nodes[1], AnimationPath::Scale).is_ok());
}

#[test]
fn create_wrappers_return_errors_instead_of_panicking() {
    let (mut b, anim, nodes) = setup(1);
    let lin = InterpolationType::Linear;
    assert!(b.create_translation_animation(anim, nodes[0], vec![0.0, 1.0], vec![[0.0; 3]], lin).is_err());
    assert!(b.create_rotation_animation(anim, nodes[0], vec![0.0], vec![], lin).is_err());
    assert!(b.create_scale_animation(anim, nodes[0], vec![0.0], vec![[1.0; 3]], lin).is_ok());
}

#[test]
fn empty_animation_collections_are_not_serialized() {
    fn json(b: &GltfBuilder) -> serde_json::Value {
        // Unchecked: an empty animation is (correctly) a validation error, but we want to
        // inspect what would be serialized.
        let bytes = b.to_glb_bytes_unchecked().unwrap();
        let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        serde_json::from_slice(&bytes[20..20 + len]).unwrap()
    }
    let b = GltfBuilder::new();
    assert!(json(&b).get("animations").is_none(), "no top-level empty animations array");

    let mut b = GltfBuilder::new();
    b.add_animation(Some("Empty".into()));
    let j = json(&b);
    let anim = &j["animations"][0];
    assert!(anim.get("channels").is_none() && anim.get("samplers").is_none(), "{anim}");
}

#[test]
fn path_and_interpolation_display_as_gltf_strings_and_are_copy_eq() {
    assert_eq!(AnimationPath::Translation.to_string(), "translation");
    assert_eq!(AnimationPath::Weights.to_string(), "weights");
    assert_eq!(InterpolationType::CubicSpline.to_string(), "CUBICSPLINE");
    let p = AnimationPath::Rotation;
    let q = p; // Copy
    assert_eq!(p, q);
    assert_ne!(InterpolationType::Linear, InterpolationType::Step);
}
