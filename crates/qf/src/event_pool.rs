//! Framework-level event pools — Phase 3.
//!
//! Provides a global registry of up to `MAX_POOLS` [`QMPool`] instances and
//! typed allocation helpers ([`q_new`], [`q_new_x`]) that select the smallest
//! fitting pool.  [`EventBox<T>`] is a smart pointer that calls `gc()` on drop,
//! mirroring QP/C++ `QF::gc()` semantics.
//!
//! # Thread safety
//!
//! Each pool slot is protected by a `spin::Mutex`.  `spin::Mutex::new` is
//! `const fn`, which allows the global `POOL_REGISTRY` static to be
//! initialised at compile time in both `std` and `no_std` builds.
//!
//! Pool registration (`PoolRegistry::init_pool`) must happen before the
//! kernel starts.

#[cfg(not(feature = "static-alloc"))]
use core::any::Any;
use core::mem;
use core::ptr;

use crate::event::{Event, EventHeader, Signal};
use crate::pool::QMPool;
#[cfg(not(feature = "static-alloc"))]
use crate::sync::Arc;
use crate::trace::TraceHook;

// QS record IDs for event pool operations.
const QS_QF_NEW:         u8 = 28;
const QS_QF_GC:          u8 = 30;
const QS_QF_MPOOL_GET:   u8 = 24;
const QS_QF_MPOOL_PUT:   u8 = 25;
const QS_QF_NEW_ATTEMPT: u8 = 23;

/// Maximum number of pools that can be registered (matches QF_MAX_EPOOL).
pub const MAX_POOLS: usize = 15;

// ── PoolSlot ─────────────────────────────────────────────────────────────────

struct PoolSlot {
    pool: QMPool,
}

// SAFETY: QMPool owns its storage exclusively; the spin::Mutex serialises access.
unsafe impl Send for PoolSlot {}
unsafe impl Sync for PoolSlot {}

// ── PoolRegistry ─────────────────────────────────────────────────────────────

/// Global registry of event pools, indexed by 1-based pool-id.
///
/// Use `POOL_REGISTRY` for the process-wide instance, or create a local
/// `PoolRegistry::new()` for unit tests.
pub struct PoolRegistry {
    /// `spin::Mutex` has a `const fn new()` usable in `static` initialisers.
    slots: [spin::Mutex<PoolSlot>; MAX_POOLS],
    count: core::sync::atomic::AtomicUsize,
}

// SAFETY: all interior mutability is protected by the per-slot spin::Mutex.
unsafe impl Send for PoolRegistry {}
unsafe impl Sync for PoolRegistry {}

macro_rules! pool_slot {
    () => { spin::Mutex::new(PoolSlot { pool: QMPool::uninit() }) };
}

impl PoolRegistry {
    /// Creates an empty pool registry.  `const fn` — usable in `static`.
    pub const fn new() -> Self {
        Self {
            slots: [
                pool_slot!(), pool_slot!(), pool_slot!(),
                pool_slot!(), pool_slot!(), pool_slot!(),
                pool_slot!(), pool_slot!(), pool_slot!(),
                pool_slot!(), pool_slot!(), pool_slot!(),
                pool_slot!(), pool_slot!(), pool_slot!(),
            ],
            count: core::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Register a pool backed by `storage` with blocks of `block_size` bytes.
    ///
    /// Returns the 1-based pool-id assigned.  Register pools in order of
    /// **increasing** `block_size`; `q_new` selects the first pool that fits.
    ///
    /// # Panics
    ///
    /// Panics if more than `MAX_POOLS` pools are registered.
    pub fn init_pool(&self, storage: &'static mut [u8], block_size: usize) -> u8 {
        use core::sync::atomic::Ordering;
        let idx = self.count.fetch_add(1, Ordering::SeqCst);
        assert!(idx < MAX_POOLS, "PoolRegistry: max {} pools exceeded", MAX_POOLS);
        self.slots[idx].lock().pool.init(storage, block_size);
        (idx + 1) as u8
    }

    /// Number of registered pools.
    pub fn pool_count(&self) -> usize {
        self.count.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Allocate one block from the smallest pool that fits `size` bytes.
    ///
    /// Returns `(pool_id, raw_ptr)` on success.  The caller must eventually
    /// call `free(pool_id, raw_ptr)`.
    pub fn alloc(&self, size: usize, margin: usize, trace: Option<&TraceHook>) -> Option<(u8, *mut u8)> {
        let n = self.pool_count();
        for i in 0..n {
            let slot = self.slots[i].lock();
            if !slot.pool.can_serve(size) {
                continue;
            }
            if let Some(ptr) = slot.pool.get(margin) {
                let pool_id = (i + 1) as u8;
                let free  = slot.pool.get_free();
                let total = slot.pool.num_blocks();
                drop(slot);
                if let Some(hook) = trace {
                    emit_mpool_get(hook, pool_id, free as u16, total as u16);
                    emit_new(hook, size as u16, pool_id);
                }
                return Some((pool_id, ptr));
            } else {
                // Pool matches size but margin not met — record and try next.
                let free  = slot.pool.get_free();
                let total = slot.pool.num_blocks();
                drop(slot);
                if let Some(hook) = trace {
                    emit_new_attempt(hook, size as u16, (i + 1) as u8, free as u16, total as u16);
                }
            }
        }
        None
    }

    /// Return a block to pool `pool_id` (1-based).
    ///
    /// # Safety
    ///
    /// `block` must be a pointer returned by `alloc()` for `pool_id`.
    pub unsafe fn free(&self, pool_id: u8, block: *mut u8, trace: Option<&TraceHook>) {
        let idx = (pool_id as usize).saturating_sub(1);
        if idx >= MAX_POOLS { return; }
        let slot = self.slots[idx].lock();
        if !slot.pool.is_init() { return; }
        slot.pool.put(block);
        let free  = slot.pool.get_free();
        let total = slot.pool.num_blocks();
        drop(slot);
        if let Some(hook) = trace {
            emit_mpool_put(hook, pool_id, free as u16, total as u16);
            emit_gc(hook, pool_id);
        }
    }

    /// Free blocks in pool `pool_id` (1-based).
    pub fn get_free(&self, pool_id: u8) -> Option<usize> {
        let idx = (pool_id as usize).saturating_sub(1);
        if idx >= MAX_POOLS { return None; }
        let slot = self.slots[idx].lock();
        slot.pool.is_init().then(|| slot.pool.get_free())
    }

    /// Low-watermark (minimum free ever) for pool `pool_id`.
    pub fn get_min(&self, pool_id: u8) -> Option<usize> {
        let idx = (pool_id as usize).saturating_sub(1);
        if idx >= MAX_POOLS { return None; }
        let slot = self.slots[idx].lock();
        slot.pool.is_init().then(|| slot.pool.get_min())
    }

    /// In-use block count for pool `pool_id`.
    pub fn get_use(&self, pool_id: u8) -> Option<usize> {
        let idx = (pool_id as usize).saturating_sub(1);
        if idx >= MAX_POOLS { return None; }
        let slot = self.slots[idx].lock();
        slot.pool.is_init().then(|| slot.pool.get_use())
    }
}

impl Default for PoolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Process-wide global registry ─────────────────────────────────────────────

/// Process-wide event pool registry.
///
/// Register pools on this object before calling `Kernel::start()`.  The
/// underlying `spin::Mutex` per slot makes this safe in `static` context.
pub static POOL_REGISTRY: PoolRegistry = PoolRegistry::new();

// ── EventBox<T> ───────────────────────────────────────────────────────────────

/// Owning pointer to a pool-allocated event.
///
/// When dropped, the pool block is automatically returned (equivalent to
/// `QF::gc()`).  Dereferences to `Event<T>` for direct field access.
pub struct EventBox<T: 'static> {
    ptr: *mut Event<T>,
    /// 1-based pool id; 0 = heap-only, no pool to return to.
    pool_id: u8,
}

unsafe impl<T: Send + 'static> Send for EventBox<T> {}
unsafe impl<T: Sync + 'static> Sync for EventBox<T> {}

impl<T: 'static> EventBox<T> {
    /// # Safety
    /// `ptr` must point to a valid `Event<T>` inside the given pool's block.
    pub unsafe fn from_raw(ptr: *mut Event<T>, pool_id: u8) -> Self {
        Self { ptr, pool_id }
    }

    /// Consume without freeing.  Caller takes ownership of `(ptr, pool_id)`.
    pub fn into_raw(self) -> (*mut Event<T>, u8) {
        let r = (self.ptr, self.pool_id);
        mem::forget(self);
        r
    }

    /// Convert into a `DynEvent` for posting to an active object.
    ///
    /// The pool block is released when the last reference to the `Arc` payload
    /// is dropped via the embedded [`PoolBlock`] RAII guard.
    #[cfg(not(feature = "static-alloc"))]
    pub fn into_dyn(self) -> crate::event::DynEvent
    where
        T: Send + Sync,
    {
        let event: Event<T> = unsafe { ptr::read(self.ptr) };
        let (raw_ptr, pool_id) = self.into_raw();
        let guard = PoolBlock { ptr: raw_ptr as *mut u8, pool_id };
        let payload: Arc<dyn Any + Send + Sync> = Arc::new(PoolPayload {
            _value: event.payload,
            _guard: guard,
        });
        Event { header: event.header, payload }
    }

    /// Convert into a `DynEvent` for posting to an active object.
    ///
    /// Heap-free `static-alloc` path: the payload is re-homed into a
    /// reference-counted [`PoolArc`](crate::pool_arc::PoolArc) and the original
    /// `EventBox` block is returned to its pool.
    #[cfg(feature = "static-alloc")]
    pub fn into_dyn(self) -> crate::event::DynEvent
    where
        T: Send + Sync,
    {
        // Move the event out of its block (bitwise), then return the now-vacated
        // block to the pool without running any drop glue.
        let event: Event<T> = unsafe { ptr::read(self.ptr) };
        let (raw_ptr, pool_id) = self.into_raw();
        unsafe { gc_raw(pool_id, raw_ptr as *mut u8, None) };
        let payload = crate::pool_arc::PoolArc::from_value(event.payload);
        Event { header: event.header, payload }
    }
}

impl<T: 'static> core::ops::Deref for EventBox<T> {
    type Target = Event<T>;
    fn deref(&self) -> &Self::Target { unsafe { &*self.ptr } }
}

impl<T: 'static> core::ops::DerefMut for EventBox<T> {
    fn deref_mut(&mut self) -> &mut Self::Target { unsafe { &mut *self.ptr } }
}

impl<T: 'static> Drop for EventBox<T> {
    fn drop(&mut self) {
        if !self.ptr.is_null() && self.pool_id != 0 {
            unsafe {
                ptr::drop_in_place(self.ptr);
                gc_raw(self.pool_id, self.ptr as *mut u8, None);
            }
        }
    }
}

// ── RAII block guard inside Arc payload (dynamic build only) ──────────────────

#[cfg(not(feature = "static-alloc"))]
struct PoolBlock { ptr: *mut u8, pool_id: u8 }
#[cfg(not(feature = "static-alloc"))]
unsafe impl Send for PoolBlock {}
#[cfg(not(feature = "static-alloc"))]
unsafe impl Sync for PoolBlock {}
#[cfg(not(feature = "static-alloc"))]
impl Drop for PoolBlock {
    fn drop(&mut self) {
        if !self.ptr.is_null() && self.pool_id != 0 {
            unsafe { gc_raw(self.pool_id, self.ptr, None); }
        }
    }
}

#[cfg(not(feature = "static-alloc"))]
struct PoolPayload<T> { _value: T, _guard: PoolBlock }
#[cfg(not(feature = "static-alloc"))]
unsafe impl<T: Send> Send for PoolPayload<T> {}
#[cfg(not(feature = "static-alloc"))]
unsafe impl<T: Sync> Sync for PoolPayload<T> {}

// ── Public allocation helpers ─────────────────────────────────────────────────

/// Allocate a pool-backed event from the global [`POOL_REGISTRY`].
///
/// Selects the smallest registered pool whose block fits `Event<T>`.
/// Returns `None` if no pool is large enough or all pools are exhausted.
///
/// Equivalent to QP/C++ `QF::q_new<T>(sig)`.
pub fn q_new<T: Send + Sync + 'static>(
    sig: Signal,
    payload: T,
    trace: Option<&TraceHook>,
) -> Option<EventBox<T>> {
    q_new_x(sig, payload, 0, trace)
}

/// Like [`q_new`] but enforces a minimum free-block `margin`.
///
/// Returns `None` if the allocation would leave fewer than `margin` free
/// blocks in the selected pool.
///
/// Equivalent to QP/C++ `QF::q_new_x<T>(margin, sig)`.
pub fn q_new_x<T: Send + Sync + 'static>(
    sig: Signal,
    payload: T,
    margin: usize,
    trace: Option<&TraceHook>,
) -> Option<EventBox<T>> {
    let size = mem::size_of::<Event<T>>();
    let (pool_id, raw) = POOL_REGISTRY.alloc(size, margin, trace)?;
    unsafe {
        ptr::write(raw as *mut Event<T>, Event {
            header: EventHeader::new(sig).with_pool(pool_id),
            payload,
        });
        Some(EventBox::from_raw(raw as *mut Event<T>, pool_id))
    }
}

/// Garbage-collect a reference to a pool event.
///
/// For `Arc`-wrapped events the actual deallocation happens when the last
/// `Arc` clone is dropped (via the embedded [`PoolBlock`] guard).  This
/// function is provided for API completeness and optionally emits a QS trace.
///
/// Equivalent to QP/C++ `QF::gc(e)`.
pub fn gc(event: &crate::event::DynEvent, trace: Option<&TraceHook>) {
    if let (Some(pool_id), Some(hook)) = (event.header.pool_id, trace) {
        emit_gc(hook, pool_id);
    }
}

/// Internal raw free — called by [`EventBox::drop`] and [`PoolBlock::drop`].
unsafe fn gc_raw(pool_id: u8, ptr: *mut u8, trace: Option<&TraceHook>) {
    POOL_REGISTRY.free(pool_id, ptr, trace);
}

// ── QS trace helpers ──────────────────────────────────────────────────────────

fn emit_new(hook: &TraceHook, size: u16, pool_id: u8) {
    let _ = hook(QS_QF_NEW, &[(size & 0xFF) as u8, (size >> 8) as u8, pool_id], true);
}

fn emit_new_attempt(hook: &TraceHook, size: u16, pool_id: u8, free: u16, total: u16) {
    let _ = hook(QS_QF_NEW_ATTEMPT, &[
        (size & 0xFF) as u8, (size >> 8) as u8, pool_id,
        (free & 0xFF) as u8, (free >> 8) as u8,
        (total & 0xFF) as u8, (total >> 8) as u8,
    ], true);
}

fn emit_gc(hook: &TraceHook, pool_id: u8) {
    let _ = hook(QS_QF_GC, &[pool_id], true);
}

fn emit_mpool_get(hook: &TraceHook, pool_id: u8, free: u16, total: u16) {
    let _ = hook(QS_QF_MPOOL_GET, &[
        pool_id,
        (free & 0xFF) as u8, (free >> 8) as u8,
        (total & 0xFF) as u8, (total >> 8) as u8,
    ], true);
}

fn emit_mpool_put(hook: &TraceHook, pool_id: u8, free: u16, total: u16) {
    let _ = hook(QS_QF_MPOOL_PUT, &[
        pool_id,
        (free & 0xFF) as u8, (free >> 8) as u8,
        (total & 0xFF) as u8, (total >> 8) as u8,
    ], true);
}
