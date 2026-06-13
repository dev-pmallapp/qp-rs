//! Generic qspy console binary.
//!
//! All runtime logic lives in the `qspy` library ([`qspy::run`]). This binary
//! registers no record formatters, so it stays domain-agnostic — downstream
//! crates that want project-specific record rendering build their own thin
//! binary that registers formatters via
//! [`qspy::FrameInterpreter::add_user_formatter`] before calling `run`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    qspy::run(|_interpreter| {})
}
