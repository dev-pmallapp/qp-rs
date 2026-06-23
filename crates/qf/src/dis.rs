//! Duplicate Inverse Storage (DIS) — an error-detecting code for
//! safety-critical scalar state (see `docs/FUSA.md`, Phase 3).
//!
//! A [`Dis<T>`] keeps a value together with its bitwise complement. Every read
//! verifies that the two are still exact inverses of one another; a mismatch
//! means the storage was corrupted (e.g. a single-event upset / bit flip) and
//! is routed to [`crate::fusa::on_error`] — the crash-only response of the
//! functional-safety fault model.
//!
//! This is the QP/C `Q_DIS`-style redundancy applied to scalar fields such as
//! active-object priorities, queue indices, and pool free-list links.
//!
//! ```
//! use qf::dis::Dis;
//!
//! let mut prio = Dis::new(5u8);
//! assert_eq!(prio.get(), 5);   // verified read
//! prio.set(7);
//! assert_eq!(prio.get(), 7);
//! ```

use core::ops::Not;

mod sealed {
    pub trait Sealed {}
}

/// Integer-like scalars that [`Dis`] can protect.
///
/// Sealed: implemented only for the primitive integer types, whose bitwise
/// complement is a faithful, reversible redundant encoding.
pub trait DisInt: Copy + PartialEq + Not<Output = Self> + sealed::Sealed {}

macro_rules! impl_disint {
    ($($t:ty),+ $(,)?) => {$(
        impl sealed::Sealed for $t {}
        impl DisInt for $t {}
    )+};
}
impl_disint!(u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize);

/// A scalar stored together with its bitwise complement.
///
/// Reads verify the redundant copy and fault on corruption; writes keep both
/// halves consistent. `Copy`, so it is a drop-in replacement for a plain scalar
/// field.
#[derive(Clone, Copy)]
pub struct Dis<T: DisInt> {
    value: T,
    inverse: T,
}

impl<T: DisInt> Dis<T> {
    /// Wrap `value`, computing its redundant inverse.
    #[inline]
    pub fn new(value: T) -> Self {
        Self {
            value,
            inverse: !value,
        }
    }

    /// Read the protected value, verifying the redundant inverse first.
    ///
    /// Faults via [`crate::fusa::on_error`] (does not return) if the two halves
    /// disagree — i.e. the storage has been corrupted.
    #[inline]
    pub fn get(&self) -> T {
        if self.value != !self.inverse {
            crate::fusa::on_error(module_path!(), line!());
        }
        self.value
    }

    /// Overwrite the protected value, refreshing its inverse.
    #[inline]
    pub fn set(&mut self, value: T) {
        self.value = value;
        self.inverse = !value;
    }

    /// Non-faulting integrity check: `true` if the two halves are consistent.
    #[inline]
    pub fn is_intact(&self) -> bool {
        self.value == !self.inverse
    }

    /// Corrupt the redundant copy without updating the value — test only.
    #[cfg(test)]
    fn corrupt_for_test(&mut self) {
        self.inverse = !self.inverse;
    }
}

impl<T: DisInt + core::fmt::Debug> core::fmt::Debug for Dis<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Avoid faulting inside Debug; report integrity instead.
        f.debug_struct("Dis")
            .field("value", &self.value)
            .field("intact", &self.is_intact())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_update() {
        let mut d = Dis::new(0xA5u8);
        assert!(d.is_intact());
        assert_eq!(d.get(), 0xA5);
        d.set(0x00);
        assert_eq!(d.get(), 0x00);
        assert!(d.is_intact());

        let mut w = Dis::new(0xDEAD_BEEFu32);
        assert_eq!(w.get(), 0xDEAD_BEEF);
        w.set(0);
        assert_eq!(w.get(), 0);
    }

    #[test]
    fn corruption_is_detected_without_faulting() {
        let mut d = Dis::new(42u16);
        assert!(d.is_intact());
        d.corrupt_for_test();
        assert!(!d.is_intact());
    }

    #[cfg(feature = "std")]
    #[test]
    fn corrupted_read_faults() {
        let prev = std::panic::take_hook();
        std::panic::set_hook(std::boxed::Box::new(|_| {}));
        let mut d = Dis::new(7u8);
        d.corrupt_for_test();
        let caught = std::panic::catch_unwind(move || d.get());
        std::panic::set_hook(prev);
        assert!(caught.is_err(), "a corrupted DIS read must fault");
    }
}
