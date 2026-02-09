//! Platform abstraction for synchronization primitives.
//!
//! Provides unified `Mutex` and `Arc` types that work in both `std` and `no_std`
//! environments. With the `std` feature enabled, uses standard library types.
//! Without it, uses `spin::Mutex` for locking.

#[cfg(not(feature = "std"))]
pub use alloc::sync::Arc;
#[cfg(feature = "std")]
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
    pub fn new(value: T) -> Self {
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
            self.inner.lock().expect("mutex poisoned")
        }
        #[cfg(not(feature = "std"))]
        {
            self.inner.lock()
        }
    }
}
