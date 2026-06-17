//! Fixed-size block memory pool (QMPool equivalent — Phase 3).
//!
//! `QMPool` manages a region of static memory as a free-list of equal-sized
//! blocks.  It is the foundation for event pools in embedded targets that
//! cannot afford heap allocation.
//!
//! # Layout
//!
//! The storage slice is divided into `(total_size / block_size)` blocks.  Each
//! free block's first `size_of::<usize>()` bytes hold the index of the next
//! free block (or `SENTINEL` for the tail).  In-use blocks have opaque
//! content owned by the caller.
//!
//! # Safety
//!
//! The unsafe internals manipulate raw pointers into the storage slice.  The
//! safe public API guarantees:
//! - Every pointer returned by `get()` points to exactly one block inside the
//!   storage and is aligned to `usize`.
//! - No block is ever returned twice without an intervening `put()`.
//! - `put()` accepts only pointers previously returned by `get()`.

use core::cell::UnsafeCell;
use core::mem;
use core::ptr;

// Block-index sentinel meaning "end of free list".
const SENTINEL: usize = usize::MAX;

/// A free-list block allocator over a `&'static mut [u8]` storage region.
///
/// All operations are `O(1)`.  The pool is not thread-safe on its own; callers
/// must ensure mutual exclusion (e.g., via a `Mutex` wrapper or critical
/// section).
pub struct QMPool {
    /// Pointer to the first byte of storage.
    storage: *mut u8,
    /// Total capacity of the storage region in bytes.
    storage_len: usize,
    /// Rounded-up block size (≥ `size_of::<usize>()`, aligned to `usize`).
    block_size: usize,
    /// Total number of blocks carved from the storage.
    num_blocks: usize,
    /// Index of the head of the free list (`SENTINEL` = empty).
    free_head: UnsafeCell<usize>,
    /// Current number of free blocks.
    free_cnt: UnsafeCell<usize>,
    /// Minimum free count ever seen (high-water-mark complement).
    free_min: UnsafeCell<usize>,
}

// SAFETY: `QMPool` is `Send` because it owns its storage exclusively and all
// interior-mutability access must be serialised by the caller.
unsafe impl Send for QMPool {}
// SAFETY: `Sync` is safe for the same reason — mutual exclusion is the
// caller's responsibility.
unsafe impl Sync for QMPool {}

impl QMPool {
    /// Creates an uninitialised pool constant.  Must be followed by `init()`.
    pub const fn uninit() -> Self {
        Self {
            storage: ptr::null_mut(),
            storage_len: 0,
            block_size: 0,
            num_blocks: 0,
            free_head: UnsafeCell::new(SENTINEL),
            free_cnt: UnsafeCell::new(0),
            free_min: UnsafeCell::new(0),
        }
    }

    /// Initialises the pool over `storage` with blocks of `block_size` bytes.
    ///
    /// `block_size` is rounded up to the next `usize` alignment boundary and
    /// must be at least 1.  Returns the number of blocks carved from the
    /// storage.
    ///
    /// # Panics
    ///
    /// Panics if `block_size == 0` or if `storage` is too small to hold even
    /// one block.
    pub fn init(&mut self, storage: &'static mut [u8], block_size: usize) -> usize {
        assert!(block_size > 0, "QMPool: block_size must be > 0");

        // Round up block_size to usize alignment so embedded next-pointers fit.
        let align = mem::size_of::<usize>();
        let blk = ((block_size + align - 1) / align) * align;
        let blk = blk.max(align); // at minimum one pointer fits

        // Align the start of storage to usize so free-list pointer writes are
        // always aligned (static [u8; N] only guarantees 1-byte alignment).
        let raw = storage.as_mut_ptr() as usize;
        let aligned_start = (raw + align - 1) & !(align - 1);
        let offset = aligned_start - raw;

        let total = storage.len().saturating_sub(offset);
        let n = total / blk;
        assert!(n > 0, "QMPool: storage too small for one aligned block");

        // SAFETY: offset < align ≤ storage.len() (checked above).
        let aligned_ptr = unsafe { storage.as_mut_ptr().add(offset) };

        self.storage = aligned_ptr;
        self.storage_len = total;
        self.block_size = blk;
        self.num_blocks = n;

        // Build the initial free list: each block[i] → block[i+1].
        unsafe {
            for i in 0..n {
                let ptr = self.storage.add(i * blk) as *mut usize;
                *ptr = if i + 1 < n { i + 1 } else { SENTINEL };
            }
        }

        // SAFETY: we are the only accessor at init time.
        unsafe {
            *self.free_head.get() = 0;
            *self.free_cnt.get() = n;
            *self.free_min.get() = n;
        }

        n
    }

    // ── Allocation ────────────────────────────────────────────────────────────

    /// Allocate one block from the pool.
    ///
    /// Returns `None` if the pool is exhausted or if fewer than `margin`
    /// blocks would remain after the allocation (useful for ensuring
    /// high-priority events always have room).
    ///
    /// # Safety contract for callers
    ///
    /// The returned pointer is valid for `block_size()` bytes and must
    /// eventually be passed back to [`put()`].
    pub fn get(&self, margin: usize) -> Option<*mut u8> {
        // SAFETY: Single-threaded access guaranteed by caller's mutex.
        unsafe {
            let free_cnt = *self.free_cnt.get();
            if free_cnt == 0 || free_cnt.saturating_sub(1) < margin {
                return None;
            }

            let head = *self.free_head.get();
            if head == SENTINEL {
                return None;
            }

            // Advance free-list head.
            let ptr = self.storage.add(head * self.block_size) as *mut usize;
            let next = *ptr;
            *self.free_head.get() = next;

            let new_free = free_cnt - 1;
            *self.free_cnt.get() = new_free;

            // Update low-watermark.
            let free_min = *self.free_min.get();
            if new_free < free_min {
                *self.free_min.get() = new_free;
            }

            Some(self.storage.add(head * self.block_size))
        }
    }

    /// Return a block to the pool.
    ///
    /// # Safety
    ///
    /// `block` must be a pointer previously returned by `get()` on THIS pool
    /// and must not have already been put back.
    pub unsafe fn put(&self, block: *mut u8) {
        // Compute block index from pointer.
        let offset = block.offset_from(self.storage) as usize;
        let idx = offset / self.block_size;
        debug_assert!(idx < self.num_blocks, "QMPool::put: pointer out of range");
        debug_assert_eq!(
            offset % self.block_size,
            0,
            "QMPool::put: misaligned pointer"
        );

        // Push block onto free-list head.
        let ptr = block as *mut usize;
        let old_head = *self.free_head.get();
        *ptr = old_head;
        *self.free_head.get() = idx;

        *self.free_cnt.get() += 1;
    }

    // ── Diagnostics ──────────────────────────────────────────────────────────

    /// Block size (in bytes, rounded up to usize alignment).
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Total number of blocks in the pool.
    pub fn num_blocks(&self) -> usize {
        self.num_blocks
    }

    /// Current number of free (available) blocks.
    pub fn get_free(&self) -> usize {
        unsafe { *self.free_cnt.get() }
    }

    /// Current number of allocated (in-use) blocks.
    pub fn get_use(&self) -> usize {
        self.num_blocks - self.get_free()
    }

    /// Minimum number of free blocks ever recorded (low watermark).
    pub fn get_min(&self) -> usize {
        unsafe { *self.free_min.get() }
    }

    /// Returns `true` if this pool can service a block of `size` bytes
    /// (i.e., `size <= self.block_size()`).
    pub fn can_serve(&self, size: usize) -> bool {
        size <= self.block_size
    }

    /// Returns `true` if the pool has been initialised.
    pub fn is_init(&self) -> bool {
        !self.storage.is_null()
    }
}
