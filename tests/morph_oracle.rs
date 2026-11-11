//! Independent oracle tests for morph targets.
//!
//! The library builds documents; everything below reads them back through the `gltf` crate and
//! evaluates morphing, skinning and animation sampling with code defined *in this file*. Expected
//! values are literals derived by hand (see the comments), never produced by library helpers.

mod common;

use gltf::animation::util::ReadOutputs;
use gltf::scene::Transform;
use mesh_tools::{CubicKeyframe, GltfBuilder, InterpolationType, JointIndices, MorphTarget, SkinSet};

type V3 = [f32; 3];
const EPS: f32 = 1e-6;

// Base triangle and two targets (position deltas), plus normal deltas.
const BASE: [V3; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
const T0: [V3; 3] = [[0.0, 0.0, 1.0]; 3];
const T1: [V3; 3] = [[1.0, 0.0, 0.0], [0.0, 2.0, 0.0], [-1.0, -1.0, 0.0]];
const BASE_NORMAL: V3 = [0.0, 0.0, 1.0];
// Deliberately not unit length, so any normalization by the library would be visible.
const N0: V3 = [0.0, 2.0, 0.0];
const N1: V3 = [0.5, 0.0, 0.0];

fn flat(v: &[V3]) -> Vec<f32> {
    v.iter().flatten().copied().collect()
}

/// One joint, a skinned node, and a mesh with the two targets (and normals). `mesh_weights` and
/// `node_weights` are optional defaults/overrides. Returns (builder, node index).
fn fixture(mesh_weights: Option<Vec<f32>>, node_weights: Option<Vec<f32>>) -> (GltfBuilder, usize) {
    let mut b = GltfBuilder::new();
    let joint = b.add_joint_node(Some("J".into()), None, None, None);
    let skin = b.add_skin_from_joints(None, vec![joint], Some(joint)).unwrap();
    let set = SkinSet::new(JointIndices::U8(vec![[0; 4]; 3]), vec![[1.0, 0.0, 0.0, 0.0]; 3]);
    let mut prim = b
        .create_skinned_primitive(&flat(&BASE), Some(&[0, 1, 2]), Some(&flat(&[BASE_NORMAL; 3])), None, &[set], 1, None)
        .unwrap();
    b.attach_morph_targets(
        &mut prim,
        &[
            MorphTarget::new().with_positions(flat(&T0)).with_normals(flat(&[N0; 3])),
            MorphTarget::new().with_positions(flat(&T1)).with_normals(flat(&[N1; 3])),
        ],
    )
    .unwrap();
    let mesh = b.add_morphable_mesh(None, vec![prim], mesh_weights, None).unwrap();
    let node = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    if let Some(w) = node_weights {
        b.set_node_weights(node, w).unwrap();
    }
    b.add_scene(None, Some(vec![node, joint]));
    (b, node)
}

// ------------------------------------------------------------------ CPU morphing

struct Morphable {
    base: Vec<V3>,
    base_normals: Vec<V3>,
    pos_deltas: Vec<Vec<V3>>,
    nrm_deltas: Vec<Vec<V3>>,
    /// Effective weights: node weights, else mesh weights, else zeros.
    weights: Vec<f32>,
}

fn read(glb: &gltf::Gltf, node_index: usize) -> Morphable {
    let node = glb.document.nodes().nth(node_index).unwrap();
    let mesh = node.mesh().unwrap();
    let prim = mesh.primitives().next().unwrap();
    let reader = prim.reader(|_| glb.blob.as_deref());
    let mut m = Morphable {
        base: reader.read_positions().unwrap().collect(),
        base_normals: reader.read_normals().unwrap().collect(),
        pos_deltas: vec![],
        nrm_deltas: vec![],
        weights: vec![],
    };
    for (pos, nrm, _tan) in reader.read_morph_targets() {
        m.pos_deltas.push(pos.unwrap().collect());
        m.nrm_deltas.push(nrm.unwrap().collect());
    }
    let count = m.pos_deltas.len();
    m.weights = node.weights().or_else(|| mesh.weights()).map(|w| w.to_vec()).unwrap_or_else(|| vec![0.0; count]);
    m
}

/// glTF morphing: `result = base + sum(weight_i * delta_i)`. No clamping, no renormalization.
fn morphed(m: &Morphable, weights: &[f32]) -> Vec<V3> {
    (0..m.base.len())
        .map(|v| {
            let mut p = m.base[v];
            for (t, w) in weights.iter().enumerate() {
                for c in 0..3 {
                    p[c] += w * m.pos_deltas[t][v][c];
                }
            }
            p
        })
        .collect()
}

fn close(a: V3, b: V3) {
    for c in 0..3 {
        assert!((a[c] - b[c]).abs() < EPS, "{a:?} != {b:?}");
    }
}

fn assert_all_close(got: &[V3], want: &[V3]) {
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(want) {
        close(*g, *w);
    }
}

#[test]
fn morphing_matches_hand_derived_positions_for_each_weight_vector() {
    // base + w0*T0 + w1*T1, derived by hand per vertex. No clamping: [-1, 2] is legal.
    let cases: [([f32; 2], [V3; 3]); 5] = [
        ([0.0, 0.0], [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
        ([1.0, 0.0], [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]]),
        ([0.0, 1.0], [[1.0, 0.0, 0.0], [1.0, 2.0, 0.0], [-1.0, 0.0, 0.0]]),
        ([0.5, 0.25], [[0.25, 0.0, 0.5], [1.0, 0.5, 0.5], [-0.25, 0.75, 0.5]]),
        ([-1.0, 2.0], [[2.0, 0.0, -1.0], [1.0, 4.0, -1.0], [-2.0, -1.0, -1.0]]),
    ];
    for (w, expected) in cases {
        let (b, node) = fixture(None, Some(w.to_vec()));
        let glb = common::parse_glb(&b);
        let m = read(&glb, node);
        assert_eq!(m.weights, w.to_vec(), "effective weights are the node's");
        assert_all_close(&morphed(&m, &m.weights), &expected);
    }
}

#[test]
fn deltas_are_stored_as_given_and_normals_are_not_renormalized() {
    let (b, node) = fixture(None, None);
    let glb = common::parse_glb(&b);
    let m = read(&glb, node);
    assert_eq!(m.pos_deltas[0], T0.to_vec());
    assert_eq!(m.pos_deltas[1], T1.to_vec());
    assert_eq!(m.nrm_deltas[0], vec![N0; 3]);
    assert_eq!(m.base_normals, vec![BASE_NORMAL; 3]);
    // The morphed normal base + 0.5*N0 + 0.25*N1 = (0.125, 1.0, 1.0) has length != 1: renderers
    // renormalize, the library must not. (x: 0.25*0.5, y: 0.5*2, z: base 1.)
    let w = [0.5f32, 0.25];
    let n: V3 = std::array::from_fn(|c| m.base_normals[0][c] + w[0] * m.nrm_deltas[0][0][c] + w[1] * m.nrm_deltas[1][0][c]);
    close(n, [0.125, 1.0, 1.0]);
}

#[test]
fn node_weights_override_mesh_weights_which_override_zeros() {
    let (b, node) = fixture(Some(vec![0.5, 0.0]), Some(vec![0.0, 1.0]));
    let m = read(&common::parse_glb(&b), node);
    assert_eq!(m.weights, vec![0.0, 1.0], "node weights win");

    let (b, node) = fixture(Some(vec![0.5, 0.0]), None);
    let m = read(&common::parse_glb(&b), node);
    assert_eq!(m.weights, vec![0.5, 0.0], "mesh weights are the default");
    // 0.5 * T0 only: (0,0,0)+(0,0,0.5), (1,0,0)+(0,0,0.5), (0,1,0)+(0,0,0.5)
    assert_all_close(&morphed(&m, &m.weights), &[[0.0, 0.0, 0.5], [1.0, 0.0, 0.5], [0.0, 1.0, 0.5]]);

    let (b, node) = fixture(None, None);
    let m = read(&common::parse_glb(&b), node);
    assert_eq!(m.weights, vec![0.0, 0.0], "no weights anywhere means all zero");
    assert_all_close(&morphed(&m, &m.weights), &BASE);
}

// ------------------------------------------------------------------ CPU skinning

type Mat = [[f32; 4]; 4]; // column-major m[column][row]

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut out = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            out[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

fn apply(m: &Mat, p: V3) -> V3 {
    std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r])
}

/// Skin matrix of the single joint with the given pose rotation (about Z, 90 degrees).
fn skin_matrix(glb: &gltf::Gltf) -> Mat {
    let skin = glb.document.skins().next().unwrap();
    let ibm: Vec<Mat> = skin.reader(|_| glb.blob.as_deref()).read_inverse_bind_matrices().unwrap().collect();
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let world = Transform::Decomposed { translation: [0.0; 3], rotation: [0.0, 0.0, s, s], scale: [1.0; 3] }.matrix();
    mul(&world, &ibm[0])
}

#[test]
fn morph_is_applied_before_skinning() {
    // Pose: joint rotated 90 degrees about Z, (x, y, z) -> (-y, x, z). Weights [0, 1] (T1 only).
    let (b, node) = fixture(None, Some(vec![0.0, 1.0]));
    let glb = common::parse_glb(&b);
    let m = read(&glb, node);
    let sm = skin_matrix(&glb);

    // Correct order: skin(base + morph). Hand-derived: morph = (1,0,0) (1,2,0) (-1,0,0), then rotate.
    let expected: [V3; 3] = [[0.0, 1.0, 0.0], [-2.0, 1.0, 0.0], [0.0, -1.0, 0.0]];
    let correct: Vec<V3> = morphed(&m, &m.weights).iter().map(|p| apply(&sm, *p)).collect();
    assert_all_close(&correct, &expected);

    // Wrong order: skin(base) + morph delta. Hand-derived: (1,0,0) (0,3,0) (-2,-1,0).
    let wrong_order: [V3; 3] = [[1.0, 0.0, 0.0], [0.0, 3.0, 0.0], [-2.0, -1.0, 0.0]];
    let computed_wrong: Vec<V3> = (0..3)
        .map(|v| {
            let s = apply(&sm, m.base[v]);
            std::array::from_fn(|c| s[c] + m.pos_deltas[1][v][c])
        })
        .collect();
    assert_all_close(&computed_wrong, &wrong_order);
    // The guard: the two orders must give different answers, or this test proves nothing.
    assert!(
        (0..3).any(|v| (0..3).any(|c| (expected[v][c] - wrong_order[v][c]).abs() > 0.5)),
        "orders must be distinguishable"
    );
}

// ------------------------------------------------------------------ CPU animation sampling

/// Sample a weights channel: `times` (N), `out` (flat), `m` targets, at time `t`.
fn sample(times: &[f32], out: &[f32], m: usize, interp: gltf::animation::Interpolation, t: f32) -> Vec<f32> {
    use gltf::animation::Interpolation::*;
    let n = times.len();
    if t <= times[0] {
        return value_at(out, m, interp, 0);
    }
    if t >= times[n - 1] {
        return value_at(out, m, interp, n - 1);
    }
    let k = (0..n - 1).find(|&k| t < times[k + 1]).unwrap();
    let dt = times[k + 1] - times[k];
    let u = (t - times[k]) / dt;
    match interp {
        Step => value_at(out, m, interp, k),
        Linear => {
            let (a, b) = (value_at(out, m, interp, k), value_at(out, m, interp, k + 1));
            (0..m).map(|i| a[i] + u * (b[i] - a[i])).collect()
        }
        CubicSpline => {
            // Per keyframe the layout is [in-tangents(m), values(m), out-tangents(m)].
            let at = |kf: usize, part: usize, i: usize| out[kf * 3 * m + part * m + i];
            let (h00, h10, h01, h11) = (2.0 * u.powi(3) - 3.0 * u * u + 1.0, u.powi(3) - 2.0 * u * u + u, -2.0 * u.powi(3) + 3.0 * u * u, u.powi(3) - u * u);
            (0..m)
                .map(|i| h00 * at(k, 1, i) + h10 * dt * at(k, 2, i) + h01 * at(k + 1, 1, i) + h11 * dt * at(k + 1, 0, i))
                .collect()
        }
    }
}

fn value_at(out: &[f32], m: usize, interp: gltf::animation::Interpolation, k: usize) -> Vec<f32> {
    if interp == gltf::animation::Interpolation::CubicSpline {
        out[k * 3 * m + m..k * 3 * m + 2 * m].to_vec()
    } else {
        out[k * m..(k + 1) * m].to_vec()
    }
}

/// Build a weights animation on the fixture's node and read (times, flat output, interpolation) back.
fn animate(times: &[f32], build: impl FnOnce(&mut GltfBuilder, usize, usize)) -> (Vec<f32>, Vec<f32>, gltf::animation::Interpolation) {
    let (mut b, node) = fixture(None, None);
    let anim = b.add_animation(None);
    build(&mut b, anim, node);
    let glb = common::parse_glb(&b);
    let chan = glb.document.animations().next().unwrap().channels().next().unwrap();
    let reader = chan.reader(|_| glb.blob.as_deref());
    let t: Vec<f32> = reader.read_inputs().unwrap().collect();
    assert_eq!(t, times);
    let ReadOutputs::MorphTargetWeights(w) = reader.read_outputs().unwrap() else { panic!("weights output") };
    (t, w.into_f32().collect(), chan.sampler().interpolation())
}

fn assert_weights(got: &[f32], want: &[f32]) {
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < EPS, "{got:?} != {want:?}");
    }
}

#[test]
fn linear_weights_sample_to_hand_derived_values() {
    let times = [0.0, 1.0, 2.0];
    let (t, out, interp) = animate(&times, |b, a, n| {
        b.add_weights_track(a, n, &times, &[vec![0.0, 0.0], vec![1.0, 0.0], vec![0.5, 0.25]], InterpolationType::Linear).unwrap();
    });
    assert_weights(&sample(&t, &out, 2, interp, 0.0), &[0.0, 0.0]);
    assert_weights(&sample(&t, &out, 2, interp, 0.5), &[0.5, 0.0]); // halfway (0,0)->(1,0)
    assert_weights(&sample(&t, &out, 2, interp, 1.5), &[0.75, 0.125]); // halfway (1,0)->(0.5,0.25)
    assert_weights(&sample(&t, &out, 2, interp, 2.0), &[0.5, 0.25]);
    assert_weights(&sample(&t, &out, 2, interp, 9.0), &[0.5, 0.25]); // holds the last keyframe
}

#[test]
fn step_weights_hold_the_previous_keyframe() {
    let times = [0.0, 1.0, 2.0];
    let (t, out, interp) = animate(&times, |b, a, n| {
        b.add_weights_track(a, n, &times, &[vec![0.0, 0.0], vec![1.0, 0.0], vec![0.5, 0.25]], InterpolationType::Step).unwrap();
    });
    assert_weights(&sample(&t, &out, 2, interp, 0.5), &[0.0, 0.0]);
    assert_weights(&sample(&t, &out, 2, interp, 1.0), &[1.0, 0.0]); // at a keyframe: that keyframe
    assert_weights(&sample(&t, &out, 2, interp, 1.99), &[1.0, 0.0]);
}

#[test]
fn cubic_spline_weights_sample_to_hand_derived_hermite_values() {
    // Two keyframes. Target 0: p0=0, outTangent0=2, p1=1, inTangent1=0.  Target 1: p0=0, out0=-2, p1=1, in1=4.
    // Hermite at u=0.5, dt=1: h00=.5 h10=.125 h01=.5 h11=-.125
    //   target 0: .5*0 + .125*2 + .5*1 + -.125*0 = 0.75
    //   target 1: .5*0 + .125*-2 + .5*1 + -.125*4 = -0.25
    // At u=0.25: h00=.84375 h10=.140625 h01=.15625 h11=-.046875 -> target 0: .140625*2 + .15625 = 0.4375
    let kf = |i: [f32; 2], v: [f32; 2], o: [f32; 2]| CubicKeyframe { in_tangent: i.to_vec(), value: v.to_vec(), out_tangent: o.to_vec() };
    let times = [0.0, 1.0];
    let (t, out, interp) = animate(&times, |b, a, n| {
        b.add_cubic_weights_track(a, n, &times, &[kf([0.0, 0.0], [0.0, 0.0], [2.0, -2.0]), kf([0.0, 4.0], [1.0, 1.0], [0.0, 0.0])]).unwrap();
    });
    assert_weights(&sample(&t, &out, 2, interp, 0.0), &[0.0, 0.0]);
    assert_weights(&sample(&t, &out, 2, interp, 0.5), &[0.75, -0.25]);
    assert_weights(&sample(&t, &out, 2, interp, 0.25)[..1], &[0.4375]);
    assert_weights(&sample(&t, &out, 2, interp, 1.0), &[1.0, 1.0]);
}

#[test]
fn cubic_spline_tangents_scale_with_the_keyframe_interval() {
    // Same tangents over dt=2: at the midpoint, .125*dt*out0 = .125*2*2 = 0.5, plus .5*p1 = 0.5 -> 1.0.
    let kf = |v: f32, o: f32| CubicKeyframe { in_tangent: vec![0.0], value: vec![v], out_tangent: vec![o] };
    let times = [0.0, 2.0];
    // The fixture mesh has 2 targets, so use a second single-target mesh for this M=1 case.
    let mut b = GltfBuilder::new();
    let set = SkinSet::new(JointIndices::U8(vec![[0; 4]; 3]), vec![[1.0, 0.0, 0.0, 0.0]; 3]);
    let mut prim = b.create_skinned_primitive(&flat(&BASE), None, None, None, &[set], 1, None).unwrap();
    b.attach_morph_targets(&mut prim, &[MorphTarget::new().with_positions(flat(&T0))]).unwrap();
    let mesh = b.add_morphable_mesh(None, vec![prim], None, None).unwrap();
    let node = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![node]));
    let anim = b.add_animation(None);
    b.add_cubic_weights_track(anim, node, &times, &[kf(0.0, 2.0), kf(1.0, 0.0)]).unwrap();
    let glb = common::parse_glb(&b);
    let chan = glb.document.animations().next().unwrap().channels().next().unwrap();
    let reader = chan.reader(|_| glb.blob.as_deref());
    let ReadOutputs::MorphTargetWeights(w) = reader.read_outputs().unwrap() else { panic!() };
    let out: Vec<f32> = w.into_f32().collect();
    assert_weights(&sample(&times, &out, 1, chan.sampler().interpolation(), 1.0), &[1.0]);
}

#[test]
fn animated_weights_outside_zero_one_are_not_clamped() {
    let times = [0.0, 1.0];
    let (t, out, interp) = animate(&times, |b, a, n| {
        b.add_weights_track(a, n, &times, &[vec![-1.0, 2.0], vec![3.0, -0.5]], InterpolationType::Linear).unwrap();
    });
    assert_eq!(out, vec![-1.0, 2.0, 3.0, -0.5], "stored as given");
    // Midpoint, by hand: (-1 + 3) / 2 = 1.0 and (2 + -0.5) / 2 = 0.75.
    assert_weights(&sample(&t, &out, 2, interp, 0.5), &[1.0, 0.75]);
}

#[test]
fn sparse_and_dense_targets_morph_identically() {
    // T1 as sparse: vertex 0 has (1,0,0), vertex 1 (0,2,0), vertex 2 (-1,-1,0) (all non-zero),
    // T0 only moves vertex 1 by (0,0,1). Dense counterpart has zeros elsewhere.
    let build = |sparse: bool| {
        let mut b = GltfBuilder::new();
        let mut prim = b.create_primitive(mesh_tools::PrimitiveSpec {
            positions: flat(&BASE),
            indices: Some(mesh_tools::Indices::U8(vec![0, 1, 2])),
            ..Default::default()
        }).unwrap();
        let t0 = if sparse {
            MorphTarget::new().with_sparse_positions(vec![1], vec![0.0, 0.0, 1.0])
        } else {
            MorphTarget::new().with_positions(vec![0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0])
        };
        let t1 = if sparse {
            MorphTarget::new().with_sparse_positions(vec![0, 1, 2], flat(&T1))
        } else {
            MorphTarget::new().with_positions(flat(&T1))
        };
        b.attach_morph_targets(&mut prim, &[t0, t1]).unwrap();
        let mesh = b.add_morphable_mesh(None, vec![prim], Some(vec![0.5, 0.25]), None).unwrap();
        let n = b.add_node(None, Some(mesh), None, None, None);
        b.add_scene(None, Some(vec![n]));
        b
    };
    let read_morphed = |b: &GltfBuilder| {
        let glb = common::parse_glb(b);
        let prim = glb.document.meshes().next().unwrap().primitives().next().unwrap();
        let r = prim.reader(|_| glb.blob.as_deref());
        let base: Vec<V3> = r.read_positions().unwrap().collect();
        let deltas: Vec<Vec<V3>> = r.read_morph_targets().map(|(p, _, _)| p.unwrap().collect()).collect();
        let m = Morphable { base, base_normals: vec![], pos_deltas: deltas, nrm_deltas: vec![], weights: vec![] };
        morphed(&m, &[0.5, 0.25])
    };
    // Hand oracle at w = (0.5, 0.25): v0 = (0,0,0)+.25*(1,0,0); v1 = (1,0,0)+.5*(0,0,1)+.25*(0,2,0);
    // v2 = (0,1,0)+.25*(-1,-1,0).
    let want = [[0.25, 0.0, 0.0], [1.0, 0.5, 0.5], [-0.25, 0.75, 0.0]];
    assert_all_close(&read_morphed(&build(true)), &want);
    assert_all_close(&read_morphed(&build(false)), &want);
}
