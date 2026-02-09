#[cfg(not(feature = "std"))]
pub use alloc::sync::Arc;
#[cfg(feature = "std")]
pub use std::sync::Arc;

#[cfg(feature = "std")]
pub type MutexGuard<'a, T> = std::sync::MutexGuard<'a, T>;
#[cfg(not(feature = "std"))]
pub type MutexGuard<'a, T> = spin::MutexGuard<'a, T>;

pub struct Mutex<T> {
    #[cfg(feature = "std")]
    inner: std::sync::Mutex<T>,
    #[cfg(not(feature = "std"))]
    inner: spin::Mutex<T>,
}

impl<T> Mutex<T> {
    pub fn new(value: T) -> Self {
        Self {
            #[cfg(feature = "std")]
            inner: std::sync::Mutex::new(value),
            #[cfg(not(feature = "std"))]
            inner: spin::Mutex::new(value),
        }
    }

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
