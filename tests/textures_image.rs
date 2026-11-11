//! Images: embedded PNG/JPEG and external URIs, read back through the independent `gltf` crate.

mod common;

use gltf::image::Source;
use image::{DynamicImage, ImageBuffer, Rgb};
use mesh_tools::texture::{self, TextureFormat};
use mesh_tools::GltfBuilder;

/// 2x2 RGB image with hand-chosen pixels: red, green / blue, yellow.
fn pixels_2x2() -> (DynamicImage, [u8; 12]) {
    let px: [u8; 12] = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    let img = ImageBuffer::<Rgb<u8>, _>::from_raw(2, 2, px.to_vec()).unwrap();
    (DynamicImage::ImageRgb8(img), px)
}

fn with_texture(b: &mut GltfBuilder, image: usize) {
    b.add_texture(None, image, None).unwrap();
}

#[test]
fn magic_numbers() {
    let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];
    assert_eq!(TextureFormat::from_magic(&png), Some(TextureFormat::PNG));
    assert_eq!(TextureFormat::from_magic(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(TextureFormat::JPEG));
    assert_eq!(TextureFormat::from_magic(&[0xFF, 0xD8]), None);
    assert_eq!(TextureFormat::from_magic(b"GIF89a"), None);
    assert_eq!(TextureFormat::from_magic(&[]), None);
}

#[test]
fn png_pixels_round_trip_exactly() {
    let (img, px) = pixels_2x2();
    let mut b = GltfBuilder::new();
    let i = b.add_image(Some("quad".into()), &img, TextureFormat::PNG).unwrap();
    with_texture(&mut b, i);
    let glb = b.to_glb_bytes().unwrap();
    let (doc, _buffers, images) = gltf::import_slice(&glb).unwrap();
    let im = doc.images().next().unwrap();
    assert_eq!(im.name(), Some("quad"));
    match im.source() {
        Source::View { mime_type, .. } => assert_eq!(mime_type, "image/png"),
        _ => panic!("expected an embedded image"),
    }
    assert_eq!((images[0].width, images[0].height), (2, 2));
    assert_eq!(images[0].format, gltf::image::Format::R8G8B8);
    assert_eq!(images[0].pixels, px);
}

#[test]
fn jpeg_pixels_within_tolerance() {
    let img = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(8, 8, Rgb([200u8, 100, 50])));
    let mut b = GltfBuilder::new();
    let i = b.add_image(None, &img, TextureFormat::JPEG).unwrap();
    with_texture(&mut b, i);
    let glb = b.to_glb_bytes().unwrap();
    let (doc, _, images) = gltf::import_slice(&glb).unwrap();
    match doc.images().next().unwrap().source() {
        Source::View { mime_type, .. } => assert_eq!(mime_type, "image/jpeg"),
        _ => panic!(),
    }
    assert_eq!((images[0].width, images[0].height), (8, 8));
    for p in images[0].pixels.chunks(3) {
        for (got, want) in p.iter().zip([200i32, 100, 50]) {
            assert!((*got as i32 - want).abs() <= 6, "{p:?}");
        }
    }
}

#[test]
fn embedded_bytes_are_byte_identical_and_buffer_stays_aligned() {
    let (img, _) = pixels_2x2();
    let bytes = texture::image_to_bytes(&img, TextureFormat::PNG).unwrap();
    let mut b = GltfBuilder::new();
    let i = b.add_image_bytes(None, TextureFormat::PNG, &bytes).unwrap();
    with_texture(&mut b, i);
    // A mesh after the image: every accessor must stay 4-aligned whatever the PNG length
    let mesh = b.create_box(1.0);
    let n = b.add_node(None, Some(mesh), None, None, None);
    b.add_scene(None, Some(vec![n]));
    assert!(b.validate().is_valid(), "{:?}", b.validate());
    let g = common::parse_glb(&b);
    let view = g.document.images().next().map(|im| match im.source() {
        Source::View { view, .. } => view,
        _ => panic!(),
    }).unwrap();
    let blob = g.blob.as_ref().unwrap();
    assert_eq!(&blob[view.offset()..view.offset() + view.length()], &bytes[..]);
    for a in g.document.accessors() {
        let v = a.view().unwrap();
        assert_eq!((v.offset() + a.offset()) % 4, 0);
    }
}

#[test]
fn add_image_bytes_rejects_bad_input_atomically() {
    let (img, _) = pixels_2x2();
    let png = texture::image_to_bytes(&img, TextureFormat::PNG).unwrap();
    let mut b = GltfBuilder::new();
    // wrong declared format
    assert!(b.add_image_bytes(None, TextureFormat::JPEG, &png).is_err());
    // not an image at all
    assert!(b.add_image_bytes(None, TextureFormat::PNG, b"hello world").is_err());
    // empty
    assert!(b.add_image_bytes(None, TextureFormat::PNG, &[]).is_err());
    // right magic number, truncated header
    assert!(b.add_image_bytes(None, TextureFormat::PNG, &png[..12]).is_err());
    assert!(b.gltf.images.as_ref().map_or(true, |v| v.is_empty()));
    assert!(b.gltf.buffers.as_ref().map_or(true, |v| v.is_empty()));
    assert!(b.gltf.buffer_views.as_ref().map_or(true, |v| v.is_empty()));
    let json = serde_json::to_string(&b.gltf).unwrap();
    b.add_image_bytes(None, TextureFormat::PNG, &png).unwrap();
    assert_ne!(json, serde_json::to_string(&b.gltf).unwrap());
}

#[test]
fn external_uri_round_trips_with_and_without_mime() {
    let mut b = GltfBuilder::new();
    let a = b.add_image_uri(Some("a".into()), "textures/a.png", None).unwrap();
    let c = b.add_image_uri(None, "c.jpg", Some(TextureFormat::JPEG)).unwrap();
    with_texture(&mut b, a);
    with_texture(&mut b, c);
    let g = common::parse_glb(&b);
    let ims: Vec<_> = g.document.images().collect();
    match ims[0].source() {
        Source::Uri { uri, mime_type } => {
            assert_eq!(uri, "textures/a.png");
            assert_eq!(mime_type, None);
        }
        _ => panic!(),
    }
    match ims[1].source() {
        Source::Uri { uri, mime_type } => {
            assert_eq!(uri, "c.jpg");
            assert_eq!(mime_type, Some("image/jpeg"));
        }
        _ => panic!(),
    }
    // URI images add no buffer data
    assert!(b.gltf.buffers.as_ref().map_or(true, |v| v.is_empty()));
}

#[test]
fn add_image_uri_rejects_empty_and_data_uris() {
    let mut b = GltfBuilder::new();
    assert!(b.add_image_uri(None, "", None).is_err());
    assert!(b.add_image_uri(None, "data:image/png;base64,AAAA", None).is_err());
    assert!(b.gltf.images.as_ref().map_or(true, |v| v.is_empty()));
}

#[test]
fn add_image_from_file_embeds_and_detects_format() {
    let (img, px) = pixels_2x2();
    let dir = std::env::temp_dir().join(format!("mesh_tools_img_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("quad.bin"); // extension is irrelevant, the magic number decides
    std::fs::write(&path, texture::image_to_bytes(&img, TextureFormat::PNG).unwrap()).unwrap();
    let mut b = GltfBuilder::new();
    let i = b.add_image_from_file(Some("f".into()), &path).unwrap();
    with_texture(&mut b, i);
    let (_, _, images) = gltf::import_slice(&b.to_glb_bytes().unwrap()).unwrap();
    assert_eq!(images[0].pixels, px);

    let bad = dir.join("bad.png");
    std::fs::write(&bad, b"not an image").unwrap();
    assert!(b.add_image_from_file(None, &bad).is_err());
    assert!(b.add_image_from_file(None, dir.join("missing.png")).is_err());
    assert_eq!(b.gltf.images.as_ref().unwrap().len(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn validator_flags_hand_edited_images() {
    let (img, _) = pixels_2x2();
    let mk = || {
        let mut b = GltfBuilder::new();
        let i = b.add_image(None, &img, TextureFormat::PNG).unwrap();
        with_texture(&mut b, i);
        b
    };
    assert!(mk().validate().is_valid());

    let mut b = mk();
    b.gltf.images.as_mut().unwrap()[0].uri = Some("x.png".into());
    assert!(b.validate().has_error("IMAGE_SOURCE_AMBIGUOUS"));

    let mut b = mk();
    b.gltf.images.as_mut().unwrap()[0].buffer_view = None;
    assert!(b.validate().has_error("IMAGE_SOURCE_AMBIGUOUS"));

    let mut b = mk();
    b.gltf.images.as_mut().unwrap()[0].mime_type = None;
    assert!(b.validate().has_error("IMAGE_MIME_MISSING"));

    let mut b = mk();
    b.gltf.images.as_mut().unwrap()[0].mime_type = Some("image/gif".into());
    assert!(b.validate().has_error("IMAGE_MIME_INVALID"));

    let mut b = mk();
    b.gltf.images.as_mut().unwrap()[0].mime_type = Some("image/jpeg".into());
    assert!(b.validate().has_error("IMAGE_DATA_MISMATCH"));

    let mut b = mk();
    b.gltf.images.as_mut().unwrap()[0].buffer_view = Some(99);
    assert!(b.validate().has_error("IMAGE_BUFFER_VIEW_INVALID"));

    let mut b = mk();
    b.gltf.buffer_views.as_mut().unwrap()[0].byte_stride = Some(4);
    assert!(b.validate().has_error("IMAGE_BUFFER_VIEW_INVALID"));

    let mut b = GltfBuilder::new();
    let i = b.add_image_uri(None, "a.png", None).unwrap();
    with_texture(&mut b, i);
    b.gltf.images.as_mut().unwrap()[0].uri = Some(String::new());
    assert!(b.validate().has_error("IMAGE_URI_EMPTY"));
    b.gltf.images.as_mut().unwrap()[0].uri = Some("a.png".into());
    b.gltf.images.as_mut().unwrap()[0].mime_type = Some("text/plain".into());
    assert!(b.validate().has_error("IMAGE_MIME_INVALID"));
}

#[test]
fn validator_rejects_overflowing_or_out_of_range_image_view() {
    let (img, _) = pixels_2x2();
    for (off, len) in [(usize::MAX, 8), (0, usize::MAX), (0, 1 << 30)] {
        let mut b = GltfBuilder::new();
        let i = b.add_image(None, &img, TextureFormat::PNG).unwrap();
        with_texture(&mut b, i);
        let v = &mut b.gltf.buffer_views.as_mut().unwrap()[0];
        v.byte_offset = off;
        v.byte_length = len;
        assert!(b.validate().has_error("IMAGE_BUFFER_VIEW_INVALID"));
    }
}
