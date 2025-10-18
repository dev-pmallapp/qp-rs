//! Helpers for constructing application-specific (USER) QS records.
//!
//! The official QP stream tags each field in a user record with a one-byte
//! *format descriptor* followed by the field payload encoded in little-endian
//! order. The [`UserRecordBuilder`] mirrors this layout so that payloads emitted
//! by the Rust port remain interoperable with the QSPY tooling.

/// Format identifier for `QS_I8_ENUM_FMT` records.
pub const FMT_I8_ENUM: u8 = 0x0;
/// Format identifier for `QS_U8_FMT` records.
pub const FMT_U8: u8 = 0x1;
/// Format identifier for `QS_I16_FMT` records.
pub const FMT_I16: u8 = 0x2;
/// Format identifier for `QS_U16_FMT` records.
pub const FMT_U16: u8 = 0x3;
/// Format identifier for `QS_I32_FMT` records.
pub const FMT_I32: u8 = 0x4;
/// Format identifier for `QS_U32_FMT` records.
pub const FMT_U32: u8 = 0x5;
/// Format identifier for `QS_F32_FMT` records.
pub const FMT_F32: u8 = 0x6;
/// Format identifier for `QS_F64_FMT` records.
pub const FMT_F64: u8 = 0x7;
/// Format identifier for `QS_STR_FMT` records.
pub const FMT_STR: u8 = 0x8;
/// Format identifier for `QS_MEM_FMT` records.
pub const FMT_MEM: u8 = 0x9;
/// Format identifier for `QS_SIG_FMT` records.
pub const FMT_SIG: u8 = 0xA;
/// Format identifier for `QS_OBJ_FMT` records.
pub const FMT_OBJ: u8 = 0xB;
/// Format identifier for `QS_FUN_FMT` records.
pub const FMT_FUN: u8 = 0xC;
/// Format identifier for `QS_I64_FMT` records.
pub const FMT_I64: u8 = 0xD;
/// Format identifier for `QS_U64_FMT` records.
pub const FMT_U64: u8 = 0xE;
/// Format identifier for the optional hexadecimal flag (`QS_HEX_FMT`).
pub const FMT_HEX: u8 = 0xF;

/// Computes a user-record format descriptor by combining a width hint with a
/// base format identifier.
pub fn make_format(width: u8, base: u8) -> u8 {
    ((width & 0x0F) << 4) | (base & 0x0F)
}

/// Incremental builder for QS user-record payloads.
#[derive(Debug, Default)]
pub struct UserRecordBuilder {
    bytes: Vec<u8>,
}

impl UserRecordBuilder {
    /// Creates an empty builder.
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    /// Creates a builder with reserved capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }

    /// Adds an unsigned 8-bit field using the provided width hint.
    pub fn push_u8(&mut self, width: u8, value: u8) -> &mut Self {
        self.bytes.push(make_format(width, FMT_U8));
        self.bytes.push(value);
        self
    }

    /// Adds an unsigned 16-bit field using the provided width hint.
    pub fn push_u16(&mut self, width: u8, value: u16) -> &mut Self {
        self.bytes.push(make_format(width, FMT_U16));
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Adds an unsigned 32-bit field using the provided width hint.
    pub fn push_u32(&mut self, width: u8, value: u32) -> &mut Self {
        self.bytes.push(make_format(width, FMT_U32));
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Adds an unsigned 64-bit field using the provided width hint.
    pub fn push_u64(&mut self, width: u8, value: u64) -> &mut Self {
        self.bytes.push(make_format(width, FMT_U64));
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Adds a raw memory blob (length limited to 255 bytes).
    pub fn push_mem(&mut self, data: &[u8]) -> &mut Self {
        let len = u8::try_from(data.len()).expect("QS MEM payloads must be <= 255 bytes");
        self.bytes.push(make_format(0, FMT_MEM));
        self.bytes.push(len);
        self.bytes.extend_from_slice(data);
        self
    }

    /// Adds a null-terminated ASCII string field.
    pub fn push_str(&mut self, value: &str) -> &mut Self {
        self.bytes.push(make_format(0, FMT_STR));
        self.bytes.extend_from_slice(value.as_bytes());
        self.bytes.push(0);
        self
    }

    /// Adds a pre-computed format descriptor alongside raw bytes.
    pub fn push_raw(&mut self, format: u8, bytes: &[u8]) -> &mut Self {
        self.bytes.push(format);
        self.bytes.extend_from_slice(bytes);
        self
    }

    /// Consumes the builder and returns the accumulated payload bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_u8_fields() {
        let mut builder = UserRecordBuilder::new();
        builder.push_u8(0, 0xAB).push_u8(1, 0xCD);
        assert_eq!(builder.into_vec(), vec![0x01, 0xAB, 0x11, 0xCD]);
    }

    #[test]
    fn builds_u16_field() {
        let mut builder = UserRecordBuilder::new();
        builder.push_u16(0, 0x1234);
        assert_eq!(builder.into_vec(), vec![0x03, 0x34, 0x12]);
    }

    #[test]
    fn builds_string_field() {
        let mut builder = UserRecordBuilder::new();
        builder.push_str("hi");
        assert_eq!(builder.into_vec(), vec![0x08, b'h', b'i', 0]);
    }
}
