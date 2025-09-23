//! # KTX File Format Support
//!
//! ## References
//!
//! - [Official KTX 1.1 Spec](https://registry.khronos.org/KTX/specs/1.0/ktxspec.v1.html)
//! - [Official KTX 2.0 Spec](https://registry.khronos.org/KTX/specs/2.0/ktxspec.v2.html)

use crate::{GPUTexture, LoadTextureError};
use nom::{IResult, Parser};
use std::io::{Read, Seek, SeekFrom};

/// First 12 bytes of a KTX 1.1 file
const KTX1_MAGIC_NUM: [u8; 12] = *b"\xABKTX 11\xBB\r\n\x1A\n";

/// First 12 bytes of a KTX 2.0 file
const KTX2_MAGIC_NUM: [u8; 12] = *b"\xABKTX 20\xBB\r\n\x1A\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Endianness {
    Little,
    Big,
}

impl TryFrom<[u8; 4]> for Endianness {
    type Error = LoadTextureError;

    fn try_from(value: [u8; 4]) -> Result<Self, Self::Error> {
        match value {
            [0x01, 0x02, 0x03, 0x04] => Ok(Endianness::Little),
            [0x04, 0x03, 0x02, 0x01] => Ok(Endianness::Big),
            _ => Err(LoadTextureError::FileFormatNotSupported),
        }
    }
}

impl Endianness {
    /// Reads a native-endian u32 from the input stream using the specified endianness
    #[inline]
    fn read_u32(&self, input: &mut impl Read) -> Result<u32, LoadTextureError> {
        let mut buf = [0; 4];
        input.read_exact(&mut buf).map_err(LoadTextureError::IO)?;
        match self {
            Endianness::Little => Ok(u32::from_le_bytes(buf)),
            Endianness::Big => Ok(u32::from_be_bytes(buf)),
        }
    }

    /// Reads a native-endian u16 from the input stream using the specified endianness
    #[inline]
    fn read_u16(&self, input: &mut impl Read) -> Result<u16, LoadTextureError> {
        let mut buf = [0; 2];
        input.read_exact(&mut buf).map_err(LoadTextureError::IO)?;
        match self {
            Endianness::Little => Ok(u16::from_le_bytes(buf)),
            Endianness::Big => Ok(u16::from_be_bytes(buf)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KTXv1Header {
    endianness: Endianness,

    gl_type: u32,
    gl_type_size: u32,
    gl_format: u32,
    gl_internal_format: u32,
    gl_base_internal_format: u32,
    pixel_width: u32,
    pixel_height: u32,
    pixel_depth: u32,
    array_elem_count: usize,
    /// Number of faces (1 for non-cubemaps, 6 for cubemaps)
    face_count: usize,
    /// Number of mipmap levels (including top-level image)
    ///
    /// If set to `0`, the texture is a single image, and mipmaps must be generated at load time
    mipmap_count: usize,
    /// Size (in bytes) of all data contained in the key-value block
    kv_block_size: usize,
}

impl KTXv1Header {
    const STRUCT_SIZE: usize = 64;

    fn parse<R: Read + Seek>(input: &mut R) -> Result<Self, LoadTextureError> {
        // Assume that the identifier has already been checked, and we're starting at 0x12
        let endianness = {
            let mut buf = [0u8; 4];
            input.read_exact(&mut buf).map_err(LoadTextureError::IO)?;
            Endianness::try_from(buf)?
        };

        let gl_type = endianness.read_u32(input)?;
        let gl_type_size = endianness.read_u32(input)?;
        let gl_format = endianness.read_u32(input)?;
        let gl_internal_format = endianness.read_u32(input)?;
        let gl_base_internal_format = endianness.read_u32(input)?;
        let pixel_width = endianness.read_u32(input)?;
        let pixel_height = endianness.read_u32(input)?;
        let pixel_depth = endianness.read_u32(input)?;
        let array_element_count = endianness.read_u32(input)?;
        let face_count = endianness.read_u32(input)?;
        let mipmap_count = endianness.read_u32(input)?;
        let kv_block_size = endianness.read_u32(input)?;

        Ok(Self {
            endianness,
            gl_type,
            gl_type_size,
            gl_format,
            gl_internal_format,
            gl_base_internal_format,
            pixel_width,
            pixel_height,
            pixel_depth,
            array_elem_count: array_element_count as usize,
            face_count: face_count as usize,
            mipmap_count: mipmap_count as usize,
            kv_block_size: kv_block_size as usize,
        })
    }
}

struct KTXv1File {
    header: KTXv1Header,
    kv_pairs: Option<Vec<KvPair>>,
    texture: GPUTexture,
}

impl KTXv1File {
    fn parse<R: Read + Seek>(input: &mut R) -> Result<Self, LoadTextureError> {
        let header = KTXv1Header::parse(input)?;
        let kv_pairs = KvPair::parse_all(input, header.kv_block_size, header.endianness)?;
        let texture = KTXv1File::parse_gpu_texture(input, &header)?;

        Ok(Self {
            header,
            kv_pairs,
            texture,
        })
    }

    fn parse_gpu_texture(
        input: &mut impl Read,
        header: &KTXv1Header,
    ) -> Result<GPUTexture, LoadTextureError> {
        let num_mip_levels = if header.mipmap_count == 0 {
            1
        } else {
            header.mipmap_count
        };
        let num_array_elements = if header.array_elem_count == 0 {
            1
        } else {
            header.array_elem_count
        };
        let num_faces = if header.face_count == 0 {
            1
        } else {
            header.face_count
        };

        todo!()
    }
}

/// Key-Value data pair for v1 and v2 files
struct KvPair {
    /// UTF-8 string key
    key: String,
    /// Arbitrary data bytes; likely to be a NUL-terminated UTF-8 string
    value: Vec<u8>,
}

impl KvPair {
    fn parse_all<R: Read + Seek>(
        input: &mut R,
        block_size: usize,
        endianness: Endianness,
    ) -> Result<Option<Vec<Self>>, LoadTextureError> {
        if block_size == 0 {
            return Ok(None);
        }

        let mut total_read = 0;

        // Load all raw kv pairs before parsing
        let mut raw_kvs = Vec::new();
        while total_read < block_size {
            // Get the size of the kv pair and calc padding_size
            let kv_size = endianness.read_u32(input)? as usize;
            let padding_size = 3 - ((kv_size + 3) % 4);
            total_read += 4;

            // Read the raw kv pair bytes
            let mut kv = vec![0u8; kv_size];
            input.read_exact(&mut kv).map_err(LoadTextureError::IO)?;
            total_read += kv_size + padding_size;

            raw_kvs.push(kv);

            // Move cursor to account for padding
            input
                .seek(SeekFrom::Current(padding_size as i64))
                .map_err(LoadTextureError::IO)?;
        }

        let parsed_kvs: Vec<Self> = raw_kvs
            .into_iter()
            .filter_map(|raw_kv| {
                let null_pos = raw_kv.iter().position(|&x| x == 0)?;
                Some(Self {
                    key: String::from_utf8_lossy(&raw_kv[..null_pos]).to_string(),
                    value: raw_kv[null_pos + 1..].to_vec(),
                })
            })
            .collect();

        if parsed_kvs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(parsed_kvs))
        }
    }
}

enum KTX {
    /// KTX Ver. 1.x
    V1,
    /// KTX Ver. 2.x
    V2,
}

#[cfg(test)]
mod tests {
    use super::*;
}
