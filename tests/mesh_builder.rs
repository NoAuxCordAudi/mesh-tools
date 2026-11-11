mod common;

use gltf::mesh::util::ReadJoints;
use gltf::Semantic;
use mesh_tools::mesh::MeshBuilder;
use mesh_tools::{GltfBuilder, GltfError, Primitive};

const POS: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

fn base() -> MeshBuilder {
    MeshBuilder::new(Some("M".into())).with_positions(POS.to_vec()).with_indices(vec![0, 1, 2])
}

fn first_primitive(glb: &gltf::Gltf) -> gltf::Primitive<'_> {
    glb.document.meshes().next().unwrap().primitives().next().unwrap()
}

fn counts(b: &GltfBuilder) -> (usize, usize, usize) {
    (
        b.gltf.accessors.as_ref().map_or(0, |a| a.len()),
        b.gltf.buffer_views.as_ref().map_or(0, |a| a.len()),
        b.gltf.meshes.as_ref().map_or(0, |a| a.len()),
    )
}

#[test]
fn build_into_uploads_geometry_instead_of_an_empty_primitive() {
    let mut b = GltfBuilder::new();
    let idx = base()
        .with_normals(vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0])
        .build_into(&mut b)
        .unwrap();
    assert_eq!(idx, 0);

    let glb = common::parse_glb(&b);
    let prim = first_primitive(&glb);
    let reader = prim.reader(|_| glb.blob.as_deref());
    assert_eq!(reader.read_positions().unwrap().count(), 3);
    assert_eq!(reader.read_normals().unwrap().count(), 3);
    assert_eq!(reader.read_indices().unwrap().into_u32().collect::<Vec<_>>(), vec![0, 1, 2]);
    assert!(prim.get(&Semantic::Positions).unwrap().min().is_some(), "POSITION has bounds");
}

#[test]
fn build_into_supports_multiple_texcoord_sets_and_material() {
    let mut b = GltfBuilder::new();
    let material = b.create_basic_material(None, [1.0, 0.0, 0.0, 1.0]);
    let uv = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    base().with_multiple_texcoords(vec![uv.clone(), uv]).with_material(material).build_into(&mut b).unwrap();
    let glb = common::parse_glb(&b);
    let prim = first_primitive(&glb);
    assert!(prim.get(&Semantic::TexCoords(0)).is_some());
    assert!(prim.get(&Semantic::TexCoords(1)).is_some());
    assert_eq!(b.gltf.meshes.as_ref().unwrap()[0].primitives[0].material, Some(material));
}

#[test]
fn build_into_without_indices_is_non_indexed() {
    let mut b = GltfBuilder::new();
    MeshBuilder::new(None).with_positions(POS.to_vec()).build_into(&mut b).unwrap();
    assert!(b.gltf.meshes.as_ref().unwrap()[0].primitives[0].indices.is_none());
}

#[test]
fn build_into_writes_joints_and_weights() {
    let mut b = GltfBuilder::new();
    base()
        .with_joints(vec![[0, 1, 0, 0], [1, 0, 0, 0], [0, 0, 0, 0]])
        .with_weights(vec![[0.5, 0.5, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]])
        .with_joint_count(2)
        .build_into(&mut b)
        .unwrap();

    let glb = common::parse_glb(&b);
    let prim = first_primitive(&glb);
    let reader = prim.reader(|_| glb.blob.as_deref());
    match reader.read_joints(0).unwrap() {
        ReadJoints::U16(mut it) => assert_eq!(it.next().unwrap(), [0, 1, 0, 0]),
        _ => panic!("expected U16"),
    }
    let w: Vec<[f32; 4]> = reader.read_weights(0).unwrap().into_f32().collect();
    assert_eq!(w[0], [0.5, 0.5, 0.0, 0.0]);
}

#[test]
fn joint_count_is_enforced_when_given() {
    let mut b = GltfBuilder::new();
    let r = base()
        .with_joints(vec![[5, 0, 0, 0]; 3])
        .with_weights(vec![[1.0, 0.0, 0.0, 0.0]; 3])
        .with_joint_count(2)
        .build_into(&mut b);
    assert!(matches!(r, Err(GltfError::InvalidInput(m)) if m.contains("out of range")));
    assert_eq!(counts(&b), (0, 0, 0));
}

#[test]
fn joints_without_weights_or_reverse_is_an_error_and_writes_nothing() {
    let mut b = GltfBuilder::new();
    assert!(matches!(
        base().with_joints(vec![[0; 4]; 3]).build_into(&mut b),
        Err(GltfError::InvalidInput(m)) if m.contains("weights were not")
    ));
    assert!(matches!(
        base().with_weights(vec![[1.0, 0.0, 0.0, 0.0]; 3]).build_into(&mut b),
        Err(GltfError::InvalidInput(m)) if m.contains("joints were not")
    ));
    assert!(base().with_joint_count(3).build_into(&mut b).is_err(), "joint count alone");
    assert_eq!(counts(&b), (0, 0, 0));
}

#[test]
fn invalid_geometry_is_rejected() {
    let mut b = GltfBuilder::new();
    assert!(MeshBuilder::new(None).build_into(&mut b).is_err(), "no positions");
    assert!(base().with_indices(vec![0, 1, 3]).build_into(&mut b).is_err(), "index out of range");
    assert!(base().with_normals(vec![0.0; 6]).build_into(&mut b).is_err(), "short normals");
    assert!(base().with_texcoords(vec![0.0; 4]).build_into(&mut b).is_err(), "short texcoords");
    assert_eq!(counts(&b), (0, 0, 0));
}

#[test]
fn bad_weights_are_rejected_not_silently_fixed() {
    let mut b = GltfBuilder::new();
    let r = base()
        .with_joints(vec![[0; 4]; 3])
        .with_weights(vec![[0.4, 0.0, 0.0, 0.0]; 3])
        .build_into(&mut b);
    assert!(matches!(r, Err(GltfError::InvalidInput(m)) if m.contains("sum")));
}

#[test]
fn custom_primitive_builder_is_still_honored() {
    let mut b = GltfBuilder::new();
    let idx = MeshBuilder::new(Some("C".into()))
        .with_positions(POS.to_vec())
        .with_primitive_builder(Box::new(|_, _, _, _, material| {
            let mut p = Primitive::default();
            p.material = material;
            p.mode = Some(0);
            p
        }))
        .with_material(7)
        .build_into(&mut b)
        .unwrap();
    let prim = &b.gltf.meshes.as_ref().unwrap()[idx].primitives[0];
    assert_eq!((prim.material, prim.mode), (Some(7), Some(0)));
}

#[test]
fn custom_primitive_builder_cannot_carry_skinning_data() {
    let mut b = GltfBuilder::new();
    let r = base()
        .with_primitive_builder(Box::new(|_, _, _, _, _| Primitive::default()))
        .with_joints(vec![[0; 4]; 3])
        .with_weights(vec![[1.0, 0.0, 0.0, 0.0]; 3])
        .build_into(&mut b);
    assert!(matches!(r, Err(GltfError::InvalidInput(m)) if m.contains("custom primitive builder")));
    assert_eq!(counts(&b), (0, 0, 0));
}
