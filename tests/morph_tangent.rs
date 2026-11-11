//! Tangent deltas of morph targets, read back through the independent `gltf` crate.

mod common;

use mesh_tools::mesh::MeshBuilder;
use mesh_tools::{GltfBuilder, MorphTarget};

const POS: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
const NRM: [f32; 9] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
// Base tangents are VEC4 (xyz + handedness w).
const TAN: [f32; 12] = [1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, -1.0];
// Tangent deltas are VEC3 (no w).
const TD: [f32; 9] = [0.0, 0.5, 0.0, 0.0, -0.5, 0.0, 0.25, 0.0, 0.0];

fn counts(b: &GltfBuilder) -> (usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffers.as_ref().map_or(0, |a| a.first().map_or(0, |b| b.byte_length)),
    )
}

fn builder_with_tangent_target() -> (GltfBuilder, usize) {
    let mut b = GltfBuilder::new();
    let mesh = MeshBuilder::new(None)
        .with_positions(POS.to_vec())
        .with_normals(NRM.to_vec())
        .with_tangents(TAN.to_vec())
        .with_indices(vec![0, 1, 2])
        .with_morph_targets(vec![MorphTarget::new().with_tangents(TD.to_vec())])
        .build_into(&mut b)
        .unwrap();
    (b, mesh)
}

#[test]
fn tangent_deltas_round_trip_and_blend_as_base_plus_weight_times_delta() {
    let (b, _) = builder_with_tangent_target();
    assert!(b.validate().is_valid(), "{:?}", b.validate().errors);
    let glb = common::parse_glb(&b);
    let prim = glb.document.meshes().next().unwrap().primitives().next().unwrap();
    let reader = prim.reader(|_| glb.blob.as_deref());
    let base: Vec<[f32; 4]> = reader.read_tangents().unwrap().collect();
    let (pos, nrm, tan) = reader.read_morph_targets().next().unwrap();
    assert!(pos.is_none() && nrm.is_none());
    let delta: Vec<[f32; 3]> = tan.unwrap().collect();
    assert_eq!(delta, vec![[0.0, 0.5, 0.0], [0.0, -0.5, 0.0], [0.25, 0.0, 0.0]]);
    // Hand-derived at weight 0.5: base.xyz + 0.5 * delta; w is not morphed.
    let w = 0.5;
    let got: Vec<[f32; 3]> = (0..3).map(|v| [0, 1, 2].map(|c| base[v][c] + w * delta[v][c])).collect();
    assert_eq!(got, vec![[1.0, 0.25, 0.0], [1.0, -0.25, 0.0], [0.125, 1.0, 0.0]]);
}

#[test]
fn tangent_accessor_is_float_vec3_array_buffer() {
    let (b, _) = builder_with_tangent_target();
    let prim = &b.gltf.meshes.as_ref().unwrap()[0].primitives[0];
    let acc = &b.gltf.accessors.as_ref().unwrap()[prim.targets.as_ref().unwrap()[0]["TANGENT"]];
    assert_eq!((acc.type_.as_str(), acc.component_type, acc.count), ("VEC3", 5126, 3));
    assert!(acc.min.is_none() && acc.max.is_none(), "only POSITION deltas carry bounds");
}

#[test]
fn tangent_deltas_without_a_base_tangent_are_rejected_atomically() {
    let mut b = GltfBuilder::new();
    let before = counts(&b);
    let r = MeshBuilder::new(None)
        .with_positions(POS.to_vec())
        .with_normals(NRM.to_vec())
        .with_indices(vec![0, 1, 2])
        .with_morph_targets(vec![MorphTarget::new().with_tangents(TD.to_vec())])
        .build_into(&mut b);
    let msg = r.unwrap_err().to_string();
    assert!(msg.contains("tangent deltas but the primitive has no base TANGENT"), "{msg}");
    assert_eq!(counts(&b), before);
    assert!(b.gltf.meshes.as_ref().is_none_or(|m| m.is_empty()));
}

#[test]
fn tangent_delta_length_and_finiteness_are_checked() {
    let mut b = GltfBuilder::new();
    let mut prim = b
        .create_primitive(mesh_tools::PrimitiveSpec {
            positions: POS.to_vec(),
            normals: Some(NRM.to_vec()),
            tangents: Some(TAN.to_vec()),
            ..Default::default()
        })
        .unwrap();
    let e = b.attach_morph_targets(&mut prim, &[MorphTarget::new().with_tangents(vec![0.0; 12])]).unwrap_err();
    assert!(e.to_string().contains("tangent deltas have 12 floats, expected 9"), "{e}");
    let mut bad = TD.to_vec();
    bad[4] = f32::NAN;
    let e = b.attach_morph_targets(&mut prim, &[MorphTarget::new().with_tangents(bad)]).unwrap_err();
    assert!(e.to_string().contains("tangent delta value 4 is not finite"), "{e}");
    assert!(prim.targets.is_none());
}
