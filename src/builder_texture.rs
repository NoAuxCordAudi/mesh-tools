use crate::builder::GltfBuilder;
use crate::error::{GltfError, Result};
use crate::models::{Image, Texture};
use crate::sampler::SamplerDesc;
use std::path::Path;
use crate::texture;
use image::DynamicImage;

impl GltfBuilder {
    /// Add a metallic-roughness material from texture indices (all sampled with `TEXCOORD_0`).
    ///
    /// Deprecated: use [`MaterialBuilder`](crate::material::MaterialBuilder) (per-slot `texCoord`,
    /// scale, strength) and [`add_material_from`](GltfBuilder::add_material_from).
    ///
    /// # Errors
    /// `GltfError::InvalidMaterial` for a dangling texture index, an out-of-range factor or an
    /// invalid alpha mode (all previously accepted silently). The document is unchanged on error.
    ///
    /// ```
    /// # #![allow(deprecated)]
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// let bad = b.add_textured_material(None, Some(7), None, None, None, None, None, None, None, None, None, None);
    /// assert!(bad.is_err());
    /// let ok = b.add_textured_material(None, None, None, None, None, None, None, Some(0.5), None, None, None, None);
    /// assert_eq!(ok.unwrap(), 0);
    /// ```
    #[deprecated(since = "0.13.0", note = "use `MaterialBuilder` and `add_material_from`")]
    pub fn add_textured_material(&mut self, name: Option<String>, 
                               base_color_texture: Option<usize>,
                               metallic_roughness_texture: Option<usize>,
                               normal_texture: Option<usize>,
                               occlusion_texture: Option<usize>,
                               emissive_texture: Option<usize>,
                               emissive_factor: Option<[f32; 3]>,
                               metallic_factor: Option<f32>,
                               roughness_factor: Option<f32>,
                               alpha_mode: Option<String>,
                               alpha_cutoff: Option<f32>,
                               double_sided: Option<bool>) -> Result<usize> {
        let material = crate::material::create_textured_material(
            name,
            base_color_texture,
            metallic_roughness_texture,
            normal_texture,
            occlusion_texture,
            emissive_texture,
            emissive_factor,
            metallic_factor,
            roughness_factor,
            alpha_mode,
            alpha_cutoff,
            double_sided
        );
        self.add_material_from(material)
    }
    
    /// Create a basic textured material (base color texture, metallic and roughness 1.0).
    ///
    /// # Errors
    /// `GltfError::InvalidMaterial` (`MATERIAL_TEXTURE_INVALID`) if the texture does not exist.
    pub fn create_textured_material(&mut self, name: Option<String>, 
                                  base_color_texture: usize) -> Result<usize> {
        let material = crate::material::MaterialBuilder::new(name)
            .with_base_color_texture(base_color_texture)
            .with_metallic_factor(1.0)
            .with_roughness_factor(1.0)
            .build();
        self.add_material_from(material)
    }
    
    /// Encode `image` as `format` and embed it as a new image (bytes in the binary buffer).
    ///
    /// # Errors
    /// `GltfError::Texture` if encoding fails. The document is unchanged on error.
    ///
    /// ```
    /// use mesh_tools::{texture::{create_uv_test_pattern, TextureFormat}, GltfBuilder};
    /// let mut b = GltfBuilder::new();
    /// let img = create_uv_test_pattern(8, 8).unwrap();
    /// assert_eq!(b.add_image(None, &img, TextureFormat::PNG).unwrap(), 0);
    /// ```
    pub fn add_image(&mut self, name: Option<String>, image: &DynamicImage,
                     format: texture::TextureFormat) -> Result<usize> {
        let bytes = texture::image_to_bytes(image, format)?;
        Ok(self.add_image_from_buffer(name, format.mime_type().to_string(), &bytes))
    }

    /// Embed already-encoded image bytes. The magic number must match `format` and the header
    /// must be readable (header only, pixels are not decoded).
    ///
    /// # Errors
    /// `GltfError::InvalidInput` if the bytes are not a PNG/JPEG of the declared format or the
    /// header is truncated or has a zero dimension. The document is unchanged on error.
    ///
    /// ```
    /// use mesh_tools::{texture::TextureFormat, GltfBuilder};
    /// let mut b = GltfBuilder::new();
    /// assert!(b.add_image_bytes(None, TextureFormat::PNG, b"not a png").is_err());
    /// assert!(b.gltf.images.as_ref().map_or(true, |v| v.is_empty()));
    /// ```
    pub fn add_image_bytes(&mut self, name: Option<String>, format: texture::TextureFormat,
                           bytes: &[u8]) -> Result<usize> {
        check_image_bytes(format, bytes)?;
        Ok(self.add_image_from_buffer(name, format.mime_type().to_string(), bytes))
    }

    /// Embed already-encoded image bytes as a base64 `data:` URI in the image's `uri`, so the image
    /// travels inside the JSON and adds nothing to the binary buffer. Same checks as
    /// [`add_image_bytes`](Self::add_image_bytes).
    ///
    /// # Errors
    /// `GltfError::InvalidInput` if the bytes are not a PNG/JPEG of the declared format. The
    /// document is unchanged on error.
    ///
    /// ```
    /// use mesh_tools::{texture::{create_uv_test_pattern, image_to_bytes, TextureFormat}, GltfBuilder};
    /// let mut b = GltfBuilder::new();
    /// let png = image_to_bytes(&create_uv_test_pattern(8, 8).unwrap(), TextureFormat::PNG).unwrap();
    /// let i = b.add_image_data_uri(None, TextureFormat::PNG, &png).unwrap();
    /// assert!(b.inspect_gltf().images.as_ref().unwrap()[i].uri.as_ref().unwrap().starts_with("data:image/png;base64,"));
    /// assert!(b.add_image_data_uri(None, TextureFormat::PNG, b"not a png").is_err());
    /// ```
    pub fn add_image_data_uri(&mut self, name: Option<String>, format: texture::TextureFormat,
                              bytes: &[u8]) -> Result<usize> {
        check_image_bytes(format, bytes)?;
        let image = Image {
            name,
            uri: Some(crate::uri::encode_data_uri(format.mime_type(), bytes)),
            mime_type: Some(format.mime_type().to_string()),
            ..Default::default()
        };
        Ok(push(&mut self.gltf.images, image))
    }

    /// Reference an external image by relative URI. `format` sets the optional `mimeType`.
    /// The file is not read and its existence is not checked.
    ///
    /// # Errors
    /// `GltfError::InvalidInput` for an empty, absolute, scheme-qualified or `..` URI, or a `data:` URI (use
    /// [`add_image_data_uri`](Self::add_image_data_uri)).
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// assert_eq!(b.add_image_uri(None, "textures/a.png", None).unwrap(), 0);
    /// assert!(b.add_image_uri(None, "", None).is_err());
    /// ```
    pub fn add_image_uri(&mut self, name: Option<String>, uri: &str,
                         format: Option<texture::TextureFormat>) -> Result<usize> {
        if uri.is_empty() {
            return Err(GltfError::InvalidInput("image uri is empty".into()));
        }
        if uri.get(..5).map_or(false, |p| p.eq_ignore_ascii_case("data:")) {
            return Err(GltfError::InvalidInput("data: URIs are added with add_image_data_uri".into()));
        }
        crate::uri::check_relative_uri(uri).map_err(|e| GltfError::InvalidInput(format!("image {e}")))?;
        let image = Image {
            name,
            uri: Some(uri.to_string()),
            mime_type: format.map(|f| f.mime_type().to_string()),
            ..Default::default()
        };
        Ok(push(&mut self.gltf.images, image))
    }

    /// Read a PNG or JPEG file and embed it. The format comes from the magic number, not the
    /// file extension.
    ///
    /// # Errors
    /// `GltfError::Io` if the file cannot be read, `GltfError::InvalidInput` if it is not a
    /// readable PNG/JPEG. The document is unchanged on error.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// assert!(b.add_image_from_file(None, "/nonexistent/x.png").is_err());
    /// ```
    pub fn add_image_from_file<P: AsRef<Path>>(&mut self, name: Option<String>, path: P) -> Result<usize> {
        let bytes = std::fs::read(path.as_ref())?;
        let format = texture::TextureFormat::from_magic(&bytes).ok_or_else(|| {
            GltfError::InvalidInput(format!("{} is not a PNG or JPEG file", path.as_ref().display()))
        })?;
        self.add_image_bytes(name, format, &bytes)
    }

    /// Create the shared default sampler (see [`SamplerDesc::default_sampler`]) if it does not
    /// exist yet and return its index. Repeated calls return the same index.
    pub fn create_default_sampler(&mut self) -> usize {
        self.get_or_add_sampler(SamplerDesc::default_sampler())
    }

    /// Create a texture from a DynamicImage using the shared default sampler.
    pub fn create_texture_from_image(&mut self, name: Option<String>,
                                   image: &DynamicImage,
                                   format: texture::TextureFormat) -> Result<usize> {
        self.create_texture_from_image_with_sampler(name, image, format, SamplerDesc::default_sampler())
    }

    /// Create a texture from a DynamicImage with the given sampler (reused if an identical
    /// plain sampler exists, see [`get_or_add_sampler`](Self::get_or_add_sampler)).
    ///
    /// # Errors
    /// `GltfError::Texture` if encoding fails. The document is unchanged on error.
    ///
    /// ```
    /// use mesh_tools::{texture::{create_uv_test_pattern, TextureFormat}, GltfBuilder, SamplerDesc, WrapMode};
    /// let mut b = GltfBuilder::new();
    /// let img = create_uv_test_pattern(8, 8).unwrap();
    /// let desc = SamplerDesc { wrap_s: Some(WrapMode::ClampToEdge), ..Default::default() };
    /// let t = b.create_texture_from_image_with_sampler(None, &img, TextureFormat::PNG, desc).unwrap();
    /// assert_eq!(t, 0);
    /// ```
    pub fn create_texture_from_image_with_sampler(&mut self, name: Option<String>,
                                   image: &DynamicImage,
                                   format: texture::TextureFormat,
                                   sampler: SamplerDesc) -> Result<usize> {
        // Encode first: nothing is added if it fails
        let bytes = texture::image_to_bytes(image, format)?;
        let image_index = self.add_image_from_buffer(name.clone(), format.mime_type().to_string(), &bytes);
        let sampler_index = self.get_or_add_sampler(sampler);
        self.add_texture(name, image_index, Some(sampler_index))
    }

    /// Create a checkerboard texture (for testing)
    pub fn create_checkerboard_texture(&mut self,
                                     width: u32,
                                     height: u32,
                                     cell_size: u32,
                                     color1: [u8; 3],
                                     color2: [u8; 3]) -> Result<usize> {
        let image = texture::create_colored_checkerboard(
            width,
            height,
            cell_size,
            color1,
            color2
        )?;

        self.create_texture_from_image(Some("checkerboard".to_string()), &image, texture::TextureFormat::PNG)
    }

    /// Create a UV test pattern texture (for testing)
    pub fn create_uv_test_texture(&mut self, width: u32, height: u32) -> Result<usize> {
        let image = texture::create_uv_test_pattern(width, height)?;

        self.create_texture_from_image(Some("uv_test".to_string()), &image, texture::TextureFormat::PNG)
    }

    /// Append a sampler and return its index. Never reuses an existing one; see
    /// [`get_or_add_sampler`](Self::get_or_add_sampler).
    ///
    /// ```
    /// use mesh_tools::{GltfBuilder, MagFilter, SamplerDesc};
    /// let mut b = GltfBuilder::new();
    /// let desc = SamplerDesc { mag_filter: Some(MagFilter::Nearest), ..Default::default() };
    /// assert_eq!(b.add_sampler(desc), 0);
    /// assert_eq!(b.add_sampler(desc), 1);
    /// ```
    pub fn add_sampler(&mut self, desc: SamplerDesc) -> usize {
        push(&mut self.gltf.samplers, desc.to_model())
    }

    /// Set or clear the `name` of sampler `index`.
    ///
    /// # Errors
    /// `GltfError::InvalidInput` if the sampler does not exist. The document is unchanged on error.
    ///
    /// ```
    /// use mesh_tools::{GltfBuilder, SamplerDesc};
    /// let mut b = GltfBuilder::new();
    /// let s = b.add_sampler(SamplerDesc::default());
    /// b.set_sampler_name(s, Some("pixelated".into())).unwrap();
    /// assert!(b.set_sampler_name(5, None).is_err());
    /// ```
    pub fn set_sampler_name(&mut self, index: usize, name: Option<String>) -> Result<()> {
        let n = self.gltf.samplers.as_ref().map_or(0, |v| v.len());
        match self.gltf.samplers.as_mut().and_then(|v| v.get_mut(index)) {
            Some(s) => {
                s.name = name;
                Ok(())
            }
            None => Err(GltfError::InvalidInput(format!("sampler {index} does not exist ({n} present)"))),
        }
    }

    /// Return the first existing sampler equal to `desc` (same four fields, and no name,
    /// extensions or extras), or add one. The comparison is by full struct equality, so
    /// `SamplerDesc::default()` only matches an empty sampler.
    ///
    /// ```
    /// use mesh_tools::{GltfBuilder, SamplerDesc};
    /// let mut b = GltfBuilder::new();
    /// let a = b.get_or_add_sampler(SamplerDesc::default_sampler());
    /// assert_eq!(b.get_or_add_sampler(SamplerDesc::default_sampler()), a);
    /// ```
    pub fn get_or_add_sampler(&mut self, desc: SamplerDesc) -> usize {
        let found = self.gltf.samplers.as_ref().and_then(|v| v.iter().position(|s| desc.matches(s)));
        found.unwrap_or_else(|| self.add_sampler(desc))
    }

    /// Add an image to the glTF document
    pub(crate) fn add_image_from_buffer(&mut self, name: Option<String>,
                               mime_type: String, data: &[u8]) -> usize {
        let (offset, length) = self.add_buffer_data(data);
        let buffer_view = self.add_buffer_view(offset, length, None);
        let image = Image {
            name,
            uri: None,
            mime_type: Some(mime_type),
            buffer_view: Some(buffer_view),
            ..Default::default()
        };
        push(&mut self.gltf.images, image)
    }

    /// Add a texture from an existing image and an optional sampler.
    ///
    /// # Errors
    /// `GltfError::InvalidInput` if `source` or `sampler` does not exist. The document is
    /// unchanged on error.
    ///
    /// ```
    /// use mesh_tools::{GltfBuilder, SamplerDesc};
    /// let mut b = GltfBuilder::new();
    /// assert!(b.add_texture(None, 0, None).is_err()); // no image yet
    /// let img = b.add_image_uri(None, "a.png", None).unwrap();
    /// let s = b.add_sampler(SamplerDesc::default());
    /// assert_eq!(b.add_texture(None, img, Some(s)).unwrap(), 0);
    /// ```
    pub fn add_texture(&mut self, name: Option<String>, source: usize,
                      sampler: Option<usize>) -> Result<usize> {
        let images = self.gltf.images.as_ref().map_or(0, |v| v.len());
        if source >= images {
            return Err(GltfError::InvalidInput(format!("image {source} does not exist ({images} present)")));
        }
        if let Some(s) = sampler {
            let n = self.gltf.samplers.as_ref().map_or(0, |v| v.len());
            if s >= n {
                return Err(GltfError::InvalidInput(format!("sampler {s} does not exist ({n} present)")));
            }
        }
        let texture = Texture {
            name,
            source,
            sampler,
            ..Default::default()
        };
        Ok(push(&mut self.gltf.textures, texture))
    }
}

/// Append to an optional array, creating it on first use, and return the new index.
fn push<T>(list: &mut Option<Vec<T>>, item: T) -> usize {
    let v = list.get_or_insert_with(Vec::new);
    v.push(item);
    v.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::TextureFormat;

    fn png_builder() -> GltfBuilder {
        let img = texture::create_uv_test_pattern(8, 8).unwrap();
        let mut b = GltfBuilder::new();
        let i = b.add_image(None, &img, TextureFormat::PNG).unwrap();
        b.add_texture(None, i, None).unwrap();
        b
    }

    #[test]
    fn zero_width_png_header_is_flagged() {
        let mut b = png_builder();
        let off = b.gltf.buffer_views.as_ref().unwrap()[0].byte_offset;
        b.buffer_data[0][off + 16..off + 20].copy_from_slice(&[0, 0, 0, 0]);
        let r = b.validate();
        assert!(r.has_error("IMAGE_DATA_UNDECODABLE") || r.has_error("IMAGE_DIMENSIONS_INVALID"), "{r:?}");
    }

    #[test]
    fn destroyed_header_is_flagged() {
        let mut b = png_builder();
        let off = b.gltf.buffer_views.as_ref().unwrap()[0].byte_offset;
        for x in &mut b.buffer_data[0][off + 8..off + 24] {
            *x = 0xAA;
        }
        assert!(b.validate().has_error("IMAGE_DATA_UNDECODABLE"));
    }
}

/// Magic number, header and size checks shared by the byte-based image writers.
fn check_image_bytes(format: texture::TextureFormat, bytes: &[u8]) -> Result<()> {
    match texture::TextureFormat::from_magic(bytes) {
        Some(f) if f == format => {}
        found => {
            return Err(GltfError::InvalidInput(format!(
                "image bytes do not start with a {} signature (detected {:?})",
                format.mime_type(), found
            )))
        }
    }
    let (w, h) = format.header_dimensions(bytes).map_err(|e| {
        GltfError::InvalidInput(format!("unreadable {} header: {e}", format.mime_type()))
    })?;
    if w == 0 || h == 0 {
        return Err(GltfError::InvalidInput(format!("image has zero size {w}x{h}")));
    }
    Ok(())
}
