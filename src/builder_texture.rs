use crate::builder::GltfBuilder;
use crate::constants::{sampler_filter, sampler_wrap};
use crate::error::Result;
use crate::models::{Image, Sampler, Texture};
use crate::texture;
use image::DynamicImage;

impl GltfBuilder {
    /// Add a material with a texture to the glTF document
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
                               double_sided: Option<bool>) -> usize {
        use crate::material;
        
        let material = material::create_textured_material(
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
        
        if let Some(materials) = &mut self.gltf.materials {
            let index = materials.len();
            materials.push(material);
            index
        } else {
            self.gltf.materials = Some(vec![material]);
            0
        }
    }
    
    /// Create a basic textured material
    pub fn create_textured_material(&mut self, name: Option<String>, 
                                  base_color_texture: usize) -> usize {
        self.add_textured_material(
            name,
            Some(base_color_texture),
            None,
            None,
            None,
            None,
            None,
            Some(1.0),
            Some(1.0),
            None,
            None,
            None
        )
    }
    
    /// Add an image to the glTF document from a DynamicImage using the specified format
    pub(crate) fn add_image_from_dynamic_image(&mut self, name: Option<String>, 
                                       image: &DynamicImage, 
                                       format: texture::TextureFormat) -> Result<usize> {
        let bytes = texture::image_to_bytes(image, format)?;
        
        Ok(self.add_image_from_buffer(name, format.mime_type().to_string(), &bytes))
    }
    
    /// Create a default texture sampler with reasonable settings
    pub fn create_default_sampler(&mut self) -> usize {
        self.add_sampler(
            Some(sampler_filter::LINEAR),
            Some(sampler_filter::LINEAR_MIPMAP_LINEAR),
            Some(sampler_wrap::REPEAT),
            Some(sampler_wrap::REPEAT)
        )
    }
    
    /// Create a default texture from a DynamicImage (uses default sampler)
    pub fn create_texture_from_image(&mut self, name: Option<String>, 
                                   image: &DynamicImage, 
                                   format: texture::TextureFormat) -> Result<usize> {
        // Add the image
        let image_index = self.add_image_from_dynamic_image(name.clone(), image, format)?;
        
        // Create a new default sampler for this texture
        let sampler_index = self.create_default_sampler();
        
        // Create the texture
        Ok(self.add_texture(name, image_index, Some(sampler_index)))
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
    
    /// Add a sampler to the glTF document
    pub(crate) fn add_sampler(&mut self, mag_filter: Option<usize>, min_filter: Option<usize>,
                      wrap_s: Option<usize>, wrap_t: Option<usize>) -> usize {
        let sampler = Sampler {
            mag_filter: mag_filter,
            min_filter: min_filter,
            wrap_s: wrap_s,
            wrap_t: wrap_t,
        };
        
        if let Some(samplers) = &mut self.gltf.samplers {
            let index = samplers.len();
            samplers.push(sampler);
            index
        } else {
            self.gltf.samplers = Some(vec![sampler]);
            0
        }
    }
    
    /// Add an image to the glTF document
    pub(crate) fn add_image_from_buffer(&mut self, name: Option<String>, 
                               mime_type: String, data: &[u8]) -> usize {
        // Add image data to buffer
        let (offset, length) = self.add_buffer_data(data);
        
        // Create buffer view for image data
        let buffer_view = self.add_buffer_view(offset, length, None);
        
        // Create image that references buffer view
        let image = Image {
            name,
            uri: None,
            mime_type: Some(mime_type),
            buffer_view: Some(buffer_view),
        };
        
        if let Some(images) = &mut self.gltf.images {
            let index = images.len();
            images.push(image);
            index
        } else {
            self.gltf.images = Some(vec![image]);
            0
        }
    }
    
    /// Add a texture to the glTF document
    pub(crate) fn add_texture(&mut self, name: Option<String>, source: usize, 
                      sampler: Option<usize>) -> usize {
        let texture = Texture {
            name,
            source,
            sampler,
        };
        
        if let Some(textures) = &mut self.gltf.textures {
            let index = textures.len();
            textures.push(texture);
            index
        } else {
            self.gltf.textures = Some(vec![texture]);
            0
        }
    }
}
