//! N3a: several in-memory buffers, a current write buffer, per-buffer validation.
//! Export of extra buffers is N3b; documents with more than one buffer are not exportable yet.
mod common;

use mesh_tools::{AccessorSpec, AccessorType, BufferViewTarget, GltfBuilder, GltfError, JointIndices, SkinSet};

/// Snapshot of everything a failed call must leave alone.
fn snap(b: &GltfBuilder) -> (String, Vec<Vec<u8>>) {
    let n = b.gltf.buffers.as_ref().map_or(0, |v| v.len());
    (serde_json::to_string(&b.gltf).unwrap(), (0..n).map(|i| b.buffer_bytes(i).unwrap_or(&[]).to_vec()).collect())
}

fn view(b: &GltfBuilder, i: usize) -> (usize, usize, usize) {
    let v = &b.gltf.buffer_views.as_ref().unwrap()[i];
    (v.buffer, v.byte_offset, v.byte_length)
}

#[test]
fn views_land_in_the_chosen_buffer_with_hand_derived_offsets() {
    let mut b = GltfBuilder::new();
    let v0 = b.add_data_view(&[1, 2, 3], None, None).unwrap(); // creates buffer 0
    assert_eq!(b.add_buffer(Some("extra".into())).unwrap(), 1);
    let v1 = b.add_data_view_in_buffer(1, &[9; 5], None, None).unwrap();
    let v2 = b.add_data_view_in_buffer(1, &[7; 2], None, None).unwrap();
    let v3 = b.add_data_view(&[4], None, None).unwrap(); // write buffer is still 0
    assert_eq!(view(&b, v0), (0, 0, 3));
    assert_eq!(view(&b, v1), (1, 0, 5));
    assert_eq!(view(&b, v2), (1, 8, 2)); // 5 bytes padded to 8
    assert_eq!(view(&b, v3), (0, 4, 1));
    assert_eq!(b.buffer_bytes(0).unwrap(), &[1, 2, 3, 0, 4]);
    assert_eq!(b.buffer_bytes(1).unwrap(), &[9, 9, 9, 9, 9, 0, 0, 0, 7, 7]);
    assert_eq!(b.buffer_bytes(2), None);
    let bufs = b.gltf.buffers.as_ref().unwrap();
    assert_eq!((bufs[0].byte_length, bufs[1].byte_length), (5, 10));
    assert_eq!(bufs[1].name.as_deref(), Some("extra"));
}

#[test]
fn set_write_buffer_redirects_every_writer() {
    let mut b = GltfBuilder::new();
    b.add_buffer(None).unwrap();
    b.add_buffer(None).unwrap();
    b.add_buffer(None).unwrap();
    b.set_write_buffer(1).unwrap();
    let a = b.add_accessor_from_slice(&[1.0f32, 2.0, 3.0], AccessorType::Vec3, AccessorSpec::new()).unwrap();
    let vi = b.gltf.accessors.as_ref().unwrap()[a].buffer_view.unwrap();
    assert_eq!(view(&b, vi), (1, 0, 12));
    assert_eq!(b.buffer_bytes(0).unwrap().len(), 0);
    assert_eq!(b.buffer_bytes(1).unwrap().len(), 12);
    // Accessor reads (Bounds::Compute) use the buffer of the view.
    let c = b.add_accessor_in_view(vi, 0, 1, AccessorType::Vec3, mesh_tools::ComponentType::F32, false, mesh_tools::Bounds::Compute).unwrap();
    let acc = &b.gltf.accessors.as_ref().unwrap()[c];
    assert_eq!(acc.max.as_deref(), Some(&[1.0, 2.0, 3.0][..]));
}

#[test]
fn failed_calls_leave_the_document_unchanged() {
    let mut b = GltfBuilder::new();
    b.add_data_view(&[1, 2, 3, 4], None, None).unwrap();
    b.add_buffer(None).unwrap();
    let before = snap(&b);
    assert!(matches!(b.set_write_buffer(2), Err(GltfError::InvalidInput(_))));
    assert!(matches!(b.add_data_view_in_buffer(2, &[1], None, None), Err(GltfError::InvalidInput(_))));
    assert!(matches!(b.add_data_view_in_buffer(1, &[], None, None), Err(GltfError::InvalidInput(_))));
    assert!(matches!(b.add_data_view_in_buffer(1, &[1; 8], None, Some(6)), Err(GltfError::InvalidInput(_))));
    assert!(b.add_data_view_in_buffer(1, &[1; 8], Some(BufferViewTarget::ElementArrayBuffer), Some(4)).is_err());
    assert_eq!(snap(&b), before);
    // A failed call through a redirected write buffer restores nothing it did not change, and
    // add_data_view_in_buffer never leaves the write buffer changed.
    b.add_data_view_in_buffer(1, &[5], None, None).unwrap();
    let v = b.add_data_view(&[6], None, None).unwrap();
    assert_eq!(view(&b, v).0, 0);
}

#[test]
fn add_buffer_and_set_write_buffer_on_an_empty_document() {
    let mut b = GltfBuilder::new();
    assert!(b.set_write_buffer(0).is_err()); // no buffer yet
    assert_eq!(b.add_buffer(None).unwrap(), 0);
    b.set_write_buffer(0).unwrap();
    assert!(b.validate().has_error("BUFFER_EMPTY"));
}

/// Property-style: random operation sequences against an oracle `Vec<Vec<u8>>`.
#[test]
fn random_writes_match_an_oracle() {
    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut next = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    for _ in 0..40 {
        let mut b = GltfBuilder::new();
        let mut oracle: Vec<Vec<u8>> = vec![Vec::new()];
        b.add_buffer(None).unwrap();
        let mut cur = 0usize;
        let mut views: Vec<(usize, usize, usize)> = Vec::new();
        for step in 0..60 {
            match next(4) {
                0 => {
                    assert_eq!(b.add_buffer(None).unwrap(), oracle.len());
                    oracle.push(Vec::new());
                }
                1 => {
                    let i = next(oracle.len() + 1);
                    let r = b.set_write_buffer(i);
                    assert_eq!(r.is_ok(), i < oracle.len());
                    if r.is_ok() {
                        cur = i;
                    }
                }
                _ => {
                    let data: Vec<u8> = (0..1 + next(9)).map(|k| (step + k) as u8).collect();
                    let target = if next(2) == 0 { cur } else { next(oracle.len() + 1) };
                    let r = b.add_data_view_in_buffer(target, &data, None, None);
                    if target >= oracle.len() {
                        assert!(r.is_err());
                        continue;
                    }
                    let o = &mut oracle[target];
                    while o.len() % 4 != 0 {
                        o.push(0);
                    }
                    views.push((target, o.len(), data.len()));
                    o.extend_from_slice(&data);
                    assert_eq!(view(&b, r.unwrap()), *views.last().unwrap());
                }
            }
        }
        for (i, o) in oracle.iter().enumerate() {
            assert_eq!(b.buffer_bytes(i).unwrap(), &o[..]);
            assert_eq!(b.gltf.buffers.as_ref().unwrap()[i].byte_length, o.len());
        }
    }
}

#[test]
fn single_buffer_documents_are_unchanged() {
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    assert_eq!(b.gltf.buffers.as_ref().unwrap().len(), 1);
    assert!(b.gltf.buffer_views.as_ref().unwrap().iter().all(|v| v.buffer == 0));
    let n = b.buffer_bytes(0).unwrap().len();
    assert_eq!(b.gltf.buffers.as_ref().unwrap()[0].byte_length, n);
}

// ---- validator ---------------------------------------------------------------------------------

#[test]
fn extra_buffer_without_a_uri_is_reported_per_buffer() {
    let mut b = GltfBuilder::new();
    b.add_data_view(&[1; 4], None, None).unwrap();
    b.add_buffer(None).unwrap();
    b.add_data_view_in_buffer(1, &[1; 4], None, None).unwrap();
    let r = b.validate();
    assert!(r.errors.iter().any(|e| e.starts_with("BUFFER_URI_MISSING") && e.contains("buffer 1")), "{r:#?}");
    assert_eq!(r.errors.len(), 1, "{r:#?}");
}

#[test]
fn buffer_length_mismatch_is_checked_for_every_buffer() {
    let mut b = GltfBuilder::new();
    b.add_data_view(&[1; 4], None, None).unwrap();
    b.add_buffer(None).unwrap();
    b.add_data_view_in_buffer(1, &[1; 4], None, None).unwrap();
    b.gltf.buffers.as_mut().unwrap()[1].byte_length = 9;
    let r = b.validate();
    assert!(r.errors.iter().any(|e| e.starts_with("BUFFER_LENGTH_MISMATCH") && e.contains("buffer 1")), "{r:#?}");
    assert!(!r.errors.iter().any(|e| e.starts_with("BUFFER_LENGTH_MISMATCH") && e.contains("buffer 0")));
}

#[test]
fn view_range_is_checked_against_its_own_buffer() {
    let mut b = GltfBuilder::new();
    b.add_data_view(&[1; 64], None, None).unwrap(); // buffer 0 is large
    b.add_buffer(None).unwrap();
    let v = b.add_data_view_in_buffer(1, &[1; 8], None, None).unwrap();
    assert!(!b.validate().errors.iter().any(|e| e.starts_with("BUFFER_VIEW_RANGE")));
    b.gltf.buffer_views.as_mut().unwrap()[v].byte_length = 16; // fits buffer 0, not buffer 1
    let r = b.validate();
    assert!(r.errors.iter().any(|e| e.starts_with("BUFFER_VIEW_RANGE") && e.contains(&format!("buffer view {v}"))), "{r:#?}");
}

#[test]
fn view_of_a_missing_buffer_is_invalid_and_not_range_checked() {
    let mut b = GltfBuilder::new();
    let v = b.add_data_view(&[1; 8], None, None).unwrap();
    b.gltf.buffer_views.as_mut().unwrap()[v].buffer = 3;
    let r = b.validate();
    assert!(r.errors.iter().any(|e| e.starts_with("BUFFER_VIEW_BUFFER_INVALID") && e.contains("buffer 3")), "{r:#?}");
    assert!(!r.errors.iter().any(|e| e.starts_with("BUFFER_VIEW_RANGE")), "{r:#?}");
}

#[test]
fn views_into_a_buffer_without_stored_bytes_warn_unverified() {
    let mut b = GltfBuilder::new();
    b.add_data_view(&[1; 8], None, None).unwrap();
    let mut buf = mesh_tools::Buffer::default();
    buf.uri = Some("ext.bin".into());
    buf.byte_length = 16;
    b.gltf.buffers.as_mut().unwrap().push(buf);
    let mut v = mesh_tools::BufferView::default();
    v.buffer = 1;
    v.byte_offset = 8;
    v.byte_length = 8;
    b.gltf.buffer_views.as_mut().unwrap().push(v);
    let r = b.validate();
    assert!(r.is_valid(), "{r:#?}");
    assert!(r.warnings.iter().any(|w| w.starts_with("BUFFER_DATA_UNVERIFIED") && w.contains("buffer 1")), "{r:#?}");
    // Declared length still bounds the view.
    b.gltf.buffer_views.as_mut().unwrap()[1].byte_length = 12;
    assert!(b.validate().has_error("BUFFER_VIEW_RANGE"));
}

// ---- N5/N6 validators read the buffer of their view ----------------------------------------------

fn skinned_in_buffer_1(bad_joint: bool) -> GltfBuilder {
    let mut b = GltfBuilder::new();
    let joints = b.create_bone_chain(&["A".into(), "B".into()], &[[0.0; 3], [0.0, 1.0, 0.0]]).unwrap();
    b.add_buffer(None).unwrap();
    b.add_buffer(None).unwrap();
    b.set_write_buffer(1).unwrap();
    let skin = b.add_skin_from_joints(None, joints.clone(), Some(joints[0])).unwrap();
    let pos = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let set = SkinSet::new(JointIndices::U8(vec![[0, 1, 0, 0]; 3]), vec![[200u8, 55, 0, 0]; 3]);
    let prim = b.create_skinned_primitive(&pos, Some(&[0, 1, 2]), None, None, &[set], 2, None).unwrap();
    let mesh = b.add_mesh(None, vec![prim]);
    if bad_joint {
        let j = b.add_accessor_from_slice(&[0u8, 5, 0, 0, 0, 5, 0, 0, 0, 5, 0, 0], AccessorType::Vec4, AccessorSpec::new().with_target(BufferViewTarget::ArrayBuffer)).unwrap();
        b.gltf.meshes.as_mut().unwrap()[mesh].primitives[0].attributes.insert("JOINTS_0".into(), j);
    }
    let node = b.add_skinned_node(None, mesh, skin, None, None, None).unwrap();
    b.add_scene(None, Some(vec![node, joints[0]]));
    b
}

#[test]
fn skin_data_in_buffer_1_is_read_from_buffer_1() {
    let good = skinned_in_buffer_1(false).validate();
    // Only the missing uri of buffer 1 remains; nothing is "outside the buffer".
    assert!(good.errors.iter().all(|e| e.starts_with("BUFFER_URI_MISSING") || e.starts_with("BUFFER_EMPTY")), "{good:#?}");
    let bad = skinned_in_buffer_1(true).validate();
    assert!(bad.has_error("SKIN_JOINT_OUT_OF_RANGE"), "{bad:#?}");
}

#[test]
fn animation_data_in_buffer_1_is_read_from_buffer_1() {
    let mut b = GltfBuilder::new();
    let n = b.add_node(None, None, None, None, None);
    b.add_scene(None, Some(vec![n]));
    b.add_buffer(None).unwrap();
    b.add_buffer(None).unwrap();
    b.set_write_buffer(1).unwrap();
    let a = b.add_animation(None);
    b.add_rotation_track(a, n, &[0.0, 1.0], &[[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0]], mesh_tools::InterpolationType::Linear).unwrap();
    // Replace the output with a non-unit rotation stored in buffer 1.
    let r = b.validate();
    assert!(r.errors.iter().all(|e| e.starts_with("BUFFER_URI_MISSING") || e.starts_with("BUFFER_EMPTY")), "{r:#?}");
    let acc = b.add_accessor_from_slice(&[0.0f32, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 1.0], AccessorType::Vec4, AccessorSpec::new()).unwrap();
    b.gltf.animations.as_mut().unwrap()[0].samplers.as_mut().unwrap()[0].output = acc;
    assert!(b.validate().has_error("ANIMATION_ROTATION_NOT_UNIT"));
}

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, x| (h ^ *x as u64).wrapping_mul(0x100000001b3))
}

/// Hashes recorded from the previous commit (0.20.0): single-buffer output must not change.
#[test]
fn single_buffer_output_is_byte_identical_to_the_previous_release() {
    let mut b = GltfBuilder::new();
    b.create_box(1.0);
    b.add_data_view(&[1, 2, 3], None, None).unwrap();
    let glb = b.to_glb_bytes().unwrap();
    assert_eq!((glb.len(), fnv(&glb)), (1236, 0x9691a24847ff9ce8));
    assert_eq!(fnv(b.to_gltf_string_embedded().unwrap().as_bytes()), 0xf91a1b43e2cef1e7);
}

fn only_missing_uri(r: &mesh_tools::ValidationReport) -> bool {
    r.errors.iter().all(|e| e.starts_with("BUFFER_URI_MISSING") || e.starts_with("BUFFER_EMPTY"))
}

#[test]
fn image_data_in_buffer_1_is_read_from_buffer_1() {
    use mesh_tools::texture::{create_uv_test_pattern, TextureFormat};
    let mut b = GltfBuilder::new();
    b.add_buffer(None).unwrap();
    b.add_buffer(None).unwrap();
    b.set_write_buffer(1).unwrap();
    let img = create_uv_test_pattern(8, 8).unwrap();
    b.add_image(None, &img, TextureFormat::PNG).unwrap();
    let r = b.validate();
    assert!(only_missing_uri(&r), "{r:#?}");
    // A view pointing at the empty buffer 0 cannot hold the image.
    b.gltf.buffer_views.as_mut().unwrap()[0].buffer = 0;
    assert!(b.validate().has_error("IMAGE_BUFFER_VIEW_INVALID"));
}

#[test]
fn sparse_data_in_buffer_1_is_read_from_buffer_1() {
    use mesh_tools::SparseSpec;
    let mut b = GltfBuilder::new();
    b.add_buffer(None).unwrap();
    b.add_buffer(None).unwrap();
    b.set_write_buffer(1).unwrap();
    let base = [0.0f32; 15];
    let s = SparseSpec::new(vec![1, 3], vec![1.0f32; 6]);
    b.add_sparse_accessor(5, AccessorType::Vec3, Some(&base), s, AccessorSpec::new()).unwrap();
    let r = b.validate();
    assert!(only_missing_uri(&r), "{r:#?}");
    // Shrink the accessor so index 3 is out of range: only reported if the index bytes are read from buffer 1.
    b.gltf.accessors.as_mut().unwrap()[0].count = 3;
    let r = b.validate();
    assert!(r.errors.iter().any(|e| e.starts_with("ACCESSOR_SPARSE_INDEX_RANGE")), "{r:#?}");
}

#[test]
fn writes_after_gltf_buffers_was_cleared_do_not_panic() {
    let mut b = GltfBuilder::new();
    b.add_buffer(None).unwrap();
    b.add_buffer(None).unwrap();
    b.set_write_buffer(1).unwrap();
    b.gltf.buffers = None;
    b.add_data_view(&[1, 2, 3, 4], None, None).unwrap();
    assert_eq!(b.gltf.buffers.as_ref().unwrap().len(), 2);
    assert_eq!(b.gltf.buffers.as_ref().unwrap()[1].byte_length, 4);
}
