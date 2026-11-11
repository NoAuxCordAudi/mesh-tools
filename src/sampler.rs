//! Typed texture sampler settings (glTF 2.0 `sampler`).
//!
//! The enums map to the numeric OpenGL codes of the spec; [`SamplerDesc`] groups the four
//! optional fields. `None` omits the key, so readers apply the spec defaults: filtering is up
//! to the client and wrapping is `REPEAT`.

use crate::constants::{sampler_filter as f, sampler_wrap as w};
use crate::models::Sampler;

macro_rules! code_enum {
    ($(#[$m:meta])* $name:ident { $($var:ident = $code:expr),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($var),+ }

        impl $name {
            /// The numeric glTF code written to the file.
            pub fn code(self) -> usize {
                match self { $($name::$var => $code),+ }
            }

            /// The value for a glTF code, or `None` if the code is not valid here.
            pub fn from_code(code: usize) -> Option<Self> {
                match code { $(c if c == $code => Some($name::$var),)+ _ => None }
            }
        }
    };
}

code_enum! {
    /// Magnification filter (`magFilter`): only `NEAREST` and `LINEAR` are valid.
    MagFilter { Nearest = f::NEAREST, Linear = f::LINEAR }
}

code_enum! {
    /// Minification filter (`minFilter`), including the four mipmap modes.
    MinFilter {
        Nearest = f::NEAREST,
        Linear = f::LINEAR,
        NearestMipmapNearest = f::NEAREST_MIPMAP_NEAREST,
        LinearMipmapNearest = f::LINEAR_MIPMAP_NEAREST,
        NearestMipmapLinear = f::NEAREST_MIPMAP_LINEAR,
        LinearMipmapLinear = f::LINEAR_MIPMAP_LINEAR,
    }
}

code_enum! {
    /// Texture wrapping (`wrapS`, `wrapT`).
    WrapMode {
        Repeat = w::REPEAT,
        ClampToEdge = w::CLAMP_TO_EDGE,
        MirroredRepeat = w::MIRRORED_REPEAT,
    }
}

/// Sampler settings. `None` leaves the key out of the file (spec defaults apply).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SamplerDesc {
    pub mag_filter: Option<MagFilter>,
    pub min_filter: Option<MinFilter>,
    pub wrap_s: Option<WrapMode>,
    pub wrap_t: Option<WrapMode>,
}

impl SamplerDesc {
    /// The sampler shared by the `create_*texture*` helpers: linear magnification, trilinear
    /// minification and repeat wrapping.
    pub fn default_sampler() -> Self {
        SamplerDesc {
            mag_filter: Some(MagFilter::Linear),
            min_filter: Some(MinFilter::LinearMipmapLinear),
            wrap_s: Some(WrapMode::Repeat),
            wrap_t: Some(WrapMode::Repeat),
        }
    }

    pub(crate) fn to_model(self) -> Sampler {
        Sampler {
            mag_filter: self.mag_filter.map(MagFilter::code),
            min_filter: self.min_filter.map(MinFilter::code),
            wrap_s: self.wrap_s.map(WrapMode::code),
            wrap_t: self.wrap_t.map(WrapMode::code),
            ..Default::default()
        }
    }

    /// True if `s` is exactly this description: same fields, no extensions, extras or other data.
    pub(crate) fn matches(&self, s: &Sampler) -> bool {
        s.name.is_none()
            && s.extensions.is_none()
            && s.extras.is_none()
            && s.mag_filter == self.mag_filter.map(MagFilter::code)
            && s.min_filter == self.min_filter.map(MinFilter::code)
            && s.wrap_s == self.wrap_s.map(WrapMode::code)
            && s.wrap_t == self.wrap_t.map(WrapMode::code)
    }
}
