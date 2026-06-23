//! Fixed-size block memory pool (QMPool equivalent — Phase 3).
//!
//! `QMPool` manages a region of static memory as a free-list of equal-sized
//! blocks.  It is the foundation for event pools in embedded targets that
//! cannot afford heap allocation.
//!
//! Traceability: ASR-003 (static allocation), ASR-004 (free-list error-detecting
//! codes); see `docs/traceability.md`.
//!
//! # Layout
//!
//! The storage slice is divided into `(total_size / block_size)` blocks.  Each
//! free block's first two `usize` words hold the index of the next free block
//! (or `SENTINEL` for the tail), stored **twice** as Duplicate Storage: the
//! link is read back from both words and a mismatch (bit flip / SEU) is routed
//! to [`crate::fusa::on_error`] (see `docs/FUSA.md`, Phase 3). The free-list
//! head and counters are likewise protected with Duplicate Inverse Storage
//! ([`crate::dis::Dis`]).  Because of the duplicated link, the minimum block
//! size is `2 * size_of::<usize>()`.  In-use blocks have opaque content owned by
//! the caller.
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

use crate::dis::Dis;

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
    /// Index of the head of the free list (`SENTINEL` = empty), DIS-protected:
    /// a corrupted head would hand out a block from the wrong offset.
    free_head: UnsafeCell<Dis<usize>>,
    /// Current number of free blocks, DIS-protected.
    free_cnt: UnsafeCell<Dis<usize>>,
    /// Minimum free count ever seen (high-water-mark complement), DIS-protected.
    free_min: UnsafeCell<Dis<usize>>,
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
            free_head: UnsafeCell::new(Dis::new_usize(SENTINEL)),
            free_cnt: UnsafeCell::new(Dis::new_usize(0)),
            free_min: UnsafeCell::new(Dis::new_usize(0)),
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
        // At minimum two pointers fit: the free-list link plus its Duplicate
        // Storage copy (see module docs).
        let blk = blk.max(2 * align);

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

        // Build the initial free list: each block[i] → block[i+1].  The link is
        // written twice (Duplicate Storage) so a corrupted next-index is caught
        // when the block is later allocated.
        // SAFETY: each `ptr` is the `usize`-aligned start of block `i`, which is
        // ≥ `2 * align` bytes, so both `*ptr` and `*ptr.add(1)` are in-bounds.
        unsafe {
            for i in 0..n {
                let ptr = self.storage.add(i * blk) as *mut usize;
                let next = if i + 1 < n { i + 1 } else { SENTINEL };
                *ptr = next;
                *ptr.add(1) = next;
            }
        }

        // SAFETY: we are the only accessor at init time.
        unsafe {
            *self.free_head.get() = Dis::new(0);
            *self.free_cnt.get() = Dis::new(n);
            *self.free_min.get() = Dis::new(n);
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
            let free_cnt = (*self.free_cnt.get()).get();
            if free_cnt == 0 || free_cnt.saturating_sub(1) < margin {
                return None;
            }

            let head = (*self.free_head.get()).get();
            if head == SENTINEL {
                return None;
            }

            // Advance free-list head, verifying the duplicated next-link first:
            // both copies must agree or the free list has been corrupted.
            let ptr = self.storage.add(head * self.block_size) as *mut usize;
            let next = *ptr;
            if next != *ptr.add(1) {
                crate::fusa::on_error(module_path!(), line!());
            }
            *self.free_head.get() = Dis::new(next);

            let new_free = free_cnt - 1;
            *self.free_cnt.get() = Dis::new(new_free);

            // Update low-watermark.
            let free_min = (*self.free_min.get()).get();
            if new_free < free_min {
                *self.free_min.get() = Dis::new(new_free);
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

        // Push block onto free-list head, writing the next-link in both words
        // (Duplicate Storage) so the link can be integrity-checked on the next
        // allocation.
        let ptr = block as *mut usize;
        let old_head = (*self.free_head.get()).get();
        *ptr = old_head;
        *ptr.add(1) = old_head;
        *self.free_head.get() = Dis::new(idx);

        let new_cnt = (*self.free_cnt.get()).get() + 1;
        *self.free_cnt.get() = Dis::new(new_cnt);
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
        // SAFETY: caller-serialised access; DIS read verifies integrity.
        unsafe { (*self.free_cnt.get()).get() }
    }

    /// Current number of allocated (in-use) blocks.
    pub fn get_use(&self) -> usize {
        self.num_blocks - self.get_free()
    }

    /// Minimum number of free blocks ever recorded (low watermark).
    pub fn get_min(&self) -> usize {
        // SAFETY: caller-serialised access; DIS read verifies integrity.
        unsafe { (*self.free_min.get()).get() }
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
