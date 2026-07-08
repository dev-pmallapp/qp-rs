const FLAG: u8 = 0x7E;
const ESC: u8 = 0x7D;
const ESC_XOR: u8 = 0x20;

/// Represents a fully decoded QS frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QsFrame {
    /// Sequence counter maintained by the emitter.
    pub seq: u8,
    /// Record identifier (QS record type).
    pub record_type: u8,
    /// Raw payload bytes (timestamp + record data as emitted by the target).
    pub payload: Vec<u8>,
}

/// Errors produced while decoding QS HDLC frames.
#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// A candidate frame was shorter than the minimum valid length.
    FrameTooShort(usize),
    /// The trailing checksum did not match the computed value.
    InvalidChecksum { expected: u8, found: u8 },
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FrameTooShort(len) => write!(f, "frame too short (len={})", len),
            Self::InvalidChecksum { expected, found } => write!(
                f,
                "checksum mismatch: expected {:#04x}, found {:#04x}",
                expected, found
            ),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Incremental HDLC decoder that accepts arbitrary byte chunks and yields
/// verified QS frames.
#[derive(Debug, Default)]
pub struct HdlcDecoder {
    buffer: Vec<u8>,
    escape_next: bool,
}

impl HdlcDecoder {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            escape_next: false,
        }
    }

    /// Clears any partial frame state.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.escape_next = false;
    }

    /// Feeds raw bytes into the decoder and returns the outcome of every
    /// frame boundary crossed by this call, in order.
    ///
    /// Each frame is independently delimited by `FLAG` bytes, so a single
    /// corrupt frame (e.g. a checksum mismatch from a dropped byte on the
    /// wire) does not desync the rest of the stream: it is reported as an
    /// `Err` in place and decoding continues with the next frame, instead of
    /// aborting the whole call and silently dropping every frame still left
    /// in `input`.
    pub fn push_bytes(&mut self, input: &[u8]) -> Vec<Result<QsFrame, DecodeError>> {
        let mut results = Vec::new();

        for &byte in input {
            if byte == FLAG {
                // A FLAG always terminates framing; a dangling escape from a
                // corrupt prior frame must not bleed into the next one.
                self.escape_next = false;
                if !self.buffer.is_empty() {
                    let frame_bytes = std::mem::take(&mut self.buffer);
                    results.push(Self::decode_frame(&frame_bytes));
                }
                continue;
            }

            if self.escape_next {
                self.buffer.push(byte ^ ESC_XOR);
                self.escape_next = false;
                continue;
            }

            if byte == ESC {
                self.escape_next = true;
            } else {
                self.buffer.push(byte);
            }
        }

        results
    }

    fn decode_frame(data: &[u8]) -> Result<QsFrame, DecodeError> {
        if data.len() < 3 {
            return Err(DecodeError::FrameTooShort(data.len()));
        }

        let payload = &data[..data.len() - 1];
        let checksum = data[data.len() - 1];

        let mut sum: u8 = 0;
        for byte in payload {
            sum = sum.wrapping_add(*byte);
        }
        let expected = !sum;

        if checksum != expected {
            return Err(DecodeError::InvalidChecksum {
                expected,
                found: checksum,
            });
        }

        let seq = payload[0];
        let record_type = payload[1];
        let content = payload[2..].to_vec();

        Ok(QsFrame {
            seq,
            record_type,
            payload: content,
        })
    }
}
