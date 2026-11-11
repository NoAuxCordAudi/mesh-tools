mod common;

use mesh_tools::{CubicKeyframe, GltfBuilder, GltfError, InterpolationType, JointIndices, MorphTarget, SkinSet};

const POS: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

fn morph_node(b: &mut GltfBuilder, m: usize) -> usize {
    let set = SkinSet { joints: JointIndices::U8(vec![[0; 4]; 3]), weights: vec![[1.0, 0.0, 0.0, 0.0]; 3] };
    let mut p = b.create_skinned_primitive(&POS, Some(&[0, 1, 2]), None, None, &[set], 1, None).unwrap();
    let targets: Vec<MorphTarget> = (0..m).map(|_| MorphTarget::new().with_positions(vec![0.0; 9])).collect();
    b.attach_morph_targets(&mut p, &targets).unwrap();
    let mesh = b.add_morphable_mesh(None, vec![p], None, None).unwrap();
    b.add_node(None, Some(mesh), None, None, None)
}

struct Doc {
    b: GltfBuilder,
    plain: usize,   // node with TRS tracks
    morph2: usize,  // node whose mesh has 2 targets (LINEAR weights, N=3)
    morph3: usize,  // node whose mesh has 3 targets (CUBICSPLINE weights, N=2)
}

/// Channels in order: translation, rotation, scale (plain node), weights linear (morph2),
/// weights cubic (morph3).
fn doc() -> Doc {
    let mut b = GltfBuilder::new();
    let plain = b.add_node(None, None, None, None, None);
    let morph2 = morph_node(&mut b, 2);
    let morph3 = morph_node(&mut b, 3);
    b.add_scene(None, Some(vec![plain, morph2, morph3]));
    let a = b.add_animation(Some("A".into()));
    let t = [0.0, 1.0, 2.0];
    let lin = InterpolationType::Linear;
    b.add_translation_track(a, plain, &t, &[[0.0; 3]; 3], lin).unwrap();
    b.add_rotation_track(a, plain, &t, &[[0.0, 0.0, 0.0, 1.0]; 3], lin).unwrap();
    b.add_scale_track(a, plain, &t, &[[1.0; 3]; 3], lin).unwrap();
    b.add_weights_track(a, morph2, &t, &[vec![0.0; 2], vec![0.5; 2], vec![1.0; 2]], lin).unwrap();
    let kf = |v: f32| CubicKeyframe { in_tangent: vec![0.0; 3], value: vec![v; 3], out_tangent: vec![0.0; 3] };
    b.add_cubic_weights_track(a, morph3, &[0.0, 1.0], &[kf(0.0), kf(1.0)]).unwrap();
    Doc { b, plain, morph2, morph3 }
}

const TRANSLATION: usize = 0;
const ROTATION: usize = 1;
const WEIGHTS_LINEAR: usize = 3;
const WEIGHTS_CUBIC: usize = 4;

impl Doc {
    fn channel(&mut self, i: usize) -> &mut mesh_tools::AnimationChannel {
        &mut self.b.gltf.animations.as_mut().unwrap()[0].channels.as_mut().unwrap()[i]
    }
    fn sampler_of(&mut self, channel: usize) -> &mut mesh_tools::AnimationSampler {
        let s = self.channel(channel).sampler;
        &mut self.b.gltf.animations.as_mut().unwrap()[0].samplers.as_mut().unwrap()[s]
    }
    fn output_acc(&mut self, channel: usize) -> &mut mesh_tools::Accessor {
        let o = self.sampler_of(channel).output;
        &mut self.b.gltf.accessors.as_mut().unwrap()[o]
    }
    fn input_acc(&mut self, channel: usize) -> &mut mesh_tools::Accessor {
        let i = self.sampler_of(channel).input;
        &mut self.b.gltf.accessors.as_mut().unwrap()[i]
    }
}

#[track_caller]
fn assert_error(code: &str, edit: impl FnOnce(&mut Doc)) -> Vec<String> {
    let mut d = doc();
    edit(&mut d);
    let r = d.b.validate();
    assert!(r.has_error(code), "expected {code}; errors: {:#?}", r.errors);
    r.errors
}

#[test]
fn the_baseline_document_is_valid_with_no_warnings_and_loads() {
    let d = doc();
    let r = d.b.validate();
    assert!(r.errors.is_empty() && r.warnings.is_empty(), "{r:#?}");
    common::parse_glb(&d.b);
}

#[test]
fn builder_and_validator_agree_on_every_track_shape() {
    // The builder and validator share one expected-count helper; this walks the whole grid.
    for m in 1..=3 {
        for n in 1..=4 {
            for interp in [InterpolationType::Linear, InterpolationType::Step, InterpolationType::CubicSpline] {
                let cubic = interp == InterpolationType::CubicSpline;
                if cubic && n < 2 {
                    continue;
                }
                let mut b = GltfBuilder::new();
                let node = morph_node(&mut b, m);
                b.add_scene(None, Some(vec![node]));
                let a = b.add_animation(None);
                let times: Vec<f32> = (0..n).map(|i| i as f32).collect();
                if cubic {
                    let kfs: Vec<_> = (0..n).map(|_| CubicKeyframe { in_tangent: vec![0.0; m], value: vec![0.5; m], out_tangent: vec![0.0; m] }).collect();
                    b.add_cubic_weights_track(a, node, &times, &kfs).unwrap();
                } else {
                    b.add_weights_track(a, node, &times, &vec![vec![0.5; m]; n], interp).unwrap();
                }
                let r = b.validate();
                assert!(r.errors.is_empty(), "m={m} n={n} {interp}: {:#?}", r.errors);
            }
        }
    }
}

#[test]
fn weights_channels_need_a_node_with_morph_targets() {
    assert_error("ANIMATION_CHANNEL_WEIGHTS_NO_MORPHS", |d| {
        let plain = d.plain;
        d.channel(WEIGHTS_LINEAR).target.node = plain; // a node without a mesh
    });
    assert_error("ANIMATION_CHANNEL_WEIGHTS_NO_MORPHS", |d| {
        let node = d.morph2;
        let mesh = d.b.gltf.nodes.as_ref().unwrap()[node].mesh.unwrap();
        d.b.gltf.meshes.as_mut().unwrap()[mesh].primitives[0].targets = None;
    });
    // A mesh without targets, reached through a mesh-bearing node.
    assert_error("ANIMATION_CHANNEL_WEIGHTS_NO_MORPHS", |d| {
        let plain_mesh = d.b.create_box(1.0);
        let n = d.b.add_node(None, Some(plain_mesh), None, None, None);
        d.channel(WEIGHTS_LINEAR).target.node = n;
    });
}

#[test]
fn output_type_must_match_the_path() {
    assert_error("ANIMATION_OUTPUT_TYPE", |d| d.channel(ROTATION).target.path = "translation".into()); // VEC4 under translation
    assert_error("ANIMATION_OUTPUT_TYPE", |d| d.channel(TRANSLATION).target.path = "weights".into());
    assert_error("ANIMATION_OUTPUT_TYPE", |d| d.output_acc(TRANSLATION).component_type = 5123);
    assert_error("ANIMATION_OUTPUT_TYPE", |d| {
        // Normalized integers are not allowed for translation.
        let a = d.output_acc(TRANSLATION);
        a.component_type = 5123;
        a.normalized = Some(true);
    });
    assert_error("ANIMATION_OUTPUT_TYPE", |d| d.output_acc(ROTATION).component_type = 5123); // not flagged normalized
}

#[test]
fn rotation_and_weights_may_use_normalized_integer_outputs() {
    let mut d = doc();
    for ch in [ROTATION, WEIGHTS_LINEAR] {
        let a = d.output_acc(ch);
        a.component_type = 5123;
        a.normalized = Some(true);
    }
    let r = d.b.validate();
    assert!(!r.has_error("ANIMATION_OUTPUT_TYPE"), "{:#?}", r.errors);
}

#[test]
fn output_count_is_checked_per_path_and_interpolation() {
    for (name, ch) in [("translation", TRANSLATION), ("rotation", ROTATION), ("weights", WEIGHTS_LINEAR), ("cubic weights", WEIGHTS_CUBIC)] {
        let errs = assert_error("ANIMATION_OUTPUT_COUNT", |d| d.output_acc(ch).count += 1);
        assert!(errs.iter().any(|e| e.contains("ANIMATION_OUTPUT_COUNT") && e.contains(name.split(' ').next_back().unwrap())), "{name}: {errs:#?}");
        assert_error("ANIMATION_OUTPUT_COUNT", |d| d.output_acc(ch).count -= 1);
    }
    // A LINEAR sampler over cubic-sized data (3*N*M) is wrong; so is a cubic sampler over N*M data.
    assert_error("ANIMATION_OUTPUT_COUNT", |d| d.sampler_of(WEIGHTS_CUBIC).interpolation = Some("LINEAR".into()));
    assert_error("ANIMATION_OUTPUT_COUNT", |d| d.sampler_of(TRANSLATION).interpolation = Some("CUBICSPLINE".into()));
    // Cubic spline needs two keyframes.
    let errs = assert_error("ANIMATION_OUTPUT_COUNT", |d| d.input_acc(WEIGHTS_CUBIC).count = 1);
    assert!(errs.iter().any(|e| e.contains("at least two")), "{errs:#?}");
}

#[test]
fn a_shared_sampler_is_checked_once_per_channel() {
    let mut d = doc();
    // Make morph3 (3 targets) reuse the sampler built for morph2 (2 targets, 6 values): wrong for morph3 only.
    let sampler = d.channel(WEIGHTS_LINEAR).sampler;
    let (m2, m3) = (d.morph2, d.morph3);
    d.b.gltf.animations.as_mut().unwrap()[0].channels.as_mut().unwrap().push(mesh_tools::AnimationChannel {
        sampler,
        target: mesh_tools::AnimationChannelTarget { node: m3, path: "weights".into(), ..Default::default() },
        ..Default::default()
    });
    let r = d.b.validate();
    let count_errors: Vec<&String> = r.errors.iter().filter(|e| e.starts_with("ANIMATION_OUTPUT_COUNT")).collect();
    assert_eq!(count_errors.len(), 1, "{:#?}", r.errors);
    assert!(count_errors[0].contains(&format!("node {m3}")) && !count_errors[0].contains(&format!("node {m2})")), "{}", count_errors[0]);
}

#[test]
fn sampler_input_and_interpolation_are_checked() {
    assert_error("ANIMATION_INTERPOLATION_INVALID", |d| d.sampler_of(TRANSLATION).interpolation = Some("SMOOTH".into()));
    assert_error("ANIMATION_INPUT_TYPE", |d| d.input_acc(TRANSLATION).type_ = "VEC3".into());
    assert_error("ANIMATION_INPUT_TYPE", |d| d.input_acc(TRANSLATION).component_type = 5125);
}

#[test]
fn a_matrix_node_is_valid_for_weights_and_invalid_for_trs() {
    let mut d = doc();
    let identity = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let m2 = d.morph2;
    d.b.gltf.nodes.as_mut().unwrap()[m2].matrix = Some(identity);
    assert!(d.b.validate().is_valid(), "weights on a matrix node: {:#?}", d.b.validate().errors);
    let plain = d.plain;
    d.b.gltf.nodes.as_mut().unwrap()[plain].matrix = Some(identity);
    assert!(d.b.validate().has_error("ANIMATION_CHANNEL_NODE_MATRIX"));
}

#[test]
fn export_refuses_inconsistent_animation_output() {
    let mut d = doc();
    d.output_acc(WEIGHTS_LINEAR).count += 2;
    let Err(GltfError::InvalidData(msg)) = d.b.to_glb_bytes() else { panic!("expected InvalidData") };
    assert!(msg.contains("ANIMATION_OUTPUT_COUNT") && msg.contains("2 morph targets"), "{msg}");
}
