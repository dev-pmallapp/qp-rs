//! Platform abstraction for synchronization primitives.
//!
//! Provides unified `Mutex` and `Arc` types that work in both `std` and `no_std`
//! environments. With the `std` feature enabled, uses standard library types.
//! Without it, uses `spin::Mutex` for locking.

// The heap-free `static-alloc` build links no allocator, so `alloc::sync::Arc`
// is unavailable there; framework code on that path uses `&'static` handles
// instead (see `docs/FUSA.md`, Phase 2). `std` builds always re-export `Arc`
// (host tests still use it).
#[cfg(all(not(feature = "std"), not(feature = "static-alloc")))]
pub use alloc::sync::Arc;
// On the `static-alloc` lib build no framework code uses `Arc` (handles are
// `&'static`); it is still re-exported for `std` host tests, so suppress the
// expected unused-import lint there only.
#[cfg(feature = "std")]
#[cfg_attr(feature = "static-alloc", allow(unused_imports))]
pub use std::sync::Arc;

#[cfg(feature = "std")]
pub type MutexGuard<'a, T> = std::sync::MutexGuard<'a, T>;
#[cfg(not(feature = "std"))]
pub type MutexGuard<'a, T> = spin::MutexGuard<'a, T>;

/// Platform-agnostic mutex wrapper.
///
/// Uses `std::sync::Mutex` when the `std` feature is enabled, and `spin::Mutex`
/// for `no_std` environments. In `std` mode, panics if the mutex is poisoned,
/// as poisoning is not recoverable in real-time systems.
pub struct Mutex<T> {
    #[cfg(feature = "std")]
    inner: std::sync::Mutex<T>,
    #[cfg(not(feature = "std"))]
    inner: spin::Mutex<T>,
}

impl<T> Mutex<T> {
    /// Creates a new mutex protecting the given value.
    ///
    /// `const` so that mutex-protected primitives (e.g. the `static-alloc`
    /// event queues) can be placed in `static` storage with no runtime
    /// initialisation or heap.
    pub const fn new(value: T) -> Self {
        Self {
            #[cfg(feature = "std")]
            inner: std::sync::Mutex::new(value),
            #[cfg(not(feature = "std"))]
            inner: spin::Mutex::new(value),
        }
    }

    /// Acquires the mutex, blocking until it becomes available.
    ///
    /// # Panics
    ///
    /// In `std` mode, panics if the mutex has been poisoned by a panicking thread.
    /// This is intentional as poisoned state is not recoverable in real-time systems.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        #[cfg(feature = "std")]
        {
            // A poisoned mutex means a thread panicked inside the critical
            // section, so the protected state may be corrupt. Per the
            // crash-only model this is an unrecoverable functional-safety
            // fault — route it through the central fault handler.
            match self.inner.lock() {
                Ok(guard) => guard,
                Err(_) => crate::fusa::on_error(module_path!(), line!()),
            }
        }
        #[cfg(not(feature = "std"))]
        {
            self.inner.lock()
        }
    }
}
