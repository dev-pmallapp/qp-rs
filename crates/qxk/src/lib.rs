//! # qxk - Dual-Mode Preemptive Kernel
//!
//! QXK is a dual-mode preemptive kernel combining:
//! - Event-driven active objects (run-to-completion, like QK)
//! - Extended threads (blocking, traditional RTOS threads)
//!
//! This allows mixing event-driven code with conventional blocking middleware
//! (TCP/IP stacks, file systems, etc.) in a single system.
//!
//! ## Architecture
//!
//! **Active Objects**: Non-blocking, event-driven state machines with event queues.
//! Scheduled preemptively with run-to-completion semantics.
//!
//! **Extended Threads**: Traditional blocking threads with their own stacks.
//! Can use blocking primitives like semaphores, mutexes, and message queues.
//!
//! **Dual-Mode Scheduling**: Active objects have priority over extended threads.
//! When no active objects are ready, extended threads execute based on priority.
//!
//! ## Module Overview
//!
//! - [`thread`] - Extended thread abstraction with stack management
//! - [`scheduler`] - Dual-mode scheduler for AOs and threads
//! - [`sync`] - Synchronization primitives (semaphores, mutexes)
//! - [`kernel`] - QXK kernel with builder pattern

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod kernel;
pub mod scheduler;
mod sync;
pub mod thread;

pub use kernel::{QxkKernel, QxkKernelBuilder, QxkKernelError};
pub use scheduler::{QxkScheduler, ScheduleMode};
pub use thread::{ExtendedThread, ThreadPriority, ThreadState};
