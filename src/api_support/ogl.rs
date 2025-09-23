//! # OpenGL Support
//!
//! This module provides OpenGL-specific functionality for GPU textures, including texture format conversions.
//!
//! ## Important Details
//!
//! - This module **does not** depend on nor makes any assumptions about the OpenGL version used at runtime.
//! - This module **does** assume that raw OpenGL is the target library. Wrapper libraries, such as glium, often use
//!   their own wrapping data types and are likely to be incompatible with this module.

use crate::api_support::{GraphicsAPI, UnsupportedTextureFormatError};
use crate::{CompressedTextureFormat, TextureFormat};
use std::fmt::Display;

const GRAPHICS_API: GraphicsAPI = GraphicsAPI::OpenGL;

/// OpenGL texture format values
///
/// # Example
///
/// ```ignore
/// use gpu_texture::GPUTexture;
///
/// let texture = GPUTexture::load_from_file("compressed_tex.dds").unwrap();
///
/// /*
///    This line converts a `gpu_texture::TextureFormat` into a u32 so that
/// */
/// let ogl_format = texture.format.try_into_ogl_enum().unwrap();
///
/// unsafe {
///     // The standard texture setup code goes before this
///
///     gl::CompressedTexImage2D(
///         gl::Texture2D,
///         0,
///         texture.format.try_into_ogl_enum().unwrap(), // <-
///         texture.main_image.width as i32,
///         texture.main_image.height as i32,
///         0,
///         texture.main_image.img_buffer.len() as i32,
///         texture.main_image.img_buffer.as_ptr() as *const _
///     );
/// }
/// ```
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OGLTextureFormat {
    CompressedRgbDXT1 = 0x83F0,
    CompressedRgbaDXT1 = 0x83F1,
    CompressedRgbaDXT3 = 0x83F2,
    CompressedRgbaDXT5 = 0x83F3,
}

impl Display for OGLTextureFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let gl_name = match self {
            Self::CompressedRgbDXT1 => "GL_COMPRESSED_RGB_S3TC_DXT1_EXT",
            Self::CompressedRgbaDXT1 => "GL_COMPRESSED_RGBA_S3TC_DXT1_EXT",
            Self::CompressedRgbaDXT3 => "GL_COMPRESSED_RGBA_S3TC_DXT3_EXT",
            Self::CompressedRgbaDXT5 => "GL_COMPRESSED_RGBA_S3TC_DXT5_EXT",
        };

        write!(f, "{gl_name}")
    }
}

impl OGLTextureFormat {
    /// Casts `self` into a u32
    pub const fn into_u32(self) -> u32 {
        self as u32
    }
}

impl From<OGLTextureFormat> for u32 {
    fn from(format: OGLTextureFormat) -> u32 {
        format.into_u32()
    }
}

impl TryFrom<TextureFormat> for OGLTextureFormat {
    type Error = UnsupportedTextureFormatError;

    fn try_from(fmt: TextureFormat) -> Result<Self, Self::Error> {
        fmt.try_into_ogl_texture_format()
    }
}

impl TextureFormat {
    /// Tries to convert `self` into an `OGLTextureFormat` enum
    pub const fn try_into_ogl_texture_format(
        self,
    ) -> Result<OGLTextureFormat, UnsupportedTextureFormatError> {
        match self {
            TextureFormat::Compressed(format) => match format {
                CompressedTextureFormat::BC1 => Ok(OGLTextureFormat::CompressedRgbDXT1),
                CompressedTextureFormat::BC1A => Ok(OGLTextureFormat::CompressedRgbaDXT1),
                CompressedTextureFormat::BC2 => Ok(OGLTextureFormat::CompressedRgbaDXT3),
                CompressedTextureFormat::BC3 => Ok(OGLTextureFormat::CompressedRgbaDXT5),
            },
            _ => Err(UnsupportedTextureFormatError {
                format: self,
                api: GRAPHICS_API,
            }),
        }
    }

    /// Tries to convert `self` into a `u32` OpenGL texture format enum
    pub const fn try_into_ogl_enum(self) -> Result<u32, UnsupportedTextureFormatError> {
        match self.try_into_ogl_texture_format() {
            Ok(fmt) => Ok(fmt.into_u32()),
            Err(e) => Err(e),
        }
    }
}
