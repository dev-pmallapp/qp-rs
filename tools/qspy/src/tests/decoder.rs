use std::sync::{Arc, Mutex};

use qs::{QsConfig, TraceBackend, TraceError, Tracer};

use crate::{DecodeError, HdlcDecoder, QsFrame};

#[derive(Clone, Default)]
struct CaptureBackend {
    frames: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl TraceBackend for CaptureBackend {
    fn write_frame(&self, frame: &[u8]) -> Result<(), TraceError> {
        self.frames.lock().unwrap().push(frame.to_vec());
        Ok(())
    }
}

#[test]
fn decoder_roundtrip() {
    let backend = CaptureBackend::default();
    let mut tracer = Tracer::new(QsConfig::default(), backend.clone());

    tracer
        .record(0x42, &[0xDE, 0xAD, 0xBE, 0xEF], true)
        .unwrap();

    let frames = backend.frames.lock().unwrap();
    assert_eq!(frames.len(), 1);

    let mut decoder = HdlcDecoder::new();
    let decoded = decoder.push_bytes(&frames[0]).unwrap();

    assert_eq!(decoded.len(), 1);
    let QsFrame {
        seq,
        record_type,
        payload,
    } = &decoded[0];

    assert_eq!(*seq, 1);
    assert_eq!(*record_type, 0x42);
    // Timestamp is enabled by default, so payload contains 4-byte timestamp + data.
    assert!(payload.len() >= 4);
    assert_eq!(&payload[payload.len() - 4..], &[0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn detects_bad_checksum() {
    let mut decoder = HdlcDecoder::new();
    // Frame: seq=1, record=0x10, checksum wrong, FLAG terminator
    let frame = [0x01, 0x10, 0x00, 0x7E];
    let err = decoder.push_bytes(&frame).unwrap_err();
    assert!(matches!(err, DecodeError::InvalidChecksum { .. }));
}
