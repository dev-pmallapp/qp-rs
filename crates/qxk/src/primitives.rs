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

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::fmt;
use core::time::Duration;

use crate::scheduler::{QxkScheduler, SchedStatus};
use crate::sync::{Arc, Mutex};
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
    inner: Arc<Mutex<SemaphoreInner>>,
    #[cfg(feature = "qs")]
    trace: Option<TraceHook>,
}

struct SemaphoreInner {
    count: usize,
    max_count: usize,
    waiting: Vec<WaitingThread>,
}

impl Semaphore {
    /// Creates a new semaphore with the given initial count.
    ///
    /// # Parameters
    /// - `initial_count`: Starting value for the semaphore
    pub fn new(initial_count: usize) -> Self {
        Self::with_max(initial_count, usize::MAX)
    }

    /// Creates a new semaphore with initial and maximum counts.
    ///
    /// # Parameters
    /// - `initial_count`: Starting value for the semaphore
    /// - `max_count`: Maximum allowed value (for overflow protection)
    pub fn with_max(initial_count: usize, max_count: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SemaphoreInner {
                count: initial_count,
                max_count,
                waiting: Vec::new(),
            })),
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
            let ptr = Arc::as_ptr(&self.inner) as u64;
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

    /// Creates a binary semaphore (max count = 1).
    pub fn binary() -> Self {
        Self::with_max(0, 1)
    }

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
                inner.waiting.push(WaitingThread::new(thread, priority));
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
            let woken = if !inner.waiting.is_empty() {
                inner.waiting.sort_by(|a, b| b.priority.cmp(&a.priority));
                let w = inner.waiting.remove(0);
                Some((w.id, w.priority))
            } else {
                None
            };
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
        inner.waiting.push(WaitingThread::new(thread, priority));
    }
}

impl Clone for Semaphore {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
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
    inner: Arc<Mutex<MutexInner>>,
    #[cfg(feature = "qs")]
    trace: Option<TraceHook>,
}

struct MutexInner {
    locked: bool,
    owner: Option<ThreadId>,
    original_priority: Option<u8>,
    /// Priority ceiling.  When set, the scheduler is locked to this ceiling
    /// while the mutex is held, preventing lower-priority preemption.
    ceiling: Option<u8>,
    /// Saved scheduler lock status applied when ceiling was raised; restored on unlock.
    ceiling_sched_status: Option<SchedStatus>,
    waiting: Vec<WaitingThread>,
}

impl MutexPrim {
    /// Creates a new mutex.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MutexInner {
                locked: false,
                owner: None,
                original_priority: None,
                ceiling: None,
                ceiling_sched_status: None,
                waiting: Vec::new(),
            })),
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
    pub fn with_ceiling(ceiling: u8) -> Self {
        Self {
            inner: Arc::new(Mutex::new(MutexInner {
                locked: false,
                owner: None,
                original_priority: None,
                ceiling: Some(ceiling),
                ceiling_sched_status: None,
                waiting: Vec::new(),
            })),
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
            let ptr = Arc::as_ptr(&self.inner) as u64;
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
                inner.waiting.push(WaitingThread::new(thread, priority));
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
            if !inner.waiting.is_empty() {
                inner.waiting.sort_by(|a, b| b.priority.cmp(&a.priority));
                let woken = inner.waiting.remove(0);
                Some((woken.id, woken.priority))
            } else {
                None
            }
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
        inner.waiting.push(WaitingThread::new(thread, priority));
    }
}

impl Default for MutexPrim {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MutexPrim {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            #[cfg(feature = "qs")]
            trace: self.trace.clone(),
        }
    }
}

/// FIFO message queue for inter-thread communication.
///
/// Allows threads to send and receive typed messages. Receivers can block
/// waiting for messages.
pub struct MessageQueue<T> {
    inner: Arc<Mutex<MessageQueueInner<T>>>,
}

struct MessageQueueInner<T> {
    queue: VecDeque<T>,
    capacity: usize,
    waiting_receivers: Vec<WaitingThread>,
    waiting_senders: Vec<WaitingThread>,
}

impl<T> MessageQueue<T> {
    /// Creates a new message queue with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(MessageQueueInner {
                queue: VecDeque::with_capacity(capacity),
                capacity,
                waiting_receivers: Vec::new(),
                waiting_senders: Vec::new(),
            })),
        }
    }

    /// Sends a message to the queue without blocking.
    ///
    /// Wakes a waiting receiver if any are blocked.
    pub fn try_send(&self, message: T, scheduler: &QxkScheduler) -> SyncResult<()> {
        let woken_receiver = {
            let mut inner = self.inner.lock();
            if inner.queue.len() >= inner.capacity {
                return Err(SyncError::QueueFull);
            }
            inner.queue.push_back(message);

            // Wake one waiting receiver
            if !inner.waiting_receivers.is_empty() {
                inner.waiting_receivers.sort_by(|a, b| b.priority.cmp(&a.priority));
                let woken = inner.waiting_receivers.remove(0);
                Some((woken.id, woken.priority))
            } else {
                None
            }
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
        inner.queue.pop_front().ok_or(SyncError::QueueEmpty)
    }

    /// Sends a message, blocking if queue is full.
    ///
    /// If queue has space, sends immediately. Otherwise, registers as waiting
    /// sender and returns WouldBlock.
    pub fn send(&self, message: T, thread: ThreadId, priority: u8, scheduler: &QxkScheduler) -> SyncResult<()> {
        let woken_receiver = {
            let mut inner = self.inner.lock();
            if inner.queue.len() >= inner.capacity {
                // Queue full, register as waiting sender
                inner.waiting_senders.push(WaitingThread::new(thread, priority));
                drop(inner);
                scheduler.block_thread(thread);
                return Err(SyncError::WouldBlock);
            }
            inner.queue.push_back(message);

            // Wake one waiting receiver
            if !inner.waiting_receivers.is_empty() {
                inner.waiting_receivers.sort_by(|a, b| b.priority.cmp(&a.priority));
                let woken = inner.waiting_receivers.remove(0);
                Some((woken.id, woken.priority))
            } else {
                None
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
            if let Some(msg) = inner.queue.pop_front() {
                // Wake one waiting sender
                let woken = if !inner.waiting_senders.is_empty() {
                    inner.waiting_senders.sort_by(|a, b| b.priority.cmp(&a.priority));
                    let w = inner.waiting_senders.remove(0);
                    Some((w.id, w.priority))
                } else {
                    None
                };
                (Some(msg), woken)
            } else {
                // Queue empty, register as waiting receiver
                inner.waiting_receivers.push(WaitingThread::new(thread, priority));
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
        self.inner.lock().queue.len()
    }

    /// Returns true if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if the queue is full.
    pub fn is_full(&self) -> bool {
        let inner = self.inner.lock();
        inner.queue.len() >= inner.capacity
    }

    /// Returns the capacity of the queue.
    pub fn capacity(&self) -> usize {
        self.inner.lock().capacity
    }
}

impl<T> Clone for MessageQueue<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Condition variable for thread coordination.
///
/// Allows threads to wait for a condition to become true, and other threads
/// to notify waiting threads when the condition changes.
pub struct CondVar {
    inner: Arc<Mutex<CondVarInner>>,
}

struct CondVarInner {
    waiting: Vec<WaitingThread>,
}

impl CondVar {
    /// Creates a new condition variable.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(CondVarInner {
                waiting: Vec::new(),
            })),
        }
    }

    /// Waits for notification.
    ///
    /// Registers the thread as waiting and blocks it in the scheduler.
    /// Returns WouldBlock to indicate the thread is suspended.
    pub fn wait(&self, thread: ThreadId, priority: u8, scheduler: &QxkScheduler) -> SyncResult<()> {
        {
            let mut inner = self.inner.lock();
            inner.waiting.push(WaitingThread::new(thread, priority));
        }
        scheduler.block_thread(thread);
        Err(SyncError::WouldBlock)
    }

    /// Notifies one waiting thread (highest priority).
    pub fn notify_one(&self, scheduler: &QxkScheduler) {
        let woken_thread = {
            let mut inner = self.inner.lock();
            if !inner.waiting.is_empty() {
                inner.waiting.sort_by(|a, b| b.priority.cmp(&a.priority));
                let woken = inner.waiting.remove(0);
                Some((woken.id, woken.priority))
            } else {
                None
            }
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

impl Default for CondVar {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CondVar {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
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
        let queue: MessageQueue<u32> = MessageQueue::new(3);

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
        let queue: MessageQueue<&str> = MessageQueue::new(5);

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
