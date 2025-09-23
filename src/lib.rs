#[cfg(feature = "ogl")] // TODO: Use the `any` selector once support for more APIs has been added.
pub mod api_support;
#[cfg(feature = "dds")]
pub mod file_fmt;

use file_fmt::dds;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

/// Supported compressed texture formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressedTextureFormat {
    /// BC1/DXT1
    BC1,
    /// BC1/DXT1 with 1-bit alpha channel
    BC1A,
    /// DXT2/3 RGBA
    BC2,
    /// DXT4/5 RGBA
    BC3,
}

/// Supported texture formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    RGB8,
    RGBA8,
    /// Compressed texture formats
    Compressed(CompressedTextureFormat),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub img_buffer: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GPUTexture {
    /// Texture data format
    pub format: TextureFormat,
    /// Top-level texture
    pub main_image: TextureData,
    /// Optional Texture mip maps
    pub mip_maps: Option<Vec<TextureData>>,
}

impl GPUTexture {
    /// Loads a [GPUTexture] from a supported file at the given path
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<GPUTexture, LoadTextureError> {
        enum TextureContainer {
            /// DirectDraw Surface container format
            Dds,
            /// Khronos texture container format
            Ktx,
        }

        // Determine the file type hint from extension and open file handle.
        let type_hint = path.as_ref()
            .extension()
            .and_then(OsStr::to_str)
            .map(str::to_ascii_lowercase)
            .and_then(|ext| match ext.as_str() {
                "dds" => Some(TextureContainer::Dds),
                "ktx" => Some(TextureContainer::Ktx),
                _ => None,
            });

        // Determine the appropriate file type loader
        let loader_fn = if let Some(type_hint) = type_hint {
            match type_hint {
                TextureContainer::Dds => dds::DDSLoader::load::<BufReader<File>>,
                TextureContainer::Ktx => {
                    todo!("KTX loader not yet implemented");
                }
            }
        } else {
            return Err(LoadTextureError::FileFormatNotSupported);
        };

        let mut tex_file_buf = BufReader::new(File::open(path).map_err(LoadTextureError::IO)?);

        loader_fn(&mut tex_file_buf)
    }

    /// Returns the total number of all textures loaded (1 + mip levels)
    pub fn texture_count(&self) -> usize {
        if let Some(mip_maps) = self.mip_maps.as_ref() {
            mip_maps.len() + 1
        } else {
            1
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum NeededBytes {
    Size(usize),
    Unknown,
}

impl From<nom::Needed> for NeededBytes {
    fn from(value: nom::Needed) -> Self {
        match value {
            nom::Needed::Unknown => NeededBytes::Unknown,
            nom::Needed::Size(size) => NeededBytes::Size(size.into()),
        }
    }
}

impl std::fmt::Display for NeededBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NeededBytes::Size(size) => write!(f, "{size} bytes"),
            NeededBytes::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug)]
pub enum LoadTextureError {
    FileFormatNotSupported,
    TextureFormatNotSupported(&'static [TextureFormat]),
    Incomplete(NeededBytes),
    InvalidData(nom::error::ErrorKind),
    InvalidSize { expected: usize, actual: usize },
    IO(std::io::Error),
}

impl std::fmt::Display for LoadTextureError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::FileFormatNotSupported => write!(f, "Provided file format is not supported"),
            Self::TextureFormatNotSupported(supported) => {
                writeln!(
                    f,
                    "The texture format used in the provided file is not supported"
                )?;
                write!(f, "Format must be one of: {:#?}", supported)
            }
            Self::Incomplete(needed) => write!(f, "File incomplete: at least {needed:#?}"),
            Self::InvalidData(kind) => write!(f, "Invalid file data: {kind:#?}"),
            Self::InvalidSize { expected, actual } => {
                write!(f, "Invalid size {actual} bytes, expected {expected} bytes")
            }
            Self::IO(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for LoadTextureError {}

/// Trait used
pub(crate) trait Loader {
    /// Texture formats supported by the target file type
    const SUPPORTED_FORMATS: &'static [TextureFormat];

    /// File extensions used to identify the target file type
    const FILE_EXTENSION: &'static str;

    /// Returns `true` if the provided data meets the minimum requirements to describe the expected file type.
    ///
    /// **Note**: Passing this check does not guarantee that the data will successfully parse.
    fn is_file_type<R: Read + Seek>(input: &mut R) -> std::io::Result<bool>;

    /// Loads the provided data into a GPUTexture.
    fn load<R: Read + Seek>(input: &mut R) -> Result<GPUTexture, LoadTextureError>;
}

/// All texture file extensions (in lowercase) supported by the library features selected at build time
pub const SUPPORTED_FILE_EXTENSIONS: &[&str] = &[
    #[cfg(feature = "dds")]
    dds::DDSLoader::FILE_EXTENSION,
    // #[cfg(feature = "ktx")]
    // ktx::KTXLoader::FILE_EXTENSION,
];
