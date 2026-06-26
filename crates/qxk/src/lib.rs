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
// Functional safety (docs/FUSA.md, Phase 4): the extended-kernel layer is
// memory-safe by construction — all unsafe lives below it in `qf`.
// Traceability: ASR-006 (memory-safe language subset / trusted elements).
#![forbid(unsafe_code)]

// Heap-free `static-alloc` build links no allocator (see qf `lib.rs`): `alloc`
// is pulled in only off the `static-alloc` path or when `std` is present (host
// tests). Any stray heap use on the heap-free path is then a hard compile error
// — the forcing function that keeps the safety build allocation-free.
#[cfg(any(not(feature = "static-alloc"), feature = "std"))]
extern crate alloc;

pub mod kernel;
pub mod primitives;
pub mod scheduler;
mod sync;
pub mod thread;

/// Maximum number of extended threads (heap-free registry/ready-queue bound).
pub const MAX_THREADS: usize = 32;
/// Maximum number of threads that can wait on a single primitive (heap-free
/// wait-list bound).
pub const MAX_WAITERS: usize = 16;
/// Highest supported active-object priority (priority 0 is reserved for idle).
pub const MAX_AO_PRIORITY: usize = 63;

pub use kernel::{QxkKernel, QxkKernelBuilder, QxkKernelError};
pub use primitives::{CondVar, MessageQueue, MutexPrim, Semaphore, SyncError, SyncResult};
pub use scheduler::{QxkScheduler, SchedStatus, ScheduleMode};
pub use thread::{ExtendedThread, ThreadAction, ThreadConfig, ThreadId, ThreadPriority, ThreadState};
#[cfg(any(not(feature = "static-alloc"), feature = "std"))]
pub use thread::thread_handler;
