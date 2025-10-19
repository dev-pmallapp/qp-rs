//! Minimal host-side utilities for decoding QS HDLC frames.
//!
//! The original QSPY tool listens on a TCP socket and consumes QS frames
//! produced by embedded targets. This crate implements the core HDLC deframer
//! and checksum verification so additional frontends (CLI, GUI, loggers) can
//! be layered on top in Rust.

mod decoder;
mod interpreter;

pub use interpreter::FrameInterpreter;

pub use decoder::{DecodeError, HdlcDecoder, QsFrame};

#[cfg(test)]
mod tests;
