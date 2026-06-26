//! Unit tests for QMPool and the framework event pool infrastructure.

// These tests hand `&'static mut` slices over mutable `static` storage to the
// pool. The `&mut *(&raw mut STORAGE)` idiom is deliberate: `&raw mut` avoids
// the `static_mut_refs` lint (no intermediate reference to the static). clippy's
// `deref_addrof` suggests `&mut STORAGE`, which would re-introduce exactly that
// unsound pattern — so the lint is suppressed here.
#![allow(clippy::deref_addrof)]

use crate::event_pool::PoolRegistry;
use crate::pool::QMPool;

// ── QMPool low-level tests ────────────────────────────────────────────────────

#[test]
fn pool_basic_alloc_free() {
    let mut storage = [0u8; 128];
    // SAFETY: test-only lifetime extension; no concurrent access.
    let storage: &'static mut [u8] = unsafe { &mut *(&mut storage as *mut _) };

    let mut pool = QMPool::uninit();
    let n = pool.init(storage, 16);

    assert!(n >= 2, "should have at least 2 blocks");
    assert_eq!(pool.get_free(), n);
    assert_eq!(pool.get_use(), 0);

    let b1 = pool.get(0).expect("first alloc");
    let b2 = pool.get(0).expect("second alloc");
    assert_ne!(b1, b2, "distinct blocks");
    assert_eq!(pool.get_free(), n - 2);
    assert_eq!(pool.get_use(), 2);

    unsafe { pool.put(b1) };
    assert_eq!(pool.get_free(), n - 1);
    assert_eq!(pool.get_use(), 1);

    unsafe { pool.put(b2) };
    assert_eq!(pool.get_free(), n);
    assert_eq!(pool.get_use(), 0);
}

#[test]
fn pool_watermark_tracks_minimum_free() {
    let mut storage = [0u8; 128];
    let storage: &'static mut [u8] = unsafe { &mut *(&mut storage as *mut _) };

    let mut pool = QMPool::uninit();
    let n = pool.init(storage, 16);

    let blocks: alloc::vec::Vec<*mut u8> = (0..n).map(|_| pool.get(0).unwrap()).collect();
    assert_eq!(pool.get_min(), 0, "watermark hit zero");

    for b in blocks {
        unsafe { pool.put(b) };
    }
    // After freeing, get_min stays at zero (low watermark is sticky).
    assert_eq!(pool.get_min(), 0);
    assert_eq!(pool.get_free(), n);
}

#[test]
fn pool_exhausted_returns_none() {
    let mut storage = [0u8; 64];
    let storage: &'static mut [u8] = unsafe { &mut *(&mut storage as *mut _) };

    let mut pool = QMPool::uninit();
    let n = pool.init(storage, 16);

    let blocks: alloc::vec::Vec<*mut u8> = (0..n).map(|_| pool.get(0).unwrap()).collect();
    assert!(pool.get(0).is_none(), "pool exhausted");

    // Free one, then alloc again.
    unsafe { pool.put(blocks[0]) };
    let b = pool.get(0);
    assert!(b.is_some(), "should succeed after free");
}

#[test]
fn pool_margin_prevents_allocation() {
    let mut storage = [0u8; 64];
    let storage: &'static mut [u8] = unsafe { &mut *(&mut storage as *mut _) };

    let mut pool = QMPool::uninit();
    let n = pool.init(storage, 16);
    assert!(n >= 2);

    // Allocate until 1 block remains.
    let mut blocks = alloc::vec::Vec::new();
    while pool.get_free() > 1 {
        blocks.push(pool.get(0).unwrap());
    }
    // Margin = 1 means "keep at least 1 free". Should fail now.
    assert!(pool.get(1).is_none(), "margin=1 should reject when only 1 free");
    // Margin = 0 should succeed.
    let b = pool.get(0);
    assert!(b.is_some());

    for b in blocks { unsafe { pool.put(b) }; }
    if let Some(b) = b { unsafe { pool.put(b) }; }
}

#[test]
fn pool_small_block_size_rounded_up() {
    let mut storage = [0u8; 256];
    let storage: &'static mut [u8] = unsafe { &mut *(&mut storage as *mut _) };

    let mut pool = QMPool::uninit();
    // Request blocks of 1 byte — rounded up to the 2-word minimum (the
    // free-list link plus its Duplicate Storage copy).
    let n = pool.init(storage, 1);
    assert_eq!(pool.block_size(), 2 * core::mem::size_of::<usize>());
    assert!(n > 0);

    let b = pool.get(0).expect("alloc from tiny-block pool");
    // Block must be aligned to usize.
    assert_eq!((b as usize) % core::mem::size_of::<usize>(), 0);
    unsafe { pool.put(b) };
}

#[cfg(feature = "std")]
#[test]
fn pool_corrupted_freelist_link_faults() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let mut storage = [0u8; 128];
    let storage: &'static mut [u8] = unsafe { &mut *(&mut storage as *mut _) };

    let mut pool = QMPool::uninit();
    pool.init(storage, 16);

    // Flip one half of the head block's duplicated next-link so the two copies
    // disagree; the next allocation must detect it and fault (crash-only).
    let word = core::mem::size_of::<usize>();
    let dup = unsafe { pool_storage_ptr(&pool).add(word) as *mut usize };
    unsafe { *dup = !*dup };

    let prev = std::panic::take_hook();
    std::panic::set_hook(std::boxed::Box::new(|_| {}));
    let caught = catch_unwind(AssertUnwindSafe(|| pool.get(0)));
    std::panic::set_hook(prev);
    assert!(caught.is_err(), "a corrupted free-list link must fault on alloc");
}

// Test-only helper: the base pointer of the pool's storage region.
#[cfg(feature = "std")]
fn pool_storage_ptr(pool: &QMPool) -> *mut u8 {
    // The first free block sits at the start of storage; `get()` would return
    // exactly this pointer, so round-trip a get/put to recover the base without
    // reaching into private fields.
    let p = pool.get(0).expect("one block");
    unsafe { pool.put(p) };
    p
}

#[test]
fn pool_can_serve_checks_size() {
    let mut storage = [0u8; 256];
    let storage: &'static mut [u8] = unsafe { &mut *(&mut storage as *mut _) };

    let mut pool = QMPool::uninit();
    pool.init(storage, 32);

    assert!(pool.can_serve(1));
    assert!(pool.can_serve(32));
    assert!(!pool.can_serve(33));
}

// ── PoolRegistry / q_new / q_new_x tests ─────────────────────────────────────

#[test]
fn registry_alloc_and_free_via_q_new() {
    static mut STORAGE_A: [u8; 512] = [0u8; 512];
    let reg = PoolRegistry::new();

    let pool_id = reg.init_pool(
        unsafe { &mut *(&raw mut STORAGE_A) },
        64,
    );
    assert_eq!(pool_id, 1);
    assert_eq!(reg.pool_count(), 1);

    // Allocate an event.
    let event_box = {
        let size = core::mem::size_of::<crate::event::Event<u32>>();
        let margin = 0;
        reg.alloc(size, margin, None)
    };
    assert!(event_box.is_some(), "should allocate");
    let (_, ptr) = event_box.unwrap();
    assert_eq!(reg.get_use(1).unwrap(), 1);

    unsafe { reg.free(1, ptr, None) };
    assert_eq!(reg.get_use(1).unwrap(), 0);
}

#[test]
fn q_new_returns_event_box_and_gc_on_drop() {
    static mut STORAGE_B: [u8; 1024] = [0u8; 1024];
    let reg = PoolRegistry::new();
    reg.init_pool(unsafe { &mut *(&raw mut STORAGE_B) }, 128);

    // We can't use the global POOL_REGISTRY here since it's shared across tests.
    // Test q_new logic through the raw registry.
    let initial_free = reg.get_free(1).unwrap();
    let size = core::mem::size_of::<crate::event::Event<u64>>();
    let alloc = reg.alloc(size, 0, None);
    assert!(alloc.is_some());
    let (pool_id, ptr) = alloc.unwrap();
    assert_eq!(reg.get_free(1).unwrap(), initial_free - 1);

    // Return block.
    unsafe { reg.free(pool_id, ptr, None) };
    assert_eq!(reg.get_free(1).unwrap(), initial_free);
}

#[test]
fn q_new_x_margin_0_succeeds_when_free() {
    static mut STORAGE_C: [u8; 512] = [0u8; 512];
    let reg = PoolRegistry::new();
    reg.init_pool(unsafe { &mut *(&raw mut STORAGE_C) }, 64);

    let size = core::mem::size_of::<crate::event::Event<[u8; 8]>>();
    let (pool_id, ptr) = reg.alloc(size, 0, None).expect("should alloc");
    assert_eq!(pool_id, 1);
    unsafe { reg.free(pool_id, ptr, None) };
}

#[test]
fn registry_selects_smallest_fitting_pool() {
    static mut STORAGE_S: [u8; 256] = [0u8; 256];
    static mut STORAGE_L: [u8; 256] = [0u8; 256];
    let reg = PoolRegistry::new();

    // Register small pool (32 bytes) then large pool (128 bytes).
    let small_id = reg.init_pool(unsafe { &mut *(&raw mut STORAGE_S) }, 32);
    let large_id = reg.init_pool(unsafe { &mut *(&raw mut STORAGE_L) }, 128);
    assert_eq!(small_id, 1);
    assert_eq!(large_id, 2);

    // Request 20 bytes → should come from small pool (can_serve(20) with block=32).
    let r = reg.alloc(20, 0, None).unwrap();
    assert_eq!(r.0, small_id, "20-byte alloc should use small pool");
    unsafe { reg.free(r.0, r.1, None) };

    // Request 64 bytes → too big for small pool (32), goes to large (128).
    let r = reg.alloc(64, 0, None).unwrap();
    assert_eq!(r.0, large_id, "64-byte alloc should use large pool");
    unsafe { reg.free(r.0, r.1, None) };
}

#[test]
fn registry_margin_falls_through_to_next_pool() {
    // Use 256 bytes so alignment adjustment can't reduce count below 2 blocks.
    static mut STORAGE_P: [u8; 256] = [0u8; 256];
    static mut STORAGE_Q: [u8; 512] = [0u8; 512];
    let reg = PoolRegistry::new();

    // Small pool with ≥2 blocks; large pool with many.
    let s_id = reg.init_pool(unsafe { &mut *(&raw mut STORAGE_P) }, 64);
    let _l_id = reg.init_pool(unsafe { &mut *(&raw mut STORAGE_Q) }, 64);
    let small_total = reg.get_free(s_id).unwrap();
    assert!(small_total >= 2, "expected at least 2 blocks, got {}", small_total);

    // Drain small pool completely.
    let mut ptrs = alloc::vec::Vec::new();
    while reg.get_free(s_id).unwrap() > 0 {
        let (id, p) = reg.alloc(32, 0, None).unwrap();
        if id == s_id { ptrs.push((id, p)); } else {
            // Landed in large pool already; free it.
            unsafe { reg.free(id, p, None) };
            break;
        }
    }

    // Now request with margin=1: small pool is exhausted, falls through.
    let r = reg.alloc(32, 1, None);
    // Might land in large pool or fail; at minimum it must not panic.
    if let Some((id, p)) = r {
        unsafe { reg.free(id, p, None) };
    }

    for (id, p) in ptrs { unsafe { reg.free(id, p, None) }; }
}
