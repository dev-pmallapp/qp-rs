#![no_std]
#![forbid(unsafe_code)]

//! # QP Framework (QF)
//! 
//! The Framework layer provides active objects, event queues, and lifecycle management
//! for building concurrent, event-driven embedded systems.
//!
//! Active objects are encapsulated, event-driven concurrent objects that communicate
//! through asynchronous message passing. Each active object has its own event queue
//! and executes in its own thread of control.

pub mod active;
pub mod queue;
pub mod registry;
pub mod lifecycle;

pub use qp_core::*;
pub use active::*;
pub use queue::*;
pub use registry::*;
pub use lifecycle::*;

#[cfg(test)]
mod tests;

/// Maximum number of active objects in the system
pub const MAX_ACTIVE_OBJECTS: usize = 32;

/// Default event queue capacity for active objects
pub const DEFAULT_QUEUE_CAPACITY: usize = 8;
