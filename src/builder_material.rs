use crate::builder::GltfBuilder;
use crate::material;

impl GltfBuilder {
    /// Add a customizable PBR material to the glTF document
    /// 
    /// This function allows you to create a fully customizable PBR (Physically Based Rendering) material
    /// with control over base color, metallic factor, roughness factor, and double-sided rendering.
    /// 
    /// # Parameters
    /// 
    /// * `name` - Optional name for the material
    /// * `base_color` - Optional RGBA color array `[r, g, b, a]` where each component is in range 0.0-1.0
    /// * `metallic_factor` - Optional metallic property (0.0 = non-metal, 1.0 = metal)
    /// * `roughness_factor` - Optional roughness property (0.0 = smooth, 1.0 = rough)
    /// * `double_sided` - Optional flag to enable double-sided rendering
    /// 
    /// # Returns
    /// 
    /// The index of the created material in the glTF document
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use mesh_tools::GltfBuilder;
    /// 
    /// let mut builder = GltfBuilder::new();
    /// 
    /// // Create a shiny blue metal material
    /// let blue_metal = builder.add_material(
    ///     Some("Blue Metal".to_string()),
    ///     Some([0.1, 0.2, 0.8, 1.0]),  // Blue color
    ///     Some(0.9),                    // Highly metallic
    ///     Some(0.2),                    // Low roughness (shiny)
    ///     Some(false)                   // Single-sided
    /// );
    /// 
    /// // Create a rough plastic material
    /// let red_plastic = builder.add_material(
    ///     Some("Red Plastic".to_string()),
    ///     Some([0.8, 0.1, 0.1, 1.0]),  // Red color
    ///     Some(0.0),                    // Non-metallic
    ///     Some(0.8),                    // High roughness
    ///     Some(true)                    // Double-sided
    /// );
    /// 
    /// // Create a simple material with defaults for some properties
    /// let green_material = builder.add_material(
    ///     Some("Green".to_string()),
    ///     Some([0.1, 0.8, 0.1, 1.0]),  // Green color
    ///     None,                         // Default metallic factor
    ///     None,                         // Default roughness factor
    ///     None                          // Default single-sided
    /// );
    /// 
    /// // Use the material with a mesh
    /// let cube = builder.create_box_with_material(1.0, Some(blue_metal));
    /// ```
    pub fn add_material(&mut self, name: Option<String>, 
                        base_color: Option<[f32; 4]>,
                        metallic_factor: Option<f32>,
                        roughness_factor: Option<f32>,
                        double_sided: Option<bool>) -> usize {
        let mut builder = material::MaterialBuilder::new(name);
        
        if let Some(color) = base_color {
            builder = builder.with_base_color(color);
        }
        
        if let Some(metallic) = metallic_factor {
            builder = builder.with_metallic_factor(metallic);
        }
        
        if let Some(roughness) = roughness_factor {
            builder = builder.with_roughness_factor(roughness);
        }
        
        if let Some(double_sided) = double_sided {
            builder = builder.with_double_sided(double_sided);
        }
        
        let material = builder.build();
        
        if let Some(materials) = &mut self.gltf.materials {
            let index = materials.len();
            materials.push(material);
            index
        } else {
            self.gltf.materials = Some(vec![material]);
            0
        }
    }

    /// Add a finished [`Material`](crate::Material) after checking it against the document.
    ///
    /// Checks the context-free rules of `MaterialBuilder::try_build` plus that every texture
    /// slot references an existing texture. The material is not stored on error.
    ///
    /// # Errors
    /// `GltfError::InvalidMaterial` with a `MATERIAL_*` code (for example
    /// `MATERIAL_TEXTURE_INVALID`). The document is unchanged on error.
    ///
    /// ```
    /// use mesh_tools::{material::MaterialBuilder, GltfBuilder, GltfError};
    /// let mut b = GltfBuilder::new();
    /// // No textures exist yet, so texture 0 is a dangling reference.
    /// let m = MaterialBuilder::new(None).with_base_color_texture(0).build();
    /// let err = b.add_material_from(m).unwrap_err();
    /// assert!(matches!(err, GltfError::InvalidMaterial { code: "MATERIAL_TEXTURE_INVALID", .. }));
    /// assert_eq!(b.gltf.materials.as_ref().map_or(0, Vec::len), 0);
    ///
    /// let ok = b.add_material_from(MaterialBuilder::new(None).with_metallic_factor(0.5).build()).unwrap();
    /// assert_eq!(ok, 0);
    /// ```
    pub fn add_material_from(&mut self, material: crate::models::Material) -> crate::error::Result<usize> {
        let textures = self.gltf.textures.as_ref().map_or(0, |t| t.len());
        let index = self.gltf.materials.as_ref().map_or(0, |m| m.len());
        if let Some(i) = material::check_material(&material, Some(textures)).into_iter().find(|i| !i.warning) {
            return Err(crate::error::GltfError::InvalidMaterial { index: Some(index), code: i.code, message: i.msg });
        }
        self.gltf.materials.get_or_insert_with(Vec::new).push(material);
        Ok(index)
    }

    /// Create a basic material with the specified color
    pub fn create_basic_material(&mut self, name: Option<String>, color: [f32; 4]) -> usize {
        let material = material::create_basic_material(name, color);
        
        if let Some(materials) = &mut self.gltf.materials {
            let index = materials.len();
            materials.push(material);
            index
        } else {
            self.gltf.materials = Some(vec![material]);
            0
        }
    }

    /// Create a metallic material
    pub fn create_metallic_material(&mut self, name: Option<String>, 
                                   color: [f32; 4], 
                                   metallic: f32,
                                   roughness: f32) -> usize {
        let material = material::create_metallic_material(name, color, metallic, roughness);
        
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
