//! Golden bytes for every accessor-writing path (primitives, animation, skin, morph). The
//! fingerprints were captured BEFORE these writers were ported onto the typed accessor writer
//! and must never change. The JSON is canonicalised (sorted keys) because attribute maps are
//! serialised in hash order.

use mesh_tools::compat::{point3, vector2, vector3};
use mesh_tools::mesh::MeshBuilder;
use mesh_tools::{GltfBuilder, InterpolationType, JointIndices, MorphTarget, SkinSet, Triangle};

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn canon(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(m) => {
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            let items: Vec<String> = keys.iter().map(|k| format!("{:?}:{}", k, canon(&m[*k]))).collect();
            format!("{{{}}}", items.join(","))
        }
        serde_json::Value::Array(a) => format!("[{}]", a.iter().map(canon).collect::<Vec<_>>().join(",")),
        other => other.to_string(),
    }
}

/// (BIN length, BIN hash, canonical JSON length, canonical JSON hash).
fn fingerprint(b: &GltfBuilder) -> (usize, u64, usize, u64) {
    let bytes = b.to_glb_bytes_unchecked().unwrap();
    let jl = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + jl]).unwrap();
    let at = 20 + jl;
    let bl = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let bin = &bytes[at + 8..at + 8 + bl];
    let c = canon(&json);
    (bin.len(), fnv(bin), c.len(), fnv(c.as_bytes()))
}

fn custom_mesh() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let pos = [point3::new(-0.1, 0.2, 0.3), point3::new(1.7, -2.5, 0.0), point3::new(0.3, 0.7, -1.1)];
    let uv = vec![vector2::new(0.1, 0.2), vector2::new(0.3, 0.4), vector2::new(0.5, 0.6)];
    let n = vec![vector3::new(0.0, 0.0, 1.0); 3];
    b.create_custom_mesh(None, &pos, &[Triangle { a: 0, b: 1, c: 2 }], Some(n), Some(vec![uv.clone(), uv]), None);
    b
}

fn mesh_builder_skinned_morphed() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    MeshBuilder::new(Some("m".into()))
        .with_positions(vec![0.1, 0.0, 0.0, 1.0, 0.2, 0.0, 0.0, 1.3, 0.0])
        .with_normals(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0])
        .with_indices(vec![0, 1, 2])
        .with_texcoords(vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6])
        .with_joints(vec![[0, 0, 0, 0], [0, 1, 0, 0], [1, 0, 0, 0]])
        .with_weights(vec![[1.0, 0.0, 0.0, 0.0], [0.5, 0.5, 0.0, 0.0], [0.75, 0.25, 0.0, 0.0]])
        .with_morph_targets(vec![
            MorphTarget::new().with_positions(vec![0.0, 0.1, 1.0, 0.0, -0.3, 1.0, 0.7, 0.0, 1.0]),
            MorphTarget::new().with_normals(vec![0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]),
        ])
        .with_default_weights(vec![0.5, 0.0])
        .build_into(&mut b)
        .unwrap();
    b
}

fn skin_and_ibm() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let j0 = b.add_joint_node(None, Some([0.0, 0.1, 0.0]), None, None);
    let j1 = b.add_joint_node(None, Some([0.0, 1.3, 0.0]), None, None);
    b.add_child_to_node(j0, j1).unwrap();
    b.add_skin_from_joints(None, vec![j0, j1], Some(j0)).unwrap();
    let set = SkinSet::new(JointIndices::U16(vec![[0; 4], [1, 0, 0, 0], [0; 4]]), vec![[1.0, 0.0, 0.0, 0.0]; 3]);
    let p = b.create_skinned_primitive(&[0.0, 0.1, 0.0, 1.0, 0.0, 0.2, 0.0, 1.0, 0.3], Some(&[0, 1, 2]), None, None, &[set], 2, None).unwrap();
    b.add_mesh(None, vec![p]);
    b
}

fn animations() -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let n = b.add_node(None, None, None, None, None);
    let a = b.add_animation(Some("A".into()));
    let t = [0.0, 0.1, 0.7];
    b.add_translation_track(a, n, &t, &[[0.0, 0.1, 0.0], [1.0, 2.0, 3.0], [4.0, 5.0, 6.5]], InterpolationType::Linear).unwrap();
    b.add_rotation_track(a, n, &t, &[[0.0, 0.0, 0.0, 1.0], [0.0, 0.7071068, 0.0, 0.7071068], [0.0, 1.0, 0.0, 0.0]], InterpolationType::Linear).unwrap();
    b.add_scale_track(a, n, &t, &[[1.0; 3], [1.5; 3], [2.0; 3]], InterpolationType::Step).unwrap();
    b
}

#[test]
fn golden_accessor_writers_are_byte_stable() {
    let cases: [(&str, GltfBuilder, (usize, u64, usize, u64)); 4] = [
        ("custom_mesh", custom_mesh(), GOLDEN_CUSTOM),
        ("mesh_builder_skinned_morphed", mesh_builder_skinned_morphed(), GOLDEN_MESH_BUILDER),
        ("skin_and_ibm", skin_and_ibm(), GOLDEN_SKIN),
        ("animations", animations(), GOLDEN_ANIM),
    ];
    for (name, b, want) in cases {
        let got = fingerprint(&b);
        assert_eq!(got, want, "{name}: output changed (got {got:?})");
    }
}

const GOLDEN_CUSTOM: (usize, u64, usize, u64) = (132, 14831615753000862951, 968, 6351083261128159990);
const GOLDEN_MESH_BUILDER: (usize, u64, usize, u64) = (248, 2799355290846834606, 1452, 789599443252778313);
const GOLDEN_SKIN: (usize, u64, usize, u64) = (244, 6420289989394258907, 1069, 5957563893113964507);
const GOLDEN_ANIM: (usize, u64, usize, u64) = (156, 12553229988328004504, 1255, 15798021364752441107);
