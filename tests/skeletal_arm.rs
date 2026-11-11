//! End-to-end skinning check: build a small arm, then deform it on the CPU using only data
//! read back through the independent `gltf` crate, and compare against hand-derived positions.
//!
//! The deformation uses the glTF skinning formula with an identity skinned-node transform:
//! `v' = sum_k w_k * (jointWorld[j_k] * inverseBind[j_k]) * v`.

mod common;

use gltf::animation::util::ReadOutputs;
use gltf::mesh::util::ReadJoints;
use gltf::scene::Transform;
use mesh_tools::{GltfBuilder, InterpolationType};

type Mat = [[f32; 4]; 4]; // column-major: m[column][row]

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut out = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            out[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

fn apply(m: &Mat, p: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0; 3];
    for r in 0..3 {
        out[r] = m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r];
    }
    out
}

fn close(a: [f32; 3], b: [f32; 3]) {
    for i in 0..3 {
        assert!((a[i] - b[i]).abs() < 1e-4, "{a:?} != {b:?}");
    }
}

/// Elbow bend angle used by the fixture (radians).
const THETA_DEG: f32 = 70.0;

/// Arm along +X with joints at x=0, 2, 4 (nodes parented in a chain) and an elbow rotation
/// animation at t=1 of `THETA_DEG` about Z. Test vertices (position, [(slot, weight)]):
const VERTS: [([f32; 3], [(u16, f32); 2]); 5] = [
    ([0.5, 0.1, 0.0], [(0, 1.0), (0, 0.0)]),  // shoulder only
    ([3.0, 0.0, 0.0], [(1, 1.0), (0, 0.0)]),  // elbow only
    ([2.0, 1.0, 0.0], [(0, 0.5), (1, 0.5)]),  // shoulder/elbow blend
    ([5.0, 0.0, 0.0], [(2, 1.0), (0, 0.0)]),  // wrist only
    ([4.0, 0.5, 0.0], [(1, 0.25), (2, 0.75)]), // elbow/wrist blend
];

/// Build the arm. With `rotated`, the root joint is translated, rotated 90 degrees about Z and
/// the chain's second joint carries a non-uniform scale, so bind-pose world matrices are not
/// pure translations (exercises matrix order and rotation handling in the IBM computation).
///
/// A dummy node is created first so joint node indices (1,2,3) differ from skin slots (0,1,2).
fn fixture(rotated: bool) -> GltfBuilder {
    let mut b = GltfBuilder::new();
    b.add_node(Some("Dummy".into()), None, None, None, None);
    let joints: Vec<usize> = if rotated {
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let a = b.add_joint_node(Some("Shoulder".into()), Some([1.0, 0.5, 0.0]), Some([0.0, 0.0, s, s]), None);
        let c = b.add_joint_node(Some("Elbow".into()), Some([2.0, 0.0, 0.0]), None, Some([1.0, 2.0, 1.0]));
        let d = b.add_joint_node(Some("Wrist".into()), Some([2.0, 0.0, 0.0]), None, None);
        b.add_child_to_node(a, c).unwrap();
        b.add_child_to_node(c, d).unwrap();
        vec![a, c, d]
    } else {
        let names: Vec<String> = ["Shoulder", "Elbow", "Wrist"].iter().map(|s| s.to_string()).collect();
        b.create_bone_chain(&names, &[[0.0; 3], [2.0, 0.0, 0.0], [2.0, 0.0, 0.0]]).unwrap()
    };
    assert_eq!(joints, vec![1, 2, 3], "joint nodes must not coincide with skin slots 0..3");
    let skin = b.add_skin_from_joints(None, joints.clone(), Some(joints[0])).unwrap();

    // Three vertices per triangle keeps the mesh valid; our 5 test vertices plus one filler.
    let mut positions: Vec<f32> = VERTS.iter().flat_map(|(p, _)| *p).collect();
    positions.extend_from_slice(&[0.0, 0.0, 0.0]);
    let mut joint_data: Vec<[u16; 4]> = VERTS.iter().map(|(_, j)| [j[0].0, j[1].0, 0, 0]).collect();
    let mut weight_data: Vec<[f32; 4]> = VERTS.iter().map(|(_, j)| [j[0].1, j[1].1, 0.0, 0.0]).collect();
    joint_data.push([0; 4]);
    weight_data.push([1.0, 0.0, 0.0, 0.0]);
    let mesh = b
        .create_skinned_mesh(None, &positions, &[0, 1, 2, 3, 4, 5], None, None, &joint_data, &weight_data, None)
        .unwrap();
    let node = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.add_scene(None, Some(vec![node, joints[0]]));

    let half = THETA_DEG.to_radians() / 2.0;
    let anim = b.add_animation(None);
    b.add_rotation_track(
        anim,
        joints[1],
        &[0.0, 1.0, 2.0],
        &[[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, half.sin(), half.cos()], [0.0, 0.0, 0.0, 1.0]],
        InterpolationType::Linear,
    )
    .unwrap();
    b
}

/// Skin every vertex using a pose where `rotation_override` replaces the elbow's local rotation.
fn skinned_positions(glb: &gltf::Gltf, elbow_rotation: Option<[f32; 4]>) -> Vec<[f32; 3]> {
    let doc = &glb.document;
    let skin = doc.skins().next().unwrap();
    let reader = skin.reader(|_| glb.blob.as_deref());
    let ibm: Vec<Mat> = reader.read_inverse_bind_matrices().unwrap().collect();
    let joints: Vec<gltf::Node> = skin.joints().collect();

    // World matrix of each joint from the document's own hierarchy (chain: parent = previous joint).
    let mut world: Vec<Mat> = Vec::new();
    for (i, j) in joints.iter().enumerate() {
        let local = match (i, elbow_rotation) {
            (1, Some(rotation)) => {
                let (translation, _, scale) = j.transform().decomposed();
                Transform::Decomposed { translation, rotation, scale }.matrix()
            }
            _ => j.transform().matrix(),
        };
        world.push(if i == 0 { local } else { mul(&world[i - 1], &local) });
    }
    let skin_mats: Vec<Mat> = (0..joints.len()).map(|k| mul(&world[k], &ibm[k])).collect();

    let prim = doc.meshes().next().unwrap().primitives().next().unwrap();
    let r = prim.reader(|_| glb.blob.as_deref());
    let positions: Vec<[f32; 3]> = r.read_positions().unwrap().collect();
    let ReadJoints::U16(js) = r.read_joints(0).unwrap() else { panic!("expected U16 joints") };
    let js: Vec<[u16; 4]> = js.collect();
    let ws: Vec<[f32; 4]> = r.read_weights(0).unwrap().into_f32().collect();

    positions
        .iter()
        .enumerate()
        .map(|(v, p)| {
            let mut out = [0.0; 3];
            for k in 0..4 {
                if ws[v][k] > 0.0 {
                    let q = apply(&skin_mats[js[v][k] as usize], *p);
                    for i in 0..3 {
                        out[i] += ws[v][k] * q[i];
                    }
                }
            }
            out
        })
        .collect()
}

#[test]
fn bind_pose_reproduces_the_original_mesh() {
    let glb = common::parse_glb(&fixture(false));
    let skinned = skinned_positions(&glb, None);
    for (v, (p, _)) in VERTS.iter().enumerate() {
        close(skinned[v], *p);
    }
}

#[test]
fn elbow_bend_matches_hand_derived_positions() {
    let b = fixture(false);
    let glb = common::parse_glb(&b);

    // Read the t=1 rotation keyframe back from the file instead of trusting the fixture constants.
    let chan = glb.document.animations().next().unwrap().channels().next().unwrap();
    let reader = chan.reader(|_| glb.blob.as_deref());
    assert_eq!(reader.read_inputs().unwrap().nth(1), Some(1.0));
    let ReadOutputs::Rotations(rots) = reader.read_outputs().unwrap() else { panic!("rotations") };
    let q1 = rots.into_f32().nth(1).unwrap();

    let skinned = skinned_positions(&glb, Some(q1));
    let (s, c) = THETA_DEG.to_radians().sin_cos();

    // Elbow pivot at (2,0,0). Rz(theta) maps (x,y) -> (c*x - s*y, s*x + c*y).
    let rot_about_elbow = |p: [f32; 3]| {
        let (dx, dy) = (p[0] - 2.0, p[1]);
        [2.0 + c * dx - s * dy, s * dx + c * dy, p[2]]
    };

    // Shoulder-only vertex does not move.
    close(skinned[0], VERTS[0].0);
    // Elbow-only vertex (3,0,0) swings to (2 + cos, sin, 0).
    close(skinned[1], [2.0 + c, s, 0.0]);
    close(skinned[1], rot_about_elbow(VERTS[1].0));
    // 50/50 shoulder/elbow vertex is the average of its static and rotated positions.
    let rotated = rot_about_elbow(VERTS[2].0);
    close(skinned[2], [(VERTS[2].0[0] + rotated[0]) / 2.0, (VERTS[2].0[1] + rotated[1]) / 2.0, 0.0]);
    // Wrist-only vertex rides the whole forearm: rotates about the elbow as a rigid body.
    close(skinned[3], [2.0 + 3.0 * c, 3.0 * s, 0.0]);
    // Elbow/wrist blend: both joints carry it with the same rotation (wrist is at bind relative
    // to the elbow), so it also rotates rigidly about the elbow.
    close(skinned[4], rot_about_elbow(VERTS[4].0));
}

#[test]
fn bind_pose_is_reproduced_with_rotated_and_scaled_joints() {
    // Every IBM * world must be the identity even when joints carry rotation and non-uniform
    // scale; this fails if the library composes matrices in the wrong order or mishandles TRS.
    let glb = common::parse_glb(&fixture(true));
    let skinned = skinned_positions(&glb, None);
    for (v, (p, _)) in VERTS.iter().enumerate() {
        close(skinned[v], *p);
    }
}

#[test]
fn inverse_bind_matrices_match_hand_derived_values_for_a_rotated_bind_pose() {
    // Shoulder: T(1,0.5,0) * Rz(90). World origin (1,0.5,0). Local +X maps to world +Y.
    // IBM = inverse(world): translation = -R^T t = -( 0.5, -1, 0 ) = (-0.5, 1, 0)
    // (Rz(90)^T maps (x,y) -> (y,-x): (1,0.5) -> (0.5,-1)).
    let glb = common::parse_glb(&fixture(true));
    let skin = glb.document.skins().next().unwrap();
    let ibm: Vec<Mat> = skin.reader(|_| glb.blob.as_deref()).read_inverse_bind_matrices().unwrap().collect();

    close(apply(&ibm[0], [1.0, 0.5, 0.0]), [0.0, 0.0, 0.0]); // joint origin -> local origin
    close(apply(&ibm[0], [1.0, 1.5, 0.0]), [1.0, 0.0, 0.0]); // one unit along world +Y -> local +X
    // Elbow world = Shoulder * T(2,0,0) * S(1,2,1): origin at (1,0.5,0) + Rz(90)*(2,0,0) = (1,2.5,0).
    close(apply(&ibm[1], [1.0, 2.5, 0.0]), [0.0, 0.0, 0.0]);
    // Elbow local +Y (scaled by 2) is world -X: world (1-2, 2.5, 0) = (-1,2.5,0) is local (0,1,0).
    close(apply(&ibm[1], [-1.0, 2.5, 0.0]), [0.0, 1.0, 0.0]);
}

#[test]
fn document_is_valid_and_joint_slots_are_not_node_indices() {
    let b = fixture(false);
    assert!(b.validate().is_valid(), "{:#?}", b.validate().errors);
    let glb = common::parse_glb(&b);
    let skin = glb.document.skins().next().unwrap();
    let joint_nodes: Vec<usize> = skin.joints().map(|n| n.index()).collect();
    assert_eq!(joint_nodes, vec![1, 2, 3], "joints are nodes 1..=3, so slots 0..3 differ from node indices");

    let prim = glb.document.meshes().next().unwrap().primitives().next().unwrap();
    let ReadJoints::U16(js) = prim.reader(|_| glb.blob.as_deref()).read_joints(0).unwrap() else { panic!() };
    let js: Vec<[u16; 4]> = js.collect();
    assert!(js.iter().flatten().all(|&slot| slot < 3), "slots must index the 3-joint skin");
    // The elbow-only vertex (index 1) must reference slot 1, which maps to node 2 -- not node 1.
    assert_eq!(js[1][0], 1);
    assert_eq!(joint_nodes[js[1][0] as usize], 2);
}
