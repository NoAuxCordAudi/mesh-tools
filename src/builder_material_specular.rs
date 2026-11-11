use crate::builder::GltfBuilder;
use crate::models::{MaterialExtensions, PbrSpecularGlossiness, TextureInfo};

impl GltfBuilder {
    /// Add a material using the specular-glossiness workflow to the glTF document
    /// 
    /// This function allows you to create a PBR material using the specular-glossiness workflow
    /// as defined in the KHR_materials_pbrSpecularGlossiness extension.
    /// 
    /// # Parameters
    /// 
    /// * `name` - Optional name for the material
    /// * `diffuse_factor` - Optional RGBA diffuse color array `[r, g, b, a]` where each component is in range 0.0-1.0
    /// * `diffuse_texture` - Optional index of a texture containing diffuse color values
    /// * `specular_factor` - Optional RGB specular color array `[r, g, b]` where each component is in range 0.0-1.0
    /// * `glossiness_factor` - Optional glossiness value in range 0.0-1.0 (0.0 = rough, 1.0 = smooth)
    /// * `specular_glossiness_texture` - Optional index of a texture containing specular and glossiness values
    /// * `normal_texture` - Optional index of a normal map texture
    /// * `occlusion_texture` - Optional index of an occlusion map texture
    /// * `emissive_texture` - Optional index of an emissive map texture
    /// * `emissive_factor` - Optional RGB emissive color array `[r, g, b]` where each component is in range 0.0-1.0
    /// * `alpha_mode` - Optional alpha rendering mode ("OPAQUE", "MASK", or "BLEND")
    /// * `alpha_cutoff` - Optional alpha cutoff value for "MASK" mode
    /// * `double_sided` - Optional flag to enable double-sided rendering
    /// 
    /// # Returns
    /// 
    /// The index of the created material in the glTF document
    pub fn add_specular_glossiness_material(
        &mut self,
        name: Option<String>,
        diffuse_factor: Option<[f32; 4]>,
        diffuse_texture: Option<usize>,
        specular_factor: Option<[f32; 3]>,
        glossiness_factor: Option<f32>,
        specular_glossiness_texture: Option<usize>,
        normal_texture: Option<usize>,
        occlusion_texture: Option<usize>,
        emissive_texture: Option<usize>,
        emissive_factor: Option<[f32; 3]>,
        alpha_mode: Option<String>,
        alpha_cutoff: Option<f32>,
        double_sided: Option<bool>,
    ) -> usize {
        // Register the extension in the glTF document
        if self.gltf.extensions_used.is_none() {
            self.gltf.extensions_used = Some(vec!["KHR_materials_pbrSpecularGlossiness".to_string()]);
        } else if let Some(ext_used) = &mut self.gltf.extensions_used {
            if !ext_used.contains(&"KHR_materials_pbrSpecularGlossiness".to_string()) {
                ext_used.push("KHR_materials_pbrSpecularGlossiness".to_string());
            }
        }
        
        // Create the material
        let mut material = crate::models::Material::default();
        material.name = name;
        
        // Set optional properties
        if alpha_mode.is_some() {
            material.alpha_mode = alpha_mode;
        }
        
        if alpha_cutoff.is_some() {
            material.alpha_cutoff = alpha_cutoff;
        }
        
        if double_sided.is_some() {
            material.double_sided = double_sided;
        }
        
        if emissive_factor.is_some() {
            material.emissive_factor = emissive_factor;
        }
        
        // Create texture infos if needed
        if let Some(normal_texture_index) = normal_texture {
            material.normal_texture = Some(crate::models::NormalTextureInfo {
                index: normal_texture_index,
                tex_coord: Some(0),
                scale: Some(1.0),
                ..Default::default()
            });
        }
        
        if let Some(occlusion_texture_index) = occlusion_texture {
            material.occlusion_texture = Some(crate::models::OcclusionTextureInfo {
                index: occlusion_texture_index,
                tex_coord: Some(0),
                strength: Some(1.0),
                ..Default::default()
            });
        }
        
        if let Some(emissive_texture_index) = emissive_texture {
            material.emissive_texture = Some(TextureInfo {
                index: emissive_texture_index,
                tex_coord: Some(0),
                ..Default::default()
            });
        }
        
        // Create specular-glossiness extension
        let mut pbr_specular_glossiness = PbrSpecularGlossiness::default();
        
        pbr_specular_glossiness.diffuse_factor = diffuse_factor;
        pbr_specular_glossiness.specular_factor = specular_factor;
        pbr_specular_glossiness.glossiness_factor = glossiness_factor;
        
        if let Some(diffuse_texture_index) = diffuse_texture {
            pbr_specular_glossiness.diffuse_texture = Some(TextureInfo {
                index: diffuse_texture_index,
                tex_coord: Some(0),
                ..Default::default()
            });
        }
        
        if let Some(specular_glossiness_texture_index) = specular_glossiness_texture {
            pbr_specular_glossiness.specular_glossiness_texture = Some(TextureInfo {
                index: specular_glossiness_texture_index,
                tex_coord: Some(0),
                ..Default::default()
            });
        }
        
        // Add extension to material
        let material_extensions = MaterialExtensions {
            pbr_specular_glossiness: Some(pbr_specular_glossiness),
            ..Default::default()
        };
        
        material.extensions = Some(material_extensions);
        
        // Add the material to the glTF document
        self.add_material_direct(material)
    }
    
    /// Create a simple specular material with the specified diffuse and specular colors
    ///
    /// # Parameters
    ///
    /// * `name` - Optional name for the material
    /// * `diffuse_color` - RGBA diffuse color array `[r, g, b, a]`
    /// * `specular_color` - RGB specular color array `[r, g, b]`
    /// * `glossiness` - Glossiness value between 0.0 (rough) and 1.0 (smooth)
    ///
    /// # Returns
    ///
    /// The index of the created material in the glTF document
    pub fn create_specular_material(
        &mut self,
        name: Option<String>,
        diffuse_color: [f32; 4],
        specular_color: [f32; 3],
        glossiness: f32,
    ) -> usize {
        self.add_specular_glossiness_material(
            name,
            Some(diffuse_color),
            None,
            Some(specular_color),
            Some(glossiness),
            None,
            None,
            None,
            None,
            None,
            Some("OPAQUE".to_string()),
            None,
            Some(false),
        )
    }
    
    /// Add a material directly to the glTF document
    ///
    /// This is an internal helper method used by the material creation methods.
    fn add_material_direct(&mut self, material: crate::models::Material) -> usize {
        if let Some(materials) = &mut self.gltf.materials {
            let index = materials.len();
            materials.push(material);
            index
        } else {
            self.gltf.materials = Some(vec![material]);
            0
        }
    }
}
