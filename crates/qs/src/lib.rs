//! QS-compatible tracing (SRS §4.3) with pluggable backends.
//!
//! The Quantum Spy (QS) protocol transports binary *records* framed in HDLC
//! packets. We implement just enough of the encoder side so that existing host
//! tools (for example [`qpspy`](https://www.state-machine.com/qtools/qpspy.html))
//! can interpret the stream.

use std::io::{self, Write};
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use thiserror::Error;

mod record;

pub mod predefined;
pub mod records;

pub use predefined::TargetInfo;
pub use record::{
    make_format, UserRecordBuilder, FMT_F32, FMT_F64, FMT_FUN, FMT_HEX, FMT_I16, FMT_I32, FMT_I64,
    FMT_I8_ENUM, FMT_MEM, FMT_OBJ, FMT_SIG, FMT_STR, FMT_U16, FMT_U32, FMT_U64, FMT_U8,
};

/// Maximum payload length for a single record (excluding header/checksum).
const DEFAULT_MAX_RECORD_LEN: usize = 64;

/// Configuration for the tracer.
#[derive(Debug, Clone)]
pub struct QsConfig {
    pub max_record_len: usize,
    pub include_timestamp: bool,
}

impl Default for QsConfig {
    fn default() -> Self {
        Self {
            max_record_len: DEFAULT_MAX_RECORD_LEN,
            include_timestamp: true,
        }
    }
}

/// A single QS record.
#[derive(Debug, Clone)]
pub struct QsRecord {
    pub seq: u8,
    pub record_type: u8,
    pub timestamp: Option<Duration>,
    pub payload: Vec<u8>,
}

/// Errors that can occur while emitting QS data.
#[derive(Error, Debug)]
pub enum TraceError {
    #[error("payload too large: {0} bytes")]
    PayloadTooLarge(usize),
    #[error("backend error: {0}")]
    Backend(#[from] io::Error),
}

/// Backend trait that consumes HDLC framed bytes.
pub trait TraceBackend: Send + Sync {
    fn write_frame(&self, frame: &[u8]) -> Result<(), TraceError>;
}

/// Simple backend that writes frames to any `Write` implementation.
pub struct WriterBackend<W: Write + Send + Sync + 'static> {
    writer: Arc<Mutex<W>>,
}

impl<W: Write + Send + Sync + 'static> WriterBackend<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Arc::new(Mutex::new(writer)),
        }
    }
}

impl<W: Write + Send + Sync + 'static> TraceBackend for WriterBackend<W> {
    fn write_frame(&self, frame: &[u8]) -> Result<(), TraceError> {
        let mut guard = self.writer.lock().unwrap();
        guard.write_all(frame).map_err(TraceError::from)
    }
}

/// QS frame encoder.
#[derive(Debug)]
pub struct Tracer<B: TraceBackend> {
    backend: B,
    cfg: QsConfig,
    seq: u8,
    epoch: SystemTime,
}

#[derive(Clone)]
pub struct TracerHandle<B: TraceBackend> {
    inner: Arc<Mutex<Tracer<B>>>,
}

impl<B: TraceBackend> Tracer<B> {
    pub fn new(cfg: QsConfig, backend: B) -> Self {
        Self {
            backend,
            cfg,
            seq: 0,
            epoch: SystemTime::now(),
        }
    }

    pub fn into_handle(self) -> TracerHandle<B> {
        TracerHandle {
            inner: Arc::new(Mutex::new(self)),
        }
    }

    pub fn record(
        &mut self,
        record_type: u8,
        payload: &[u8],
        with_timestamp: bool,
    ) -> Result<QsRecord, TraceError> {
        if payload.len() > self.cfg.max_record_len {
            return Err(TraceError::PayloadTooLarge(payload.len()));
        }

        let timestamp = if self.cfg.include_timestamp && with_timestamp {
            Some(self.epoch.elapsed().unwrap_or_default())
        } else {
            None
        };

        self.seq = self.seq.wrapping_add(1);
        #[cfg(debug_assertions)]
        {
            println!("QS TX record_type={record_type} len={}", payload.len());
        }
        let record = QsRecord {
            seq: self.seq,
            record_type,
            timestamp,
            payload: payload.to_vec(),
        };

        let frame = self.build_frame(&record);
        self.backend.write_frame(&frame)?;
        Ok(record)
    }

    fn build_frame(&self, record: &QsRecord) -> Vec<u8> {
        const FLAG: u8 = 0x7E;
        const ESC: u8 = 0x7D;
        const ESC_XOR: u8 = 0x20;

        let mut bytes = Vec::with_capacity(record.payload.len() + 8);
        let mut checksum: u8 = 0;

        let push_escaped = |dest: &mut Vec<u8>, sum: &mut u8, byte: u8| {
            *sum = sum.wrapping_add(byte);
            if byte == FLAG || byte == ESC {
                dest.push(ESC);
                dest.push(byte ^ ESC_XOR);
            } else {
                dest.push(byte);
            }
        };

        let push_literal = |dest: &mut Vec<u8>, byte: u8| {
            if byte == FLAG || byte == ESC {
                dest.push(ESC);
                dest.push(byte ^ ESC_XOR);
            } else {
                dest.push(byte);
            }
        };

        push_escaped(&mut bytes, &mut checksum, record.seq);
        push_escaped(&mut bytes, &mut checksum, record.record_type);

        if let Some(ts) = record.timestamp {
            let ticks = (ts.as_micros() as u32).to_le_bytes();
            for byte in ticks {
                push_escaped(&mut bytes, &mut checksum, byte);
            }
        }

        for &byte in &record.payload {
            push_escaped(&mut bytes, &mut checksum, byte);
        }

        let checksum_byte = !checksum;
        push_literal(&mut bytes, checksum_byte);

        bytes.push(FLAG);
        bytes
    }
}

impl<B: TraceBackend + 'static> TracerHandle<B> {
    pub fn emit(&self, record_type: u8, payload: &[u8]) -> Result<QsRecord, TraceError> {
        self.emit_internal(record_type, payload, false)
    }

    pub fn emit_with_timestamp(
        &self,
        record_type: u8,
        payload: &[u8],
    ) -> Result<QsRecord, TraceError> {
        self.emit_internal(record_type, payload, true)
    }

    pub fn emit_with_flag(
        &self,
        record_type: u8,
        payload: &[u8],
        with_timestamp: bool,
    ) -> Result<(), TraceError> {
        self.emit_internal(record_type, payload, with_timestamp)
            .map(|_| ())
    }

    fn emit_internal(
        &self,
        record_type: u8,
        payload: &[u8],
        with_timestamp: bool,
    ) -> Result<QsRecord, TraceError> {
        let mut guard = self.inner.lock().unwrap();
        guard.record(record_type, payload, with_timestamp)
    }

    pub fn hook(&self) -> TraceHook {
        let inner = Arc::clone(&self.inner);
        Arc::new(move |record_type, payload, with_timestamp| {
            let mut guard = inner.lock().unwrap();
            guard
                .record(record_type, payload, with_timestamp)
                .map(|_| ())
        })
    }
}

pub type TraceHook = Arc<dyn Fn(u8, &[u8], bool) -> Result<(), TraceError> + Send + Sync>;

/// Convenience backend that writes frames to stdout; handy for early bring-up.
pub fn stdout_backend() -> WriterBackend<io::Stdout> {
    WriterBackend::new(io::stdout())
}

/// Backend that streams QS frames over a TCP connection.
pub struct TcpBackend {
    stream: Arc<Mutex<TcpStream>>,
}

impl TcpBackend {
    /// Establishes a TCP connection to the provided socket address.
    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true).ok();
        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
        })
    }
}

impl TraceBackend for TcpBackend {
    fn write_frame(&self, frame: &[u8]) -> Result<(), TraceError> {
        let mut guard = self.stream.lock().unwrap();
        guard.write_all(frame).map_err(TraceError::from)
    }
}

/// Backend that streams QS frames over a UDP socket.
pub struct UdpBackend {
    socket: Arc<Mutex<UdpSocket>>,
}

impl UdpBackend {
    /// Binds a local UDP socket and connects it to the provided remote address.
    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect(addr)?;
        Ok(Self {
            socket: Arc::new(Mutex::new(socket)),
        })
    }
}

impl TraceBackend for UdpBackend {
    fn write_frame(&self, frame: &[u8]) -> Result<(), TraceError> {
        let guard = self.socket.lock().unwrap();
        guard.send(frame).map(|_| ()).map_err(TraceError::from)
    }
}
