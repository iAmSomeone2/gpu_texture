//! # API Support
//!
//! Optional features and modules to help support various graphics APIs such as OpenGL and Vulkan

use crate::TextureFormat;
use std::error::Error;
use std::fmt::Display;

#[cfg(feature = "ogl")]
pub mod ogl;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum GraphicsAPI {
    OpenGL,
    Vulkan,
    WebGPU,
}

impl Display for GraphicsAPI {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenGL => write!(f, "OpenGL"),
            Self::Vulkan => write!(f, "Vulkan"),
            Self::WebGPU => write!(f, "WebGPU"),
        }
    }
}

/// Error used when a texture format is not supported by the graphics API
#[derive(Debug, Copy, Clone)]
pub struct UnsupportedTextureFormatError {
    format: TextureFormat,
    api: GraphicsAPI,
}

impl Display for UnsupportedTextureFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Texture format \"{:?}\" is not supported by {}",
            self.format, self.api
        )
    }
}

impl Error for UnsupportedTextureFormatError {}
