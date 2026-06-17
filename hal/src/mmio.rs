//! Volatile memory-mapped register wrappers for *SIS family ports

use core::cell::UnsafeCell;

/// Read-Write register wrapper
#[repr(transparent)]
pub struct RW<T>(UnsafeCell<T>);

unsafe impl<T> Send for RW<T> where T: Send {}
unsafe impl<T> Sync for RW<T> where T: Sync {}

impl<T: Copy> RW<T> {
    /// Read the register value with volatile semantics
    #[inline(always)]
    pub fn read(&self) -> T {
        unsafe { core::ptr::read_volatile(self.0.get()) }
    }

    /// Write a new value to the register with volatile semantics
    #[inline(always)]
    pub fn write(&self, val: T) {
        unsafe { core::ptr::write_volatile(self.0.get(), val) }
    }

    /// Modify the register value atomically relative to the caller
    #[inline(always)]
    pub fn modify<F>(&self, f: F)
    where
        F: FnOnce(T) -> T,
    {
        let val = self.read();
        self.write(f(val));
    }
}

/// Read-Only register wrapper
#[repr(transparent)]
pub struct RO<T>(UnsafeCell<T>);

unsafe impl<T> Send for RO<T> where T: Send {}
unsafe impl<T> Sync for RO<T> where T: Sync {}

impl<T: Copy> RO<T> {
    /// Read the register value with volatile semantics
    #[inline(always)]
    pub fn read(&self) -> T {
        unsafe { core::ptr::read_volatile(self.0.get()) }
    }
}

/// Write-Only register wrapper
#[repr(transparent)]
pub struct WO<T>(UnsafeCell<T>);

unsafe impl<T> Send for WO<T> where T: Send {}
unsafe impl<T> Sync for WO<T> where T: Sync {}

impl<T: Copy> WO<T> {
    /// Write a new value to the register with volatile semantics
    #[inline(always)]
    pub fn write(&self, val: T) {
        unsafe { core::ptr::write_volatile(self.0.get(), val) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rw_volatile() {
        let val = RW(UnsafeCell::new(42u32));
        assert_eq!(val.read(), 42);
        val.write(100);
        assert_eq!(val.read(), 100);
        val.modify(|v| v + 5);
        assert_eq!(val.read(), 105);
    }

    #[test]
    fn test_ro_volatile() {
        let val = RO(UnsafeCell::new(55u32));
        assert_eq!(val.read(), 55);
    }

    #[test]
    fn test_wo_volatile() {
        let raw = UnsafeCell::new(0u32);
        let val = WO(raw);
        val.write(99);
        unsafe {
            assert_eq!(*val.0.get(), 99);
        }
    }
}
