//! Heap-free, reference-counted, type-erased event payload — the QEvt model.
//!
//! This is the `static-alloc` replacement for `Arc<dyn Any + Send + Sync>` on
//! the event path (see `docs/FUSA.md`, Phase 2). A [`PoolArc`] is a shared,
//! reference-counted handle to a value that lives **inside a fixed-size pool
//! block** ([`crate::pool::QMPool`] via [`POOL_REGISTRY`]) — no global
//! allocator, no heap.
//!
//! It mirrors the small slice of the `Arc` API the framework actually uses:
//! - construct from a `T: Any + Send + Sync`,
//! - [`as_any`](PoolArc::as_any) → `&dyn Any` for `downcast_ref`,
//! - `Clone` (atomic refcount increment),
//! - `Drop` (atomic decrement; on the last reference the value is dropped and
//!   the block returned to its pool).
//!
//! A signal-only event (`()` payload) uses the allocation-free
//! [`PoolArc::empty`] variant, so the common case needs no pool at all.

use core::any::Any;
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::event_pool::POOL_REGISTRY;

/// Control block placed at the head of each pooled allocation.
///
/// Layout (`#[repr(C)]`) is `[CtrlHeader][value: T]` within one pool block; the
/// `value` fat pointer points at the trailing `value` field, carrying the
/// `dyn Any` vtable (and thus the drop glue) for the erased `T`.
#[repr(C)]
struct CtrlHeader {
    ref_count: AtomicUsize,
    pool_id: u8,
    /// Fat pointer to the trailing value, typed as `dyn Any` for downcasting
    /// and for `drop_in_place` of the concrete `T` on the last release.
    value: *mut (dyn Any + Send + Sync),
}

#[repr(C)]
struct Block<T> {
    header: CtrlHeader,
    value: T,
}

/// Static unit value backing the allocation-free [`PoolArc::empty`] payload.
static UNIT: () = ();

/// Heap-free, reference-counted, type-erased payload handle.
pub struct PoolArc {
    repr: Repr,
}

enum Repr {
    /// Signal-only payload (`()`); no allocation, no pool.
    Empty,
    /// Pool-allocated, reference-counted payload (points at the `CtrlHeader`).
    Pooled(NonNull<CtrlHeader>),
}

// SAFETY: the erased value is constrained to `Send + Sync` at construction, and
// the reference count is an atomic — so sharing/moving a `PoolArc` across
// threads is sound, exactly as for `Arc<dyn Any + Send + Sync>`.
unsafe impl Send for PoolArc {}
unsafe impl Sync for PoolArc {}

impl PoolArc {
    /// An allocation-free, empty (`()`) payload — for signal-only events.
    pub const fn empty() -> Self {
        Self { repr: Repr::Empty }
    }

    /// Allocate a pooled payload holding `value`.
    ///
    /// Faults via [`crate::fusa::on_error`] if no registered pool can serve the
    /// value (none large enough, or all exhausted) — the crash-only response to
    /// an undersized/unregistered pool configuration, matching QP/C's `q_new`
    /// assertion.
    pub fn from_value<T: Any + Send + Sync>(value: T) -> Self {
        match Self::try_from_value(value) {
            Some(p) => p,
            None => crate::fusa::on_error(module_path!(), line!()),
        }
    }

    /// Like [`from_value`](Self::from_value) but returns `None` instead of
    /// faulting when no pool can serve the allocation.
    pub fn try_from_value<T: Any + Send + Sync>(value: T) -> Option<Self> {
        // Pool blocks are `usize`-aligned; refuse over-aligned payloads rather
        // than hand out a misaligned reference.
        if core::mem::align_of::<Block<T>>() > core::mem::align_of::<usize>() {
            crate::fusa::on_error(module_path!(), line!());
        }
        let size = core::mem::size_of::<Block<T>>();
        let (pool_id, raw) = POOL_REGISTRY.alloc(size, 0, None)?;
        let block = raw as *mut Block<T>;
        // A correctly-typed placeholder fat pointer (carrying `T`'s vtable);
        // dangling and never dereferenced — overwritten right after the write.
        let placeholder: *mut (dyn Any + Send + Sync) = NonNull::<T>::dangling().as_ptr();
        // SAFETY: `raw` is an uninitialised, `usize`-aligned block of at least
        // `size` bytes, exclusively owned until we hand back the `PoolArc`.
        unsafe {
            ptr::write(
                block,
                Block {
                    header: CtrlHeader {
                        ref_count: AtomicUsize::new(1),
                        pool_id,
                        value: placeholder,
                    },
                    value,
                },
            );
            // Fix up the fat pointer now that `value` has a stable address.
            let value_ptr: *mut T = ptr::addr_of_mut!((*block).value);
            let any_ptr: *mut (dyn Any + Send + Sync) = value_ptr;
            (*block).header.value = any_ptr;
            Some(Self {
                repr: Repr::Pooled(NonNull::new_unchecked(block as *mut CtrlHeader)),
            })
        }
    }

    /// Borrow the payload as `&dyn Any` for downcasting.
    pub fn as_any(&self) -> &(dyn Any + Send + Sync) {
        match self.repr {
            Repr::Empty => &UNIT,
            // SAFETY: `value` is a valid fat pointer into the live pool block
            // for as long as this `PoolArc` (a strong reference) exists.
            Repr::Pooled(p) => unsafe { &*p.as_ref().value },
        }
    }

    /// Convenience: borrow and downcast in one step.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }
}

impl Clone for PoolArc {
    fn clone(&self) -> Self {
        if let Repr::Pooled(p) = self.repr {
            // SAFETY: a live strong reference keeps the control block valid.
            // Relaxed is sufficient for the increment, as in `Arc`.
            unsafe { p.as_ref().ref_count.fetch_add(1, Ordering::Relaxed) };
            Self { repr: Repr::Pooled(p) }
        } else {
            Self { repr: Repr::Empty }
        }
    }
}

impl Drop for PoolArc {
    fn drop(&mut self) {
        let Repr::Pooled(p) = self.repr else { return };
        // SAFETY: a live strong reference keeps the control block valid.
        unsafe {
            if p.as_ref().ref_count.fetch_sub(1, Ordering::Release) != 1 {
                return;
            }
            // Last reference: synchronise with all prior releases, then drop the
            // value via its `dyn Any` vtable and return the block to its pool.
            core::sync::atomic::fence(Ordering::Acquire);
            let header = p.as_ptr();
            let pool_id = (*header).pool_id;
            ptr::drop_in_place((*header).value);
            POOL_REGISTRY.free(pool_id, header as *mut u8, None);
        }
    }
}

impl core::fmt::Debug for PoolArc {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.repr {
            Repr::Empty => f.write_str("PoolArc::Empty"),
            Repr::Pooled(_) => f.write_str("PoolArc::Pooled"),
        }
    }
}

#[cfg(all(test, feature = "static-alloc", feature = "std"))]
mod tests {
    use super::*;
    use std::boxed::Box;
    use std::vec;

    // Register one pool, once, large enough for the payloads exercised here.
    fn ensure_pool() -> u8 {
        use core::sync::atomic::{AtomicU8, Ordering};
        static POOL_ID: AtomicU8 = AtomicU8::new(0);
        let existing = POOL_ID.load(Ordering::SeqCst);
        if existing != 0 {
            return existing;
        }
        let storage: &'static mut [u8] = Box::leak(vec![0u8; 2048].into_boxed_slice());
        let id = POOL_REGISTRY.init_pool(storage, 128);
        POOL_ID.store(id, Ordering::SeqCst);
        id
    }

    #[test]
    fn pooled_roundtrip_clone_and_free() {
        let pool_id = ensure_pool();
        let before = POOL_REGISTRY.get_free(pool_id).unwrap();

        let a = PoolArc::from_value(0xDEAD_BEEFu32);
        assert_eq!(a.downcast_ref::<u32>(), Some(&0xDEAD_BEEF));
        assert_eq!(a.downcast_ref::<u64>(), None);
        assert_eq!(POOL_REGISTRY.get_free(pool_id).unwrap(), before - 1);

        // Clone shares the same block (refcount 2) — no new allocation.
        let b = a.clone();
        assert_eq!(POOL_REGISTRY.get_free(pool_id).unwrap(), before - 1);
        assert_eq!(b.downcast_ref::<u32>(), Some(&0xDEAD_BEEF));

        drop(a);
        // Still one strong ref → block not yet returned.
        assert_eq!(POOL_REGISTRY.get_free(pool_id).unwrap(), before - 1);
        drop(b);
        // Last ref dropped → block returned to the pool.
        assert_eq!(POOL_REGISTRY.get_free(pool_id).unwrap(), before);
    }

    #[test]
    fn empty_is_allocation_free() {
        let pool_id = ensure_pool();
        let before = POOL_REGISTRY.get_free(pool_id).unwrap();
        let e = PoolArc::empty();
        let e2 = e.clone();
        assert!(e.downcast_ref::<u32>().is_none());
        assert!(e2.as_any().downcast_ref::<()>().is_some());
        assert_eq!(POOL_REGISTRY.get_free(pool_id).unwrap(), before);
    }

    #[test]
    fn drop_runs_value_destructor() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static DROPS: AtomicUsize = AtomicUsize::new(0);
        struct Tracked;
        impl Drop for Tracked {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }
        ensure_pool();
        DROPS.store(0, Ordering::SeqCst);
        let a = PoolArc::from_value(Tracked);
        let b = a.clone();
        drop(a);
        assert_eq!(DROPS.load(Ordering::SeqCst), 0); // still referenced
        drop(b);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1); // destructor ran exactly once
    }
}
