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
//! Traceability: ASR-004 (error-detecting codes); see `docs/traceability.md`.
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
use portable_atomic::{AtomicU32, Ordering};

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

impl Dis<usize> {
    /// `const` constructor for the `usize` specialisation, so a [`Dis`]-protected
    /// index can live in a `const fn` / `static` initialiser (e.g. a pool
    /// free-list head). The generic [`Dis::new`] cannot be `const` because the
    /// `Not` bound is not callable in a generic `const` context.
    #[inline]
    pub const fn new_usize(value: usize) -> Self {
        Self {
            value,
            inverse: !value,
        }
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

/// Duplicate Storage (non-inverted): keeps two identical copies of a value and
/// verifies they still agree on every read.
///
/// This is the companion to [`Dis`] for values that have **no meaningful
/// bitwise inverse** — function pointers (e.g. HSM state handlers) and opaque
/// links — where DIS's complement encoding does not apply. A mismatch between
/// the two copies signals corruption (bit flip / SEU) and is routed to
/// [`crate::fusa::on_error`]. This is the QP/C "Duplicate Storage" redundancy
/// (see `docs/FUSA.md`, Phase 3), as distinct from the inverse-encoded [`Dis`].
///
/// `Copy` when `T: Copy`, so it drops into a struct field in place of a plain
/// `T`.
#[derive(Clone, Copy)]
pub struct Dup<T: Copy + PartialEq> {
    a: T,
    b: T,
}

impl<T: Copy + PartialEq> Dup<T> {
    /// Wrap `value`, storing it in both copies.
    #[inline]
    pub fn new(value: T) -> Self {
        Self { a: value, b: value }
    }

    /// Read the protected value, verifying the redundant copy first.
    ///
    /// Faults via [`crate::fusa::on_error`] (does not return) if the two copies
    /// disagree — i.e. the storage has been corrupted.
    #[inline]
    pub fn get(&self) -> T {
        if self.a != self.b {
            crate::fusa::on_error(module_path!(), line!());
        }
        self.a
    }

    /// Overwrite both copies of the protected value.
    #[inline]
    pub fn set(&mut self, value: T) {
        self.a = value;
        self.b = value;
    }

    /// Non-faulting integrity check: `true` if the two copies are consistent.
    #[inline]
    pub fn is_intact(&self) -> bool {
        self.a == self.b
    }

    /// Corrupt the redundant copy without updating the value — test only.
    #[cfg(test)]
    fn corrupt_with(&mut self, other: T) {
        self.b = other;
    }
}

/// A reference count protected by Duplicate Inverse Storage, in a single
/// atomic word so the value and its inverse always update **atomically as a
/// pair** (a plain `Dis<T>` over two atomics could be observed mid-update).
///
/// Layout: the low 16 bits hold the count, the high 16 bits hold its bitwise
/// complement. Every operation verifies the two halves and routes a mismatch
/// to [`crate::fusa::on_error`]; increment faults on overflow and decrement
/// faults on underflow — giving the refcount a built-in **double-free /
/// corruption detector**.
///
/// The count is 16-bit (max 65 535 simultaneous references), which is ample
/// for event payloads. Like `Arc`, this requires a target with atomic
/// compare-exchange.
pub struct DisAtomicU16 {
    packed: AtomicU32,
}

impl DisAtomicU16 {
    #[inline]
    const fn pack(v: u16) -> u32 {
        (v as u32) | (((!v) as u32) << 16)
    }

    /// Verify the redundant halves of `word` and return the count, faulting on
    /// mismatch.
    #[inline]
    fn verify(word: u32) -> u16 {
        let lo = word as u16;
        let hi = (word >> 16) as u16;
        if lo != !hi {
            crate::fusa::on_error(module_path!(), line!());
        }
        lo
    }

    /// Create a count initialised to `v`.
    pub const fn new(v: u16) -> Self {
        Self {
            packed: AtomicU32::new(Self::pack(v)),
        }
    }

    /// Verified read of the current count (Acquire).
    pub fn load(&self) -> u16 {
        Self::verify(self.packed.load(Ordering::Acquire))
    }

    /// Atomically increment (Relaxed, as for an `Arc` clone). Faults on
    /// corruption or overflow.
    pub fn increment(&self) {
        let mut cur = self.packed.load(Ordering::Relaxed);
        loop {
            let v = Self::verify(cur);
            let next = match v.checked_add(1) {
                Some(n) => n,
                None => crate::fusa::on_error(module_path!(), line!()),
            };
            match self.packed.compare_exchange_weak(
                cur,
                Self::pack(next),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => cur = actual,
            }
        }
    }

    /// Atomically decrement (Release, as for an `Arc` drop), returning the
    /// value **before** the decrement. Faults on corruption or underflow (a
    /// decrement at zero indicates a double-free).
    pub fn decrement(&self) -> u16 {
        let mut cur = self.packed.load(Ordering::Relaxed);
        loop {
            let v = Self::verify(cur);
            let next = match v.checked_sub(1) {
                Some(n) => n,
                None => crate::fusa::on_error(module_path!(), line!()),
            };
            match self.packed.compare_exchange_weak(
                cur,
                Self::pack(next),
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return v,
                Err(actual) => cur = actual,
            }
        }
    }

    /// Store an inconsistent word to simulate corruption — test only.
    #[cfg(test)]
    fn corrupt_for_test(&self) {
        // lo = 1, hi = 0 → !hi = 0xFFFF != 1, so the halves disagree.
        self.packed.store(0x0000_0001, Ordering::SeqCst);
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

    #[test]
    fn dup_roundtrip_and_update() {
        let mut d = Dup::new(0xDEAD_BEEFu32);
        assert!(d.is_intact());
        assert_eq!(d.get(), 0xDEAD_BEEF);
        d.set(0);
        assert_eq!(d.get(), 0);
        assert!(d.is_intact());

        // Works for non-integer `Copy + PartialEq` values (e.g. fn pointers).
        fn a() {}
        fn b() {}
        let mut f: Dup<fn()> = Dup::new(a);
        assert!(f.is_intact());
        f.set(b);
        // Compare by address rather than the fn pointers directly (the latter
        // is not a meaningful comparison per clippy); the read must still be
        // intact.
        assert!(f.is_intact());
        assert_eq!(f.get() as usize, b as usize);
    }

    #[test]
    fn dup_corruption_is_detected_without_faulting() {
        let mut d = Dup::new(7u32);
        assert!(d.is_intact());
        d.corrupt_with(8);
        assert!(!d.is_intact());
    }

    #[cfg(feature = "std")]
    #[test]
    fn dup_corrupted_read_faults() {
        let prev = std::panic::take_hook();
        std::panic::set_hook(std::boxed::Box::new(|_| {}));
        let mut d = Dup::new(7u32);
        d.corrupt_with(9);
        let caught = std::panic::catch_unwind(move || d.get());
        std::panic::set_hook(prev);
        assert!(caught.is_err(), "a corrupted Dup read must fault");
    }

    #[test]
    fn atomic_count_basic() {
        let c = DisAtomicU16::new(1);
        assert_eq!(c.load(), 1);
        c.increment();
        c.increment();
        assert_eq!(c.load(), 3);
        assert_eq!(c.decrement(), 3); // returns the pre-decrement value
        assert_eq!(c.load(), 2);
    }

    #[cfg(feature = "std")]
    #[test]
    fn atomic_count_corruption_faults() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let prev = std::panic::take_hook();
        std::panic::set_hook(std::boxed::Box::new(|_| {}));
        let c = DisAtomicU16::new(5);
        c.corrupt_for_test();
        let caught = catch_unwind(AssertUnwindSafe(|| c.load()));
        std::panic::set_hook(prev);
        assert!(caught.is_err(), "a corrupted atomic-count read must fault");
    }

    #[cfg(feature = "std")]
    #[test]
    fn atomic_count_underflow_faults() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let prev = std::panic::take_hook();
        std::panic::set_hook(std::boxed::Box::new(|_| {}));
        let c = DisAtomicU16::new(0);
        let caught = catch_unwind(AssertUnwindSafe(|| c.decrement())); // double-free
        std::panic::set_hook(prev);
        assert!(caught.is_err(), "decrement at zero must fault (double-free)");
    }

    #[cfg(feature = "std")]
    #[test]
    fn atomic_count_concurrent_balanced() {
        use std::sync::Arc as StdArc;
        let c = StdArc::new(DisAtomicU16::new(1));
        let mut handles = std::vec::Vec::new();
        for _ in 0..4 {
            let c = StdArc::clone(&c);
            handles.push(std::thread::spawn(move || {
                // Each iteration increments before decrementing, so the count
                // never drops below the initial 1 → no spurious underflow.
                for _ in 0..2000 {
                    c.increment();
                    c.decrement();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(c.load(), 1, "balanced concurrent inc/dec must return to 1");
    }
}
