//! DirectDraw Surface file handling

use crate::util::{map_read_error, parse_le_u32_flags};
use crate::{
    CompressedTextureFormat, GPUTexture, LoadTextureError, Loader, TextureData, TextureFormat,
};
use std::io::{Read, Seek};

use bitflags::bitflags;
use nom::IResult;
use nom::error::make_error;
use nom::number::complete::{be_u32, le_u32};

/// First 4 bytes of a DDS file
const MAGIC_NUM: u32 = u32::from_be_bytes(*b"DDS ");
// const MAGIC_NUM: u32 = 0x44445320;

bitflags! {
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct HeaderFlags: u32 {
        const Caps = 0x1;
        const Height = 0x2;
        const Width = 0x4;
        const Pitch = 0x8;
        const PixelFormat = 0x1000;
        const MipMapCount = 0x20000;
        const LinearSize = 0x80000;
        const Depth = 0x800000;

        const Texture = Self::Caps.bits() | Self::Height.bits() | Self::Width.bits() | Self::PixelFormat.bits();
        const MipMap = Self::MipMapCount.bits();
        const Volume = Self::Depth.bits();
    }

    /// Flags indicating what type of data is in the surface
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct PixelFmtFlags: u32 {
        /// Texture contains alpha data; dwRGBAlphaBitMask contains valid data.
        const AlphaPixels = 0x1;
        /// Used in some older DDS files for alpha channel only uncompressed data (dwRGBBitCount contains the alpha channel bit-count; dwABitMask contains valid data)
        const Alpha = 0x2;
        /// Texture contains compressed RGB data; dwFourCC contains valid data.
        const FourCC = 0x4;
        /// Texture contains uncompressed RGB data; dwRGBBitCount and the RGB masks (dwRBitMask, dwGBitMask, dwBBitMask) contain valid data.
        const RGB = 0x40;
        /// Used in some older DDS files for YUV uncompressed data (dwRGBBitCount contains the YUV bit count; dwRBitMask contains the Y mask, dwGBitMask contains the U mask, dwBBitMask contains the V mask)
        const YUV = 0x200;
        /// Used in some older DDS files for single channel color uncompressed data (dwRGBBitCount contains the luminance channel bit-count; dwRBitMask contains the channel mask). Can be combined with DDPF_ALPHAPIXELS for a two-channel DDS file.
        const Luminance = 0x20000;
    }

    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct Capabilities: u32 {
        const Complex = 0x8;
        const MipMap = 0x400000;
        const Texture = 0x1000;

        const CubeMap = 0x200;
        const CubeMapPositiveX = 0x400;
        const CubeMapNegativeX = 0x800;
        const CubeMapPositiveY = 0x1000;
        const CubeMapNegativeY = 0x2000;
        const CubeMapPositiveZ = 0x4000;
        const CubeMapNegativeZ = 0x8000;

        const Volume = 0x200000;
    }
}

impl HeaderFlags {
    fn parse(input: &[u8]) -> IResult<&[u8], Self> {
        parse_le_u32_flags(input)
    }
}

impl PixelFmtFlags {
    fn parse(input: &[u8]) -> IResult<&[u8], Self> {
        parse_le_u32_flags(input)
    }
}

impl Capabilities {
    fn parse(input: &[u8]) -> IResult<&[u8], Self> {
        parse_le_u32_flags(input)
    }
}

/// Four-character long code indicating the type of texture surface contained in the file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FourCharCode {
    DXT1,
    DXT2,
    DXT3,
    DXT4,
    DXT5,
    DX10,
}

impl FourCharCode {
    const fn into_u32(self) -> u32 {
        match self {
            FourCharCode::DXT1 => u32::from_be_bytes(*b"DXT1"),
            FourCharCode::DXT2 => u32::from_be_bytes(*b"DXT2"),
            FourCharCode::DXT3 => u32::from_be_bytes(*b"DXT3"),
            FourCharCode::DXT4 => u32::from_be_bytes(*b"DXT4"),
            FourCharCode::DXT5 => u32::from_be_bytes(*b"DXT5"),
            FourCharCode::DX10 => u32::from_be_bytes(*b"DX10"),
        }
    }
}

impl From<FourCharCode> for u32 {
    fn from(value: FourCharCode) -> Self {
        value.into_u32()
    }
}

impl TryFrom<u32> for FourCharCode {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == FourCharCode::DXT1.into_u32() {
            Ok(FourCharCode::DXT1)
        } else if value == FourCharCode::DXT2.into_u32() {
            Ok(FourCharCode::DXT2)
        } else if value == FourCharCode::DXT3.into_u32() {
            Ok(FourCharCode::DXT3)
        } else if value == FourCharCode::DXT4.into_u32() {
            Ok(FourCharCode::DXT4)
        } else if value == FourCharCode::DXT5.into_u32() {
            Ok(FourCharCode::DXT5)
        } else if value == FourCharCode::DX10.into_u32() {
            Ok(FourCharCode::DX10)
        } else {
            Err(())
        }
    }
}

impl FourCharCode {
    fn parse(input: &[u8]) -> IResult<&[u8], FourCharCode> {
        let conv_err = nom::Err::Error(make_error(input, nom::error::ErrorKind::IsNot));

        let (rem, value) = be_u32(input)?;
        let char_code: FourCharCode = value.try_into().or(Err(conv_err))?;

        Ok((rem, char_code))
    }
}

/// DirectDraw Surface pixel format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelFormat {
    struct_size: u32,
    flags: PixelFmtFlags,
    four_char_code: FourCharCode,
    rgb_bit_count: u32,
    red_mask: u32,
    green_mask: u32,
    blue_mask: u32,
    alpha_mask: u32,
}

impl TryFrom<PixelFormat> for TextureFormat {
    type Error = LoadTextureError;

    fn try_from(value: PixelFormat) -> Result<Self, Self::Error> {
        let fmt_err = LoadTextureError::TextureFormatNotSupported(DDSLoader::SUPPORTED_FORMATS);

        if value.flags.contains(PixelFmtFlags::FourCC) {
            // Image data uses a DXTn compression scheme
            match value.four_char_code {
                FourCharCode::DXT1 => {
                    if value.flags.contains(PixelFmtFlags::AlphaPixels) {
                        Ok(TextureFormat::Compressed(CompressedTextureFormat::BC1A))
                    } else {
                        Ok(TextureFormat::Compressed(CompressedTextureFormat::BC1))
                    }
                }
                FourCharCode::DXT2 | FourCharCode::DXT3 => {
                    Ok(TextureFormat::Compressed(CompressedTextureFormat::BC2))
                }
                FourCharCode::DXT4 | FourCharCode::DXT5 => {
                    Ok(TextureFormat::Compressed(CompressedTextureFormat::BC3))
                }
                _ => Err(fmt_err),
            }
        } else {
            Err(fmt_err)
        }
    }
}

impl PixelFormat {
    fn parse(input: &[u8]) -> IResult<&[u8], PixelFormat> {
        let (rem, struct_size) = le_u32(input)?;
        let (rem, flags) = PixelFmtFlags::parse(rem)?;
        let (rem, four_cc) = FourCharCode::parse(rem)?;
        let (rem, rgb_bit_count) = le_u32(rem)?;
        let (rem, red_mask) = le_u32(rem)?;
        let (rem, green_mask) = le_u32(rem)?;
        let (rem, blue_mask) = le_u32(rem)?;
        let (rem, alpha_mask) = le_u32(rem)?;

        Ok((
            rem,
            PixelFormat {
                struct_size,
                flags,
                four_char_code: four_cc,
                rgb_bit_count,
                red_mask,
                green_mask,
                blue_mask,
                alpha_mask,
            },
        ))
    }
}

/// Sizing scheme dependent on whether the texture is compressed or not
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextureSizing {
    /// The pitch or number of bytes per scan line in an uncompressed texture
    Pitch(u32),
    /// The total number of bytes in the top level texture for a compressed texture.
    Linear(u32),
}

impl TextureSizing {
    fn parse(input: &[u8], header_flags: HeaderFlags) -> IResult<&[u8], TextureSizing> {
        let (rem, value) = le_u32(input)?;
        let sizing = if header_flags.contains(HeaderFlags::Pitch) {
            TextureSizing::Pitch(value)
        } else if header_flags.contains(HeaderFlags::LinearSize) {
            TextureSizing::Linear(value)
        } else {
            return Err(nom::Err::Error(make_error(
                input,
                nom::error::ErrorKind::Fail,
            )));
        };

        Ok((rem, sizing))
    }
}

/// DDS file header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DDSHeader {
    /// Size of the file's header structure
    struct_size: u32,
    /// Flags indicating which header members contain valid data
    flags: HeaderFlags,
    /// Texture height (in pixels)
    height: u32,
    /// Texture width (in pixels)
    width: u32,
    /// Sizing scheme dependent on whether the texture is compressed or not
    texture_sizing: TextureSizing,
    /// Depth of a volume texture (in pixels)
    depth: Option<u32>,
    /// Number of mip maps levels
    mip_map_count: Option<u32>,
    pixel_format: PixelFormat,
    capabilities: Capabilities,
}

impl DDSHeader {
    const STRUCT_SIZE: usize = 124;

    fn parse<'p>(input: &'p [u8]) -> IResult<&'p [u8], Self> {
        let parse_optional = |input: &'p [u8],
                              check_flag: HeaderFlags,
                              header_flags: HeaderFlags|
         -> IResult<&'p [u8], Option<u32>> {
            // Parse u32 value regardless of flag since the headers are always the same size
            let (rem, value) = le_u32(input)?;
            let value = if header_flags.contains(check_flag) {
                Some(value)
            } else {
                None
            };

            Ok((rem, value))
        };

        let (rem, struct_size) = le_u32(input)?;
        let (rem, flags) = HeaderFlags::parse(rem)?;
        let (rem, height) = le_u32(rem)?;
        let (rem, width) = le_u32(rem)?;
        let (rem, texture_sizing) = TextureSizing::parse(rem, flags)?;
        let (rem, depth) = parse_optional(rem, HeaderFlags::Depth, flags)?;
        let (rem, mip_map_count) = parse_optional(rem, HeaderFlags::MipMapCount, flags)?;

        // Skip past the 44-byte reserved section
        let rem = &rem[44..];

        let (rem, pixel_format) = PixelFormat::parse(rem)?;

        // Parse the first two dwCaps sections and skip the last two
        let parse_caps_section =
            |input: &'p [u8], mut caps_acc: Capabilities| -> IResult<&'p [u8], Capabilities> {
                let (rem, caps) = Capabilities::parse(input)?;
                caps_acc |= caps;
                Ok((rem, caps_acc))
            };

        let (rem, caps) = parse_caps_section(rem, Capabilities::default())?;
        let (rem, caps) = parse_caps_section(rem, caps)?;

        // Skip the last two sections (8 bytes) plus the final reserved DWORD (4 bytes) -- total 12 bytes
        let rem = &rem[12..];

        let header = Self {
            struct_size,
            flags,
            height,
            width,
            texture_sizing,
            depth,
            mip_map_count,
            pixel_format,
            capabilities: caps,
        };

        Ok((rem, header))
    }

    const MISSING_FEATURE_TXT: &'static str =
        "Uncompressed DDS textures are not currently supported";

    fn get_next_mip(
        &self,
        data: &mut impl Read,
        current_layer_size: usize,
        level: usize,
    ) -> Result<TextureData, LoadTextureError> {
        let is_square = self.width == self.height;
        let min_block_bytes: usize = match self.pixel_format.four_char_code {
            FourCharCode::DXT1 => 8,
            _ => 16,
        };

        let width = self.width >> level;
        let height = self.height >> level;

        let layer_size = if is_square {
            current_layer_size / 4
        } else {
            std::cmp::max(1, (width as usize).div_ceil(4))
                * std::cmp::max(1, (height as usize).div_ceil(4))
                * min_block_bytes
        };
        let layer_size = std::cmp::max(layer_size, min_block_bytes);

        let mut img_buffer = vec![0; layer_size];
        data.read_exact(&mut img_buffer)
            .map_err(|read_err| map_read_error(read_err, layer_size))?;

        Ok(TextureData {
            img_buffer,
            width,
            height,
        })
    }

    fn get_tex_data(
        &self,
        input: &mut impl Read,
    ) -> Result<(TextureData, Option<Vec<TextureData>>), LoadTextureError> {
        if matches!(self.texture_sizing, TextureSizing::Pitch(..)) {
            todo!("{}", Self::MISSING_FEATURE_TXT);
        }

        // Number of mip maps -- not including top level texture -- in the DDS file
        let mip_count = (self.mip_map_count.unwrap_or(1) as usize) - 1;

        let mut current_layer_size: usize = match self.texture_sizing {
            TextureSizing::Linear(size) => size as usize,
            TextureSizing::Pitch(pitch) => (pitch * self.height) as usize,
        };

        // Determine the total required file size for the top-level texture and all mip maps
        let mut img_buffer = vec![0u8; current_layer_size];
        input
            .read_exact(&mut img_buffer)
            .map_err(|read_err| map_read_error(read_err, current_layer_size))?;

        let main_texture = TextureData {
            img_buffer,
            width: self.width,
            height: self.height,
        };

        if mip_count == 0 {
            return Ok((main_texture, None));
        }

        let mut mip_maps: Vec<TextureData> = Vec::with_capacity(mip_count);

        // Calculate the mip map values
        for level in 0..mip_count {
            let mip_map = self.get_next_mip(input, current_layer_size, level + 1)?;
            current_layer_size = mip_map.img_buffer.len();
            mip_maps.push(mip_map);
        }

        Ok((main_texture, Some(mip_maps)))
    }
}

/// Loader for DirectDraw Surface (DDS) files containing uncompressed or compressed texture data
pub struct DDSLoader;

impl DDSLoader {
    const MIN_FILE_SIZE: usize = 4 + DDSHeader::STRUCT_SIZE;
}

impl Loader for DDSLoader {
    const SUPPORTED_FORMATS: &'static [TextureFormat] = &[
        TextureFormat::Compressed(CompressedTextureFormat::BC1),
        TextureFormat::Compressed(CompressedTextureFormat::BC1A),
        TextureFormat::Compressed(CompressedTextureFormat::BC2),
        TextureFormat::Compressed(CompressedTextureFormat::BC3),
    ];

    const FILE_EXTENSION: &'static str = "dds";

    fn is_file_type<R: Read + Seek>(input: &mut R) -> std::io::Result<bool> {
        let mut buffer = [0; 4];
        input.read_exact(&mut buffer)?;
        input.rewind()?;

        let first_dword = u32::from_be_bytes(buffer);
        Ok(first_dword == MAGIC_NUM)
    }

    fn load<R: Read + Seek>(input: &mut R) -> Result<GPUTexture, LoadTextureError> {
        if !Self::is_file_type(input).map_err(LoadTextureError::IO)? {
            return Err(LoadTextureError::FileFormatNotSupported);
        }

        input
            .seek_relative(4)
            .map_err(|read_err| map_read_error(read_err, Self::MIN_FILE_SIZE))?;

        let mut header_buf = [0; DDSHeader::STRUCT_SIZE];
        input
            .read_exact(&mut header_buf)
            .map_err(|read_err| map_read_error(read_err, Self::MIN_FILE_SIZE))?;

        let (rem, header) = DDSHeader::parse(&header_buf).map_err(|err| match err {
            nom::Err::Error(e) | nom::Err::Failure(e) => LoadTextureError::InvalidData(e.code),
            nom::Err::Incomplete(needed) => LoadTextureError::Incomplete(needed.into()),
        })?;
        debug_assert!(rem.is_empty());

        let (main_image, mip_maps) = header.get_tex_data(input)?;

        let format = header.pixel_format.try_into()?;
        Ok(GPUTexture {
            format,
            main_image,
            mip_maps,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::BufReader;
    use std::path::PathBuf;
    use std::sync::LazyLock;

    const ASSETS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets");

    static TEST_DDS_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
        PathBuf::from(ASSETS_DIR)
            .join("awesomeface.dds")
            .canonicalize()
            .unwrap()
    });

    #[test]
    fn four_char_code_from_u32() {
        let test_cases = [
            (0x44585431u32, Ok(FourCharCode::DXT1)),
            (0x44585432, Ok(FourCharCode::DXT2)),
            (0x44585433, Ok(FourCharCode::DXT3)),
            (0x44585434, Ok(FourCharCode::DXT4)),
            (0x44585435, Ok(FourCharCode::DXT5)),
            (0x44583130, Ok(FourCharCode::DX10)),
            (0xAABBCCDD, Err(())),
        ];

        for (input, expected) in test_cases {
            assert_eq!(FourCharCode::try_from(input), expected);
        }
    }

    #[test]
    fn parse_dds_header() {
        let test_data = std::fs::read(TEST_DDS_PATH.as_path()).unwrap();

        let parse_header_result = DDSHeader::parse(&test_data[4..]);
        assert!(parse_header_result.is_ok());
        let (_rem, header) = parse_header_result.unwrap();
        println!("{:?}", header);
    }

    #[test]
    fn load_dds_file() {
        let mut test_data = BufReader::new(std::fs::File::open(TEST_DDS_PATH.as_path()).unwrap());

        let load_dds_result = DDSLoader::load(&mut test_data);
        assert!(load_dds_result.is_ok());

        let texture = load_dds_result.unwrap();
        assert_eq!(
            texture.format,
            TextureFormat::Compressed(CompressedTextureFormat::BC3)
        );
    }
}
