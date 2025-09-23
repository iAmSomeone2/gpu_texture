//! # KTX File Format Support

/// First 12 bytes of a KTX 1.1 file
const KTX1_MAGIC_NUM: [u8; 12] = *b"\xABKTX 11\xBB\r\n\x1A\n";

/// First 12 bytes of a KTX 2.0 file
const KTX2_MAGIC_NUM: [u8; 12] = *b"\xABKTX 20\xBB\r\n\x1A\n";

enum KTXVersion {
    KTX1,
    KTX2,
}

#[cfg(test)]
mod tests {
    use super::*;
}