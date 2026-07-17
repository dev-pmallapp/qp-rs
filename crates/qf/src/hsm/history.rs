//! History tracking maps conditional on the allocation model.

#[cfg(any(not(feature = "static-alloc"), feature = "std"))]
extern crate alloc;

/// Maximum number of composite states with remembered shallow history under the
/// heap-free build.
pub const HSM_HISTORY_CAP: usize = 16;

/// Max composite states with remembered history under the heap-free build.
pub const QM_HISTORY_CAP: usize = 16;

#[cfg(not(feature = "static-alloc"))]
pub type HistoryMap<V> = alloc::collections::BTreeMap<usize, V>;

#[cfg(feature = "static-alloc")]
pub type HistoryMap<V> = heapless::FnvIndexMap<usize, V, HSM_HISTORY_CAP>;
