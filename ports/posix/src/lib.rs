//! POSIX-specific utilities for hosting the qf framework.
//!
//! The original QP/C++ codebase provides a reference POSIX port. This crate
//! gradually absorbs Rust equivalents, starting with helper utilities for
//! tracing and runtime configuration.

use std::io;
use std::net::ToSocketAddrs;

use qf::{QsConfig, TraceError, TraceHook, Tracer, TracerHandle};
use qs::predefined::{self, TargetInfo};
use qs::{stdout_backend, TcpBackend, UdpBackend, WriterBackend};

enum BackendHandle {
    Stdout(TracerHandle<WriterBackend<std::io::Stdout>>),
    Tcp(TracerHandle<TcpBackend>),
    Udp(TracerHandle<UdpBackend>),
}

/// Convenience wrapper that owns a QS tracer and exposes a trace hook.
pub struct PosixPort {
    backend: BackendHandle,
}

impl PosixPort {
    /// Creates a new POSIX port instance that streams QS records to stdout.
    pub fn new() -> Self {
        let handle = Tracer::new(QsConfig::default(), stdout_backend()).into_handle();
        Self {
            backend: BackendHandle::Stdout(handle),
        }
    }

    /// Connects to a remote qspy listener over TCP.
    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let backend = TcpBackend::connect(addr)?;
        let handle = Tracer::new(QsConfig::default(), backend).into_handle();
        Ok(Self {
            backend: BackendHandle::Tcp(handle),
        })
    }

    /// Connects to a remote qspy listener over UDP.
    pub fn connect_udp<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let backend = UdpBackend::connect(addr)?;
        let handle = Tracer::new(QsConfig::default(), backend).into_handle();
        Ok(Self {
            backend: BackendHandle::Udp(handle),
        })
    }

    /// Returns the QS trace hook to be passed into the kernel.
    pub fn trace_hook(&self) -> TraceHook {
        match &self.backend {
            BackendHandle::Stdout(handle) => handle.hook(),
            BackendHandle::Tcp(handle) => handle.hook(),
            BackendHandle::Udp(handle) => handle.hook(),
        }
    }

    pub fn emit_record(
        &self,
        record_type: u8,
        payload: &[u8],
        with_timestamp: bool,
    ) -> Result<(), TraceError> {
        match &self.backend {
            BackendHandle::Stdout(handle) => {
                handle.emit_with_flag(record_type, payload, with_timestamp)
            }
            BackendHandle::Tcp(handle) => {
                handle.emit_with_flag(record_type, payload, with_timestamp)
            }
            BackendHandle::Udp(handle) => {
                handle.emit_with_flag(record_type, payload, with_timestamp)
            }
        }
    }

    pub fn emit_dictionary(&self, record_type: u8, payload: &[u8]) -> Result<(), TraceError> {
        self.emit_record(record_type, payload, false)
    }

    pub fn emit_target_info(&self, info: &TargetInfo) -> Result<(), TraceError> {
        let payload = predefined::target_info_payload(info);
        self.emit_dictionary(predefined::TARGET_INFO, &payload)
    }

    pub fn emit_obj_dict(&self, address: u64, name: &str) -> Result<(), TraceError> {
        let payload = predefined::obj_dict_payload(address, name);
        self.emit_dictionary(predefined::OBJ_DICT, &payload)
    }

    pub fn emit_fun_dict(&self, address: u64, name: &str) -> Result<(), TraceError> {
        let payload = predefined::fun_dict_payload(address, name);
        self.emit_dictionary(predefined::FUN_DICT, &payload)
    }

    pub fn emit_usr_dict(&self, record_id: u8, name: &str) -> Result<(), TraceError> {
        let payload = predefined::usr_dict_payload(record_id, name);
        self.emit_dictionary(predefined::USR_DICT, &payload)
    }

    pub fn emit_sig_dict(&self, signal: u16, object: u64, name: &str) -> Result<(), TraceError> {
        let payload = predefined::sig_dict_payload(signal, object, name);
        self.emit_dictionary(predefined::SIG_DICT, &payload)
    }
}
