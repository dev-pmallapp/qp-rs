//! Synchronization primitives for extended threads.
//!
//! Provides RTOS-style synchronization primitives that allow extended threads
//! to block and coordinate with each other.
//!
//! ## Primitives
//!
//! - **Semaphore**: Counting semaphore for signaling and resource counting
//! - **Mutex**: Mutual exclusion with optional priority inheritance
//! - **MessageQueue**: FIFO queue for inter-thread communication
//! - **CondVar**: Condition variable for wait/notify patterns
//!
//! ## Storage model (functional safety, `docs/FUSA.md` Phase 2)
//!
//! On the dynamic build each primitive shares its state through an
//! `Arc<Mutex<_>>`, so cloning a handle shares the same primitive. Under the
//! heap-free `static-alloc` build the state lives in application-owned `static`
//! storage and the handle is a `&'static Mutex<_>` (cloning copies the
//! reference). The convenience constructors (`new`, `binary`, …) leak on
//! `static-alloc` + `std` for host tests; genuine no-`std` targets build the
//! state in a `static` and use [`Semaphore::from_static`] and friends.

use core::fmt;
use core::time::Duration;

use crate::scheduler::{QxkScheduler, SchedStatus};
#[cfg(not(feature = "static-alloc"))]
use crate::sync::Arc;
use crate::sync::Mutex;
use crate::thread::{ThreadId, ThreadPriority};

#[cfg(feature = "qs")]
use qf::TraceHook;
#[cfg(feature = "qs")]
use qs::records::qxk as rec;

// Fallback record-ID module when the `qs` feature is disabled.
// The values are never used (emit() is a no-op), but the names must resolve.
#[cfg(not(feature = "qs"))]
mod rec {
    pub const SEM_TAKE:   u8 = 71;
    pub const SEM_BLOCK:  u8 = 72;
    pub const SEM_SIGNAL: u8 = 73;
    pub const MTX_LOCK:   u8 = 75;
    pub const MTX_BLOCK:  u8 = 76;
    pub const MTX_UNLOCK: u8 = 77;
}

/// Waiter list storage. Dynamic: heap [`Vec`]; `static-alloc`: heap-free
/// [`heapless::Vec`] bounded by [`crate::MAX_WAITERS`].
#[cfg(not(feature = "static-alloc"))]
type WaitVec = alloc::vec::Vec<WaitingThread>;
#[cfg(feature = "static-alloc")]
type WaitVec = heapless::Vec<WaitingThread, { crate::MAX_WAITERS }>;

/// Shared primitive state. Dynamic: `Arc<Mutex<_>>` (cloning shares); under
/// `static-alloc`: `&'static Mutex<_>` (the state lives in `static` storage).
#[cfg(not(feature = "static-alloc"))]
type Shared<T> = Arc<Mutex<T>>;
#[cfg(feature = "static-alloc")]
type Shared<T> = &'static Mutex<T>;

/// Push a waiter, faulting (crash-only) if the heap-free wait-list is full.
#[inline]
fn push_waiter(v: &mut WaitVec, w: WaitingThread) {
    #[cfg(not(feature = "static-alloc"))]
    v.push(w);
    #[cfg(feature = "static-alloc")]
    if v.push(w).is_err() {
        qf::fusa::on_error(module_path!(), line!());
    }
}

/// Remove and return the highest-priority waiter, if any. Build-agnostic
/// (uses `swap_remove`, available on both `Vec` and `heapless::Vec`).
fn take_highest(v: &mut WaitVec) -> Option<WaitingThread> {
    if v.is_empty() {
        return None;
    }
    let mut best = 0;
    for i in 1..v.len() {
        if v[i].priority > v[best].priority {
            best = i;
        }
    }
    Some(v.swap_remove(best))
}

/// Clone a shared handle: `Arc` refcount bump (dynamic) or pointer copy (`static-alloc`).
#[inline]
fn clone_shared<T>(s: &Shared<T>) -> Shared<T> {
    #[cfg(not(feature = "static-alloc"))]
    {
        Arc::clone(s)
    }
    #[cfg(feature = "static-alloc")]
    {
        // Copy the `&'static` reference out. The explicit deref is required:
        // returning `s` would deref-coerce to a shorter-lived `&Mutex<T>`, not
        // the `&'static Mutex<T>` the return type demands.
        #[allow(clippy::explicit_auto_deref)]
        *s
    }
}

/// Allocate shared state for a convenience constructor. Dynamic: `Arc`;
/// `static-alloc` + `std`: leaked `Box` (host tests). Absent on no-`std`
/// heap-free targets, which use the `from_static` constructors instead.
#[cfg(any(not(feature = "static-alloc"), feature = "std"))]
#[inline]
fn share<T>(inner: T) -> Shared<T> {
    #[cfg(not(feature = "static-alloc"))]
    {
        Arc::new(Mutex::new(inner))
    }
    #[cfg(all(feature = "static-alloc", feature = "std"))]
    {
        alloc::boxed::Box::leak(alloc::boxed::Box::new(Mutex::new(inner)))
    }
}

/// Stable address of the shared state, used as a QS object id.
#[cfg(feature = "qs")]
#[inline]
fn shared_ptr<T>(s: &Shared<T>) -> u64 {
    #[cfg(not(feature = "static-alloc"))]
    {
        Arc::as_ptr(s) as u64
    }
    #[cfg(feature = "static-alloc")]
    {
        (*s) as *const Mutex<T> as u64
    }
}

/// Error types for synchronization primitives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncError {
    /// Operation timed out.
    Timeout,
    /// Semaphore count would overflow.
    Overflow,
    /// Queue is full.
    QueueFull,
    /// Queue is empty.
    QueueEmpty,
    /// Invalid operation (e.g., unlock by non-owner).
    InvalidOperation,
    /// Operation would block, thread suspended by scheduler.
    WouldBlock,
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => write!(f, "operation timed out"),
            Self::Overflow => write!(f, "semaphore count overflow"),
            Self::QueueFull => write!(f, "message queue is full"),
            Self::QueueEmpty => write!(f, "message queue is empty"),
            Self::InvalidOperation => write!(f, "invalid operation"),
            Self::WouldBlock => write!(f, "operation would block"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SyncError {}

/// Result type returned by the QXK synchronization primitives.
pub type SyncResult<T> = Result<T, SyncError>;

/// Waiting thread information.
#[derive(Debug, Clone, Copy)]
struct WaitingThread {
    id: ThreadId,
    priority: u8,
}

impl WaitingThread {
    fn new(id: ThreadId, priority: u8) -> Self {
        Self { id, priority }
    }
}

/// Counting semaphore for signaling between threads.
///
/// A semaphore maintains a count and allows threads to wait until the count
/// is positive, then decrements it. Other threads can signal to increment
/// the count and wake waiting threads.
///
/// # Example
///
/// ```ignore
/// let sem = Semaphore::new(0); // Initially no signals
///
/// // Thread 1: Wait for signal
/// sem.wait()?; // Blocks until signaled
///
/// // Thread 2: Send signal
/// sem.signal()?; // Wakes thread 1
/// ```
pub struct Semaphore {
    inner: Shared<SemaphoreInner>,
    #[cfg(feature = "qs")]
    trace: Option<TraceHook>,
}

/// Internal semaphore state. Public so that no-`std` heap-free targets can
/// place it in `static` storage for [`Semaphore::from_static`].
pub struct SemaphoreInner {
    count: usize,
    max_count: usize,
    waiting: WaitVec,
}

impl SemaphoreInner {
    /// Creates semaphore state with the given initial and maximum counts.
    pub const fn new(initial_count: usize, max_count: usize) -> Self {
        Self {
            count: initial_count,
            max_count,
            waiting: WaitVec::new(),
        }
    }
}

impl Semaphore {
    /// Creates a new semaphore with the given initial count.
    ///
    /// # Parameters
    /// - `initial_count`: Starting value for the semaphore
    #[cfg(any(not(feature = "static-alloc"), feature = "std"))]
    pub fn new(initial_count: usize) -> Self {
        Self::with_max(initial_count, usize::MAX)
    }

    /// Creates a new semaphore with initial and maximum counts.
    ///
    /// # Parameters
    /// - `initial_count`: Starting value for the semaphore
    /// - `max_count`: Maximum allowed value (for overflow protection)
    #[cfg(any(not(feature = "static-alloc"), feature = "std"))]
    pub fn with_max(initial_count: usize, max_count: usize) -> Self {
        Self {
            inner: share(SemaphoreInner::new(initial_count, max_count)),
            #[cfg(feature = "qs")]
            trace: None,
        }
    }

    /// Creates a binary semaphore (max count = 1).
    #[cfg(any(not(feature = "static-alloc"), feature = "std"))]
    pub fn binary() -> Self {
        Self::with_max(0, 1)
    }

    /// Builds a semaphore handle over caller-owned `static` state (heap-free).
    #[cfg(feature = "static-alloc")]
    pub fn from_static(inner: &'static Mutex<SemaphoreInner>) -> Self {
        Self {
            inner,
            #[cfg(feature = "qs")]
            trace: None,
        }
    }

    /// Attach a QS trace hook.  Records are emitted on wait/signal.
    #[cfg(feature = "qs")]
    pub fn set_trace(&mut self, hook: Option<TraceHook>) {
        self.trace = hook;
    }

    #[cfg(feature = "qs")]
    fn emit(&self, record_id: u8, thread_prio: u8, count: usize) {
        if let Some(ref hook) = self.trace {
            let ptr = shared_ptr(&self.inner);
            let mut payload = [0u8; 11];
            payload[..8].copy_from_slice(&ptr.to_le_bytes());
            payload[8] = thread_prio;
            let c = count.min(u16::MAX as usize) as u16;
            payload[9..11].copy_from_slice(&c.to_le_bytes());
            let _ = hook(record_id, &payload, true);
        }
    }

    #[cfg(not(feature = "qs"))]
    #[inline(always)]
    fn emit(&self, _record_id: u8, _thread_prio: u8, _count: usize) {}

    /// Waits for the semaphore to become available (non-blocking check).
    ///
    /// Returns true if the semaphore was acquired, false if it would block.
    pub fn try_wait(&self) -> bool {
        let mut inner = self.inner.lock();
        if inner.count > 0 {
            inner.count -= 1;
            true
        } else {
            false
        }
    }

    /// Waits for the semaphore, blocking until available.
    ///
    /// If the semaphore count is > 0, decrements and returns Ok.
    /// Otherwise, registers the thread as waiting and returns WouldBlock.
    /// The scheduler will unblock the thread when signal() is called.
    pub fn wait(&self, thread: ThreadId, priority: u8, scheduler: &QxkScheduler) -> SyncResult<()> {
        let (acquired, count) = {
            let mut inner = self.inner.lock();
            if inner.count > 0 {
                inner.count -= 1;
                (true, inner.count)
            } else {
                push_waiter(&mut inner.waiting, WaitingThread::new(thread, priority));
                (false, inner.count)
            }
        };

        if acquired {
            self.emit(rec::SEM_TAKE, priority, count);
            Ok(())
        } else {
            self.emit(rec::SEM_BLOCK, priority, count);
            scheduler.block_thread(thread);
            Err(SyncError::WouldBlock)
        }
    }

    /// Waits for the semaphore with a timeout.
    pub fn wait_timeout(&self, _timeout: Duration) -> SyncResult<()> {
        // Simplified: just try once
        if self.try_wait() {
            Ok(())
        } else {
            Err(SyncError::Timeout)
        }
    }

    /// Signals the semaphore, incrementing the count.
    ///
    /// Wakes the highest priority waiting thread if any are blocked.
    pub fn signal(&self, scheduler: &QxkScheduler) -> SyncResult<()> {
        let (count, woken_thread) = {
            let mut inner = self.inner.lock();
            if inner.count >= inner.max_count {
                return Err(SyncError::Overflow);
            }
            inner.count += 1;
            let count = inner.count;

            // Wake highest priority waiter
            let woken = take_highest(&mut inner.waiting).map(|w| (w.id, w.priority));
            (count, woken)
        };

        self.emit(rec::SEM_SIGNAL, 0, count);

        // Unblock in scheduler (outside lock to avoid deadlock)
        if let Some((id, priority)) = woken_thread {
            scheduler.unblock_thread(id, ThreadPriority(priority));
        }

        Ok(())
    }

    /// Signals the semaphore from an ISR context.
    ///
    /// Identical to `signal()` but asserts that the caller is inside an ISR
    /// (i.e. `qk_isr_entry!()` has been called).  Corresponds to
    /// `QXSemaphore::signalFromISR()` in QP/C++.
    pub fn signal_from_isr(&self, scheduler: &QxkScheduler) -> SyncResult<()> {
        debug_assert!(qf::isr::in_isr(), "signal_from_isr called outside ISR context");
        self.signal(scheduler)
    }

    /// Returns the current count.
    pub fn count(&self) -> usize {
        self.inner.lock().count
    }

    /// Registers a thread as waiting (for scheduler integration).
    pub fn register_waiter(&self, thread: ThreadId, priority: u8) {
        let mut inner = self.inner.lock();
        push_waiter(&mut inner.waiting, WaitingThread::new(thread, priority));
    }
}

impl Clone for Semaphore {
    fn clone(&self) -> Self {
        Self {
            inner: clone_shared(&self.inner),
            #[cfg(feature = "qs")]
            trace: self.trace.clone(),
        }
    }
}

/// Mutual exclusion lock for protecting shared data.
///
/// A mutex ensures only one thread can access protected data at a time.
/// Optionally supports priority inheritance to prevent priority inversion.
pub struct MutexPrim {
    inner: Shared<MutexInner>,
    #[cfg(feature = "qs")]
    trace: Option<TraceHook>,
}

/// Internal mutex state. Public so that no-`std` heap-free targets can place it
/// in `static` storage for [`MutexPrim::from_static`].
pub struct MutexInner {
    locked: bool,
    owner: Option<ThreadId>,
    original_priority: Option<u8>,
    /// Priority ceiling.  When set, the scheduler is locked to this ceiling
    /// while the mutex is held, preventing lower-priority preemption.
    ceiling: Option<u8>,
    /// Saved scheduler lock status applied when ceiling was raised; restored on unlock.
    ceiling_sched_status: Option<SchedStatus>,
    waiting: WaitVec,
}

impl MutexInner {
    /// Creates mutex state with no priority ceiling.
    pub const fn new() -> Self {
        Self {
            locked: false,
            owner: None,
            original_priority: None,
            ceiling: None,
            ceiling_sched_status: None,
            waiting: WaitVec::new(),
        }
    }

    /// Creates mutex state with a priority ceiling.
    pub const fn with_ceiling(ceiling: u8) -> Self {
        Self {
            locked: false,
            owner: None,
            original_priority: None,
            ceiling: Some(ceiling),
            ceiling_sched_status: None,
            waiting: WaitVec::new(),
        }
    }
}

impl Default for MutexInner {
    fn default() -> Self {
        Self::new()
    }
}

impl MutexPrim {
    /// Creates a new mutex.
    #[cfg(any(not(feature = "static-alloc"), feature = "std"))]
    pub fn new() -> Self {
        Self {
            inner: share(MutexInner::new()),
            #[cfg(feature = "qs")]
            trace: None,
        }
    }

    /// Creates a priority-ceiling mutex.
    ///
    /// While this mutex is held the scheduler is locked at `ceiling`, preventing
    /// any task with priority ≤ `ceiling` from preempting the holder.  This
    /// eliminates unbounded priority inversion without full priority inheritance.
    /// Corresponds to `QXMutex` with `QF_QMPOOL_CTR_SIZE` ceiling in QP/C++.
    #[cfg(any(not(feature = "static-alloc"), feature = "std"))]
    pub fn with_ceiling(ceiling: u8) -> Self {
        Self {
            inner: share(MutexInner::with_ceiling(ceiling)),
            #[cfg(feature = "qs")]
            trace: None,
        }
    }

    /// Builds a mutex handle over caller-owned `static` state (heap-free).
    #[cfg(feature = "static-alloc")]
    pub fn from_static(inner: &'static Mutex<MutexInner>) -> Self {
        Self {
            inner,
            #[cfg(feature = "qs")]
            trace: None,
        }
    }

    /// Returns the priority ceiling, if one was configured.
    pub fn ceiling(&self) -> Option<u8> {
        self.inner.lock().ceiling
    }

    /// Attach a QS trace hook.  Records are emitted on lock/unlock.
    #[cfg(feature = "qs")]
    pub fn set_trace(&mut self, hook: Option<TraceHook>) {
        self.trace = hook;
    }

    #[cfg(feature = "qs")]
    fn emit(&self, record_id: u8, thread_prio: u8) {
        if let Some(ref hook) = self.trace {
            let ptr = shared_ptr(&self.inner);
            let mut payload = [0u8; 9];
            payload[..8].copy_from_slice(&ptr.to_le_bytes());
            payload[8] = thread_prio;
            let _ = hook(record_id, &payload, true);
        }
    }

    #[cfg(not(feature = "qs"))]
    #[inline(always)]
    fn emit(&self, _record_id: u8, _thread_prio: u8) {}

    /// Attempts to lock the mutex without blocking.
    pub fn try_lock(&self, thread: ThreadId) -> bool {
        let mut inner = self.inner.lock();
        if !inner.locked {
            inner.locked = true;
            inner.owner = Some(thread);
            true
        } else {
            false
        }
    }

    /// Locks the mutex, blocking until available.
    ///
    /// If the mutex is unlocked, acquires it and returns Ok.  If a priority
    /// ceiling was configured, the scheduler is locked to that ceiling for the
    /// duration of the lock (preventing lower-priority preemption).
    /// Otherwise, registers as waiting and returns WouldBlock.
    pub fn lock(&self, thread: ThreadId, priority: u8, scheduler: &QxkScheduler) -> SyncResult<()> {
        let (acquired, ceiling) = {
            let mut inner = self.inner.lock();
            if !inner.locked {
                inner.locked = true;
                inner.owner = Some(thread);
                inner.original_priority = Some(priority);
                (true, inner.ceiling)
            } else {
                push_waiter(&mut inner.waiting, WaitingThread::new(thread, priority));
                (false, None)
            }
        };

        if acquired {
            // Apply priority ceiling: lock the scheduler so no task with
            // priority <= ceiling can preempt the mutex holder.
            if let Some(c) = ceiling {
                let status = scheduler.lock(c);
                self.inner.lock().ceiling_sched_status = Some(status);
            }
            self.emit(rec::MTX_LOCK, priority);
            Ok(())
        } else {
            self.emit(rec::MTX_BLOCK, priority);
            scheduler.block_thread(thread);
            Err(SyncError::WouldBlock)
        }
    }

    /// Unlocks the mutex.
    ///
    /// Wakes the highest priority waiting thread if any are blocked.
    pub fn unlock(&self, thread: ThreadId, scheduler: &QxkScheduler) -> SyncResult<()> {
        let thread_prio;
        let ceiling_status;
        let woken_thread = {
            let mut inner = self.inner.lock();

            if inner.owner != Some(thread) {
                return Err(SyncError::InvalidOperation);
            }

            thread_prio = inner.original_priority.unwrap_or(0);
            ceiling_status = inner.ceiling_sched_status.take();
            inner.locked = false;
            inner.owner = None;
            inner.original_priority = None;

            // Wake highest priority waiter
            take_highest(&mut inner.waiting).map(|w| (w.id, w.priority))
        };

        // Restore ceiling lock before emitting trace or unblocking waiters.
        if let Some(status) = ceiling_status {
            scheduler.unlock(status);
        }
        self.emit(rec::MTX_UNLOCK, thread_prio);

        // Unblock in scheduler (outside lock)
        if let Some((id, priority)) = woken_thread {
            scheduler.unblock_thread(id, ThreadPriority(priority));
        }

        Ok(())
    }

    /// Unlocks the mutex from an ISR context.
    ///
    /// Identical to `unlock()` but asserts ISR context.  Corresponds to
    /// `QXMutex::unlockFromISR()` in QP/C++.
    pub fn unlock_from_isr(&self, thread: ThreadId, scheduler: &QxkScheduler) -> SyncResult<()> {
        debug_assert!(qf::isr::in_isr(), "unlock_from_isr called outside ISR context");
        self.unlock(thread, scheduler)
    }

    /// Checks if the mutex is currently locked.
    pub fn is_locked(&self) -> bool {
        self.inner.lock().locked
    }

    /// Returns the current owner thread ID.
    pub fn owner(&self) -> Option<ThreadId> {
        self.inner.lock().owner
    }

    /// Registers a thread as waiting.
    pub fn register_waiter(&self, thread: ThreadId, priority: u8) {
        let mut inner = self.inner.lock();
        push_waiter(&mut inner.waiting, WaitingThread::new(thread, priority));
    }
}

#[cfg(any(not(feature = "static-alloc"), feature = "std"))]
impl Default for MutexPrim {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MutexPrim {
    fn clone(&self) -> Self {
        Self {
            inner: clone_shared(&self.inner),
            #[cfg(feature = "qs")]
            trace: self.trace.clone(),
        }
    }
}

/// FIFO message queue for inter-thread communication.
///
/// Allows threads to send and receive typed messages. Receivers can block
/// waiting for messages. The capacity is the const generic `N`: heap-free
/// builds back the queue with a `heapless::Deque<T, N>`, the dynamic build with
/// a `VecDeque<T>` bounded to `N`.
pub struct MessageQueue<T: 'static, const N: usize> {
    inner: Shared<MessageQueueInner<T, N>>,
}

/// Internal message-queue state. Public so that no-`std` heap-free targets can
/// place it in `static` storage for [`MessageQueue::from_static`].
pub struct MessageQueueInner<T, const N: usize> {
    #[cfg(not(feature = "static-alloc"))]
    queue: alloc::collections::VecDeque<T>,
    #[cfg(feature = "static-alloc")]
    queue: heapless::Deque<T, N>,
    waiting_receivers: WaitVec,
    waiting_senders: WaitVec,
    // The dynamic queue does not embed `N`; keep the parameter live.
    #[cfg(not(feature = "static-alloc"))]
    _cap: core::marker::PhantomData<[(); N]>,
}

impl<T, const N: usize> MessageQueueInner<T, N> {
    /// Creates empty message-queue state.
    pub const fn new() -> Self {
        Self {
            #[cfg(not(feature = "static-alloc"))]
            queue: alloc::collections::VecDeque::new(),
            #[cfg(feature = "static-alloc")]
            queue: heapless::Deque::new(),
            waiting_receivers: WaitVec::new(),
            waiting_senders: WaitVec::new(),
            #[cfg(not(feature = "static-alloc"))]
            _cap: core::marker::PhantomData,
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.queue.len()
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.queue.len() >= N
    }

    /// Pushes to the back, returning `Err(msg)` if the queue is full.
    #[inline]
    fn try_push(&mut self, msg: T) -> Result<(), T> {
        #[cfg(not(feature = "static-alloc"))]
        {
            if self.queue.len() >= N {
                return Err(msg);
            }
            self.queue.push_back(msg);
            Ok(())
        }
        #[cfg(feature = "static-alloc")]
        {
            self.queue.push_back(msg)
        }
    }

    #[inline]
    fn pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }
}

impl<T, const N: usize> Default for MessageQueueInner<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static, const N: usize> MessageQueue<T, N> {
    /// Creates a new message queue with capacity `N`.
    #[cfg(any(not(feature = "static-alloc"), feature = "std"))]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            inner: share(MessageQueueInner::new()),
        }
    }

    /// Builds a queue handle over caller-owned `static` state (heap-free).
    #[cfg(feature = "static-alloc")]
    pub fn from_static(inner: &'static Mutex<MessageQueueInner<T, N>>) -> Self {
        Self { inner }
    }

    /// Sends a message to the queue without blocking.
    ///
    /// Wakes a waiting receiver if any are blocked.
    pub fn try_send(&self, message: T, scheduler: &QxkScheduler) -> SyncResult<()> {
        let woken_receiver = {
            let mut inner = self.inner.lock();
            if inner.try_push(message).is_err() {
                return Err(SyncError::QueueFull);
            }
            take_highest(&mut inner.waiting_receivers).map(|w| (w.id, w.priority))
        };

        // Unblock receiver (outside lock)
        if let Some((id, priority)) = woken_receiver {
            scheduler.unblock_thread(id, ThreadPriority(priority));
        }

        Ok(())
    }

    /// Receives a message from the queue without blocking.
    pub fn try_receive(&self) -> SyncResult<T> {
        let mut inner = self.inner.lock();
        inner.pop().ok_or(SyncError::QueueEmpty)
    }

    /// Sends a message, blocking if queue is full.
    ///
    /// If queue has space, sends immediately. Otherwise, registers as waiting
    /// sender and returns WouldBlock.
    pub fn send(&self, message: T, thread: ThreadId, priority: u8, scheduler: &QxkScheduler) -> SyncResult<()> {
        let woken_receiver = {
            let mut inner = self.inner.lock();
            match inner.try_push(message) {
                Ok(()) => take_highest(&mut inner.waiting_receivers).map(|w| (w.id, w.priority)),
                Err(_msg) => {
                    // Queue full, register as waiting sender
                    push_waiter(&mut inner.waiting_senders, WaitingThread::new(thread, priority));
                    drop(inner);
                    scheduler.block_thread(thread);
                    return Err(SyncError::WouldBlock);
                }
            }
        };

        // Unblock receiver (outside lock)
        if let Some((id, priority)) = woken_receiver {
            scheduler.unblock_thread(id, ThreadPriority(priority));
        }

        Ok(())
    }

    /// Receives a message, blocking if queue is empty.
    ///
    /// If queue has messages, receives immediately. Otherwise, registers as
    /// waiting receiver and returns WouldBlock.
    pub fn receive(&self, thread: ThreadId, priority: u8, scheduler: &QxkScheduler) -> SyncResult<T> {
        let (message, woken_sender) = {
            let mut inner = self.inner.lock();
            if let Some(msg) = inner.pop() {
                // Wake one waiting sender
                let woken = take_highest(&mut inner.waiting_senders).map(|w| (w.id, w.priority));
                (Some(msg), woken)
            } else {
                // Queue empty, register as waiting receiver
                push_waiter(&mut inner.waiting_receivers, WaitingThread::new(thread, priority));
                (None, None)
            }
        };

        if let Some(msg) = message {
            // Unblock sender (outside lock)
            if let Some((id, priority)) = woken_sender {
                scheduler.unblock_thread(id, ThreadPriority(priority));
            }
            Ok(msg)
        } else {
            scheduler.block_thread(thread);
            Err(SyncError::WouldBlock)
        }
    }

    /// Returns the number of messages currently in the queue.
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    /// Returns true if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if the queue is full.
    pub fn is_full(&self) -> bool {
        self.inner.lock().is_full()
    }

    /// Returns the capacity of the queue.
    pub fn capacity(&self) -> usize {
        N
    }
}

impl<T: 'static, const N: usize> Clone for MessageQueue<T, N> {
    fn clone(&self) -> Self {
        Self {
            inner: clone_shared(&self.inner),
        }
    }
}

/// Condition variable for thread coordination.
///
/// Allows threads to wait for a condition to become true, and other threads
/// to notify waiting threads when the condition changes.
pub struct CondVar {
    inner: Shared<CondVarInner>,
}

/// Internal condition-variable state. Public so that no-`std` heap-free targets
/// can place it in `static` storage for [`CondVar::from_static`].
pub struct CondVarInner {
    waiting: WaitVec,
}

impl CondVarInner {
    /// Creates empty condition-variable state.
    pub const fn new() -> Self {
        Self {
            waiting: WaitVec::new(),
        }
    }
}

impl Default for CondVarInner {
    fn default() -> Self {
        Self::new()
    }
}

impl CondVar {
    /// Creates a new condition variable.
    #[cfg(any(not(feature = "static-alloc"), feature = "std"))]
    pub fn new() -> Self {
        Self {
            inner: share(CondVarInner::new()),
        }
    }

    /// Builds a condvar handle over caller-owned `static` state (heap-free).
    #[cfg(feature = "static-alloc")]
    pub fn from_static(inner: &'static Mutex<CondVarInner>) -> Self {
        Self { inner }
    }

    /// Waits for notification.
    ///
    /// Registers the thread as waiting and blocks it in the scheduler.
    /// Returns WouldBlock to indicate the thread is suspended.
    pub fn wait(&self, thread: ThreadId, priority: u8, scheduler: &QxkScheduler) -> SyncResult<()> {
        {
            let mut inner = self.inner.lock();
            push_waiter(&mut inner.waiting, WaitingThread::new(thread, priority));
        }
        scheduler.block_thread(thread);
        Err(SyncError::WouldBlock)
    }

    /// Notifies one waiting thread (highest priority).
    pub fn notify_one(&self, scheduler: &QxkScheduler) {
        let woken_thread = {
            let mut inner = self.inner.lock();
            take_highest(&mut inner.waiting).map(|w| (w.id, w.priority))
        };

        if let Some((id, priority)) = woken_thread {
            scheduler.unblock_thread(id, ThreadPriority(priority));
        }
    }

    /// Notifies all waiting threads.
    pub fn notify_all(&self, scheduler: &QxkScheduler) {
        let waiting = {
            let mut inner = self.inner.lock();
            core::mem::take(&mut inner.waiting)
        };

        for woken in waiting {
            scheduler.unblock_thread(woken.id, ThreadPriority(woken.priority));
        }
    }

    /// Returns the number of threads waiting.
    pub fn waiting_count(&self) -> usize {
        self.inner.lock().waiting.len()
    }
}

#[cfg(any(not(feature = "static-alloc"), feature = "std"))]
impl Default for CondVar {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CondVar {
    fn clone(&self) -> Self {
        Self {
            inner: clone_shared(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semaphore_signal_and_wait() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let sem = Semaphore::new(0);
        assert_eq!(sem.count(), 0);

        sem.signal(&sched).expect("signal should succeed");
        assert_eq!(sem.count(), 1);

        assert!(sem.try_wait());
        assert_eq!(sem.count(), 0);
    }

    #[test]
    fn binary_semaphore_overflow() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let sem = Semaphore::binary();
        sem.signal(&sched).expect("first signal ok");
        assert!(matches!(sem.signal(&sched), Err(SyncError::Overflow)));
    }

    #[test]
    fn semaphore_try_wait_fails_when_empty() {
        let sem = Semaphore::new(0);
        assert!(!sem.try_wait());
    }

    #[test]
    fn mutex_lock_unlock() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let mutex = MutexPrim::new();
        let thread1 = ThreadId(1);
        let thread2 = ThreadId(2);

        assert!(mutex.try_lock(thread1));
        assert!(mutex.is_locked());
        assert_eq!(mutex.owner(), Some(thread1));

        // Different thread cannot lock
        assert!(!mutex.try_lock(thread2));

        mutex.unlock(thread1, &sched).expect("unlock should succeed");
        assert!(!mutex.is_locked());
        assert_eq!(mutex.owner(), None);

        // Now thread2 can lock
        assert!(mutex.try_lock(thread2));
    }

    #[test]
    fn mutex_unlock_by_non_owner_fails() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let mutex = MutexPrim::new();
        let thread1 = ThreadId(1);
        let thread2 = ThreadId(2);

        mutex.try_lock(thread1);
        assert!(matches!(
            mutex.unlock(thread2, &sched),
            Err(SyncError::InvalidOperation)
        ));
    }

    #[test]
    fn message_queue_send_receive() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let queue: MessageQueue<u32, 3> = MessageQueue::new();

        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);

        queue.try_send(1, &sched).expect("send 1");
        queue.try_send(2, &sched).expect("send 2");
        queue.try_send(3, &sched).expect("send 3");

        assert!(queue.is_full());
        assert!(matches!(queue.try_send(4, &sched), Err(SyncError::QueueFull)));

        assert_eq!(queue.try_receive().unwrap(), 1);
        assert_eq!(queue.try_receive().unwrap(), 2);
        assert_eq!(queue.try_receive().unwrap(), 3);

        assert!(queue.is_empty());
        assert!(matches!(queue.try_receive(), Err(SyncError::QueueEmpty)));
    }

    #[test]
    fn message_queue_fifo_order() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let queue: MessageQueue<&str, 5> = MessageQueue::new();

        queue.try_send("first", &sched).unwrap();
        queue.try_send("second", &sched).unwrap();
        queue.try_send("third", &sched).unwrap();

        assert_eq!(queue.try_receive().unwrap(), "first");
        assert_eq!(queue.try_receive().unwrap(), "second");
        assert_eq!(queue.try_receive().unwrap(), "third");
    }

    #[test]
    fn condvar_notify() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let cv = CondVar::new();
        let thread1 = ThreadId(1);

        assert_eq!(cv.waiting_count(), 0);

        let _ = cv.wait(thread1, 5, &sched);
        assert_eq!(cv.waiting_count(), 1);

        cv.notify_one(&sched);
        assert_eq!(cv.waiting_count(), 0);
    }

    #[test]
    fn condvar_notify_all() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let cv = CondVar::new();

        let _ = cv.wait(ThreadId(1), 3, &sched);
        let _ = cv.wait(ThreadId(2), 5, &sched);
        let _ = cv.wait(ThreadId(3), 2, &sched);
        assert_eq!(cv.waiting_count(), 3);

        cv.notify_all(&sched);
        assert_eq!(cv.waiting_count(), 0);
    }

    // ── Phase 7: priority ceiling + ISR-safe methods ──────────────────────────

    #[test]
    fn mutex_with_ceiling_locks_scheduler() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let mutex = MutexPrim::with_ceiling(10);
        assert_eq!(mutex.ceiling(), Some(10));
        let thread = ThreadId(1);

        // Before lock: a priority-3 task is schedulable.
        sched.mark_ao_ready(3);
        assert!(matches!(
            sched.plan_next(),
            crate::scheduler::ScheduleMode::ActiveObject { priority: 3 }
        ));

        // Lock the mutex: scheduler ceiling should be raised to 10.
        mutex.lock(thread, 3, &sched).expect("lock should succeed");
        // Priority-3 task is now blocked by ceiling-10 lock.
        assert!(matches!(sched.plan_next(), crate::scheduler::ScheduleMode::Idle));

        // Unlock: ceiling is restored, priority-3 becomes schedulable again.
        mutex.unlock(thread, &sched).expect("unlock should succeed");
        assert!(matches!(
            sched.plan_next(),
            crate::scheduler::ScheduleMode::ActiveObject { priority: 3 }
        ));
    }

    #[test]
    fn mutex_without_ceiling_does_not_lock_scheduler() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let mutex = MutexPrim::new();
        assert_eq!(mutex.ceiling(), None);
        let thread = ThreadId(2);

        sched.mark_ao_ready(5);
        mutex.lock(thread, 5, &sched).expect("lock");
        // No ceiling: AO is still schedulable.
        assert!(matches!(
            sched.plan_next(),
            crate::scheduler::ScheduleMode::ActiveObject { priority: 5 }
        ));
        mutex.unlock(thread, &sched).expect("unlock");
    }

    #[test]
    fn signal_from_isr_increments_count() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let sem = Semaphore::new(0);

        qf::qk_isr_entry!();
        sem.signal_from_isr(&sched).expect("signal_from_isr ok");
        qf::qk_isr_exit!();

        assert_eq!(sem.count(), 1);
    }

    #[test]
    fn unlock_from_isr_releases_mutex() {
        let sched = crate::scheduler::QxkScheduler::new(None);
        let mutex = MutexPrim::new();
        let thread = ThreadId(3);

        mutex.try_lock(thread);
        assert!(mutex.is_locked());

        qf::qk_isr_entry!();
        mutex.unlock_from_isr(thread, &sched).expect("unlock_from_isr ok");
        qf::qk_isr_exit!();

        assert!(!mutex.is_locked());
    }
}
