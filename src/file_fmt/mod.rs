//! # File Format
//!
//! File formats supported by GPUTexture

use nom::IResult;
use nom::number::complete::le_u32;
use crate::{LoadTextureError, NeededBytes};

#[cfg(feature = "dds")]
pub mod dds;
mod ktx;

/// Parses a little-endian formatted u32 value from the provided input data and creates the matching
/// bitflag struct from it.
///
/// # Example
///
/// ```ignore
/// use bitflags::bitflags;
///
/// use crate::util::parse_le_u32_flags;
///
/// bitflags! {
///     #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
///     struct FooBarFlags: u32 {
///         const EnableFoo = 1;
///         const EnableBar = 1 << 1;
///
///         const EnableFooBar = Self::EnableFoo.bits() | Self::EnableBar.bits();
///     }
/// }
///
/// impl TryFrom<&[u8]> for FooBarFlags {
///     type Error = &'static str;
///
///     fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
///         let (_, flags) = parse_le_u32_flags(value).map_err(|_| "Parse error")?;
///         Ok(flags)
///     }
/// }
/// ```
pub(crate) fn parse_le_u32_flags<F>(input: &[u8]) -> IResult<&[u8], F>
where
    F: bitflags::Flags<Bits = u32> + Default,
{
    let (rem, value) = le_u32(input)?;
    let flags = F::from_bits(value).unwrap_or_default();
    Ok((rem, flags))
}

pub(crate) fn map_read_error(read_err: std::io::Error, expected_size: usize) -> LoadTextureError {
    match read_err.kind() {
        std::io::ErrorKind::UnexpectedEof => {
            LoadTextureError::Incomplete(NeededBytes::Size(expected_size))
        }
        _ => LoadTextureError::IO(read_err),
    }
}