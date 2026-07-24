pub(crate) mod cursor;
pub mod commands;
mod decoder;
pub mod frontend;
mod interpreter;
pub mod output;
mod runtime;
mod sizes;

pub use commands::{CommandSender, SharedSender, try_send};
pub use decoder::{DecodeError, HdlcDecoder, QsFrame};
pub use interpreter::{FrameInterpreter, UserRecordFormatter};
pub use output::{OutputSinks, stdout_is_tty};
pub use runtime::{run, run_with_custom_handler, CustomCommandHandler};
pub use sizes::TargetSizes;

#[cfg(test)]
mod tests;
