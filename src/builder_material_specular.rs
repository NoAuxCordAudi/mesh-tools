use crate::builder::GltfBuilder;
use crate::error::Result;
use crate::material::{AlphaMode, MaterialBuilder, SpecularGlossiness};

impl GltfBuilder {
    /// Add a material using the specular-glossiness workflow (KHR_materials_pbrSpecularGlossiness).
    ///
    /// Deprecated: build the material with [`MaterialBuilder::with_specular_glossiness`]
    /// (per-slot `texCoord`, optional metallic-roughness fallback) and add it with
    /// [`add_material_from`](GltfBuilder::add_material_from). All textures here use `TEXCOORD_0`.
    ///
    /// # Errors
    /// `GltfError::InvalidMaterial` (`SG_FACTOR_RANGE`, `SG_TEXTURE_INVALID`,
    /// `MATERIAL_ALPHA_MODE_INVALID`, ...). The document is unchanged on error.
    ///
    /// ```
    /// # #![allow(deprecated)]
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// let ok = b.add_specular_glossiness_material(
    ///     None, Some([1.0, 0.0, 0.0, 1.0]), None, Some([0.5; 3]), Some(0.5),
    ///     None, None, None, None, None, None, None, None,
    /// );
    /// assert_eq!(ok.unwrap(), 0);
    /// let bad = b.add_specular_glossiness_material(
    ///     None, None, None, None, Some(2.0), None, None, None, None, None, None, None, None,
    /// );
    /// assert!(bad.is_err());
    /// ```
    #[deprecated(since = "0.13.0", note = "use `MaterialBuilder::with_specular_glossiness` and `add_material_from`")]
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
    ) -> Result<usize> {
        let mut builder = MaterialBuilder::new(name).with_specular_glossiness(SpecularGlossiness {
            diffuse_factor,
            diffuse_texture: diffuse_texture.map(Into::into),
            specular_factor,
            glossiness_factor,
            specular_glossiness_texture: specular_glossiness_texture.map(Into::into),
        });
        if let Some(t) = normal_texture {
            builder = builder.with_normal_texture(t, 1.0);
        }
        if let Some(t) = occlusion_texture {
            builder = builder.with_occlusion_texture(t, 1.0);
        }
        if let Some(t) = emissive_texture {
            builder = builder.with_emissive_texture(t);
        }
        let mut material = builder.build();
        material.emissive_factor = emissive_factor;
        // Passed through as given; add_material_from rejects invalid values.
        material.alpha_mode = alpha_mode;
        material.alpha_cutoff = alpha_cutoff;
        material.double_sided = double_sided;
        self.add_material_from(material)
    }

    /// Create a simple specular material with the specified diffuse and specular colors
    ///
    /// # Errors
    /// `GltfError::InvalidMaterial` (`SG_FACTOR_RANGE`) if a color or the glossiness is outside
    /// [0, 1]. The document is unchanged on error.
    ///
    /// ```
    /// use mesh_tools::GltfBuilder;
    /// let mut b = GltfBuilder::new();
    /// let m = b.create_specular_material(None, [0.5, 0.25, 0.125, 1.0], [0.5; 3], 0.25).unwrap();
    /// assert_eq!(m, 0);
    /// assert!(b.create_specular_material(None, [1.0; 4], [0.5; 3], 1.5).is_err());
    /// ```
    pub fn create_specular_material(
        &mut self,
        name: Option<String>,
        diffuse_color: [f32; 4],
        specular_color: [f32; 3],
        glossiness: f32,
    ) -> Result<usize> {
        let material = MaterialBuilder::new(name)
            .with_specular_glossiness(SpecularGlossiness {
                diffuse_factor: Some(diffuse_color),
                specular_factor: Some(specular_color),
                glossiness_factor: Some(glossiness),
                ..Default::default()
            })
            .with_alpha(AlphaMode::Opaque)
            .with_double_sided(false)
            .build();
        self.add_material_from(material)
    }
}
