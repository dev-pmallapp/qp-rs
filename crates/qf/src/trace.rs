#[cfg(feature = "qs")]
pub use qs::{TraceError, TraceHook};

#[cfg(feature = "qs")]
pub type TraceResult = Result<(), TraceError>;

#[cfg(not(feature = "qs"))]
use alloc::sync::Arc;

#[cfg(not(feature = "qs"))]
pub type TraceError = core::convert::Infallible;

#[cfg(not(feature = "qs"))]
pub type TraceResult = Result<(), TraceError>;

#[cfg(not(feature = "qs"))]
pub type TraceHook = Arc<dyn Fn(u8, &[u8], bool) -> TraceResult + Send + Sync>;
