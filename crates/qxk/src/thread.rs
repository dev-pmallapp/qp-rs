//! Extended thread support for QXK.
//!
//! Extended threads are traditional blocking threads with their own stacks,
//! unlike active objects which are non-blocking and event-driven.
//!
//! Extended threads can:
//! - Block on semaphores, mutexes, and message queues
//! - Call blocking APIs (file I/O, network operations, etc.)
//! - Have configurable stack sizes
//! - Run with priority-based scheduling (lower priority than active objects)

use core::fmt;

use crate::scheduler::QxkScheduler;

/// Action returned by a thread handler indicating what to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadAction {
    /// Continue running in the next dispatch cycle.
    Continue,
    /// Voluntarily yield to other threads.
    Yield,
    /// Thread is blocked waiting on a synchronization primitive.
    Blocked,
    /// Thread has completed execution.
    Terminated,
}

/// Context provided to thread handlers during polling.
///
/// Provides access to thread identity, priority, scheduler, and iteration count.
/// The scheduler is borrowed for the duration of the poll, so the context is
/// allocation-free on every build (no `Arc` handle is held).
pub struct ThreadContext<'a> {
    thread_id: ThreadId,
    priority: ThreadPriority,
    scheduler: &'a QxkScheduler,
    iteration: u64,
}

impl ThreadContext<'_> {
    /// Returns the thread ID.
    pub fn thread_id(&self) -> ThreadId {
        self.thread_id
    }

    /// Returns the thread priority.
    pub fn priority(&self) -> ThreadPriority {
        self.priority
    }

    /// Returns a reference to the scheduler.
    pub fn scheduler(&self) -> &QxkScheduler {
        self.scheduler
    }

    /// Returns the iteration count (number of times handler has been polled).
    pub fn iteration(&self) -> u64 {
        self.iteration
    }
}

/// Priority for extended threads.
///
/// Thread priorities are separate from active object priorities.
/// All active objects have higher priority than all extended threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadPriority(pub u8);

/// Thread execution state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    /// Thread is ready to run.
    Ready,
    /// Thread is currently executing.
    Running,
    /// Thread is blocked waiting for an event.
    Blocked,
    /// Thread has completed execution.
    Terminated,
}

/// Thread identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadId(pub u8);

/// Extended thread handler function type.
///
/// The handler is polled repeatedly by the scheduler. It receives a mutable
/// ThreadContext and returns a ThreadAction indicating what should happen next.
///
/// The handler must be `FnMut` to allow state mutation across polls.
///
/// On the dynamic build this is a heap-allocated `Box`. Under the heap-free
/// `static-alloc` build it is a `&'static mut` trait object — the handler lives
/// in application-owned `static` storage, so the kernel holds no allocation
/// (see `docs/FUSA.md`, Phase 2). Both are callable identically.
#[cfg(not(feature = "static-alloc"))]
pub type ThreadHandler = alloc::boxed::Box<dyn FnMut(&mut ThreadContext) -> ThreadAction + Send>;
/// Heap-free thread-handler: a `&'static mut` trait object. See the dynamic
/// variant above.
#[cfg(feature = "static-alloc")]
pub type ThreadHandler = &'static mut (dyn FnMut(&mut ThreadContext) -> ThreadAction + Send);

/// Erases a closure into a [`ThreadHandler`] (dynamic build): boxes it on the heap.
#[cfg(not(feature = "static-alloc"))]
pub fn thread_handler<F>(f: F) -> ThreadHandler
where
    F: FnMut(&mut ThreadContext) -> ThreadAction + Send + 'static,
{
    alloc::boxed::Box::new(f)
}

/// Erases a closure into a [`ThreadHandler`] (`static-alloc` + `std`): leaks the
/// closure to obtain a `&'static mut` handle.
///
/// This convenience exists for host tests, which have `std`; it deliberately
/// uses the heap (`Box::leak`). Genuine heap-free targets (no `std`) instead
/// place the closure in their own `static` storage and pass a `&'static mut`
/// reference directly — so this helper is absent there.
#[cfg(all(feature = "static-alloc", feature = "std"))]
pub fn thread_handler<F>(f: F) -> ThreadHandler
where
    F: FnMut(&mut ThreadContext) -> ThreadAction + Send + 'static,
{
    alloc::boxed::Box::leak(alloc::boxed::Box::new(f))
}

/// Configuration for creating an extended thread.
pub struct ThreadConfig {
    /// Thread identifier.
    pub id: ThreadId,
    /// Thread priority (lower priority than all active objects).
    pub priority: ThreadPriority,
    /// Stack size in bytes.
    pub stack_size: usize,
    /// Thread handler function.
    pub handler: ThreadHandler,
}

impl ThreadConfig {
    /// Creates a new thread configuration.
    pub fn new(id: ThreadId, priority: ThreadPriority, handler: ThreadHandler) -> Self {
        Self {
            id,
            priority,
            stack_size: 4096, // Default 4KB stack
            handler,
        }
    }

    /// Sets the stack size for the thread.
    pub fn with_stack_size(mut self, size: usize) -> Self {
        self.stack_size = size;
        self
    }
}

/// An extended thread in the QXK kernel.
///
/// Extended threads have their own stack and can block, unlike
/// active objects which are non-blocking and event-driven.
pub struct ExtendedThread {
    id: ThreadId,
    priority: ThreadPriority,
    state: ThreadState,
    /// Requested stack size in bytes. The cooperative poll model runs handlers
    /// on the dispatcher's stack, so no separate stack is allocated; this is
    /// retained for diagnostics and future preemptive ports.
    stack_size: usize,
    handler: Option<ThreadHandler>,
    iteration: u64,
}

impl ExtendedThread {
    /// Creates a new extended thread from configuration.
    pub fn new(config: ThreadConfig) -> Self {
        Self {
            id: config.id,
            priority: config.priority,
            state: ThreadState::Ready,
            stack_size: config.stack_size,
            handler: Some(config.handler),
            iteration: 0,
        }
    }

    /// Returns the thread ID.
    pub fn id(&self) -> ThreadId {
        self.id
    }

    /// Returns the thread priority.
    pub fn priority(&self) -> ThreadPriority {
        self.priority
    }

    /// Returns the current thread state.
    pub fn state(&self) -> ThreadState {
        self.state
    }

    /// Checks if the thread is ready to run.
    pub fn is_ready(&self) -> bool {
        self.state == ThreadState::Ready
    }

    /// Checks if the thread is blocked.
    pub fn is_blocked(&self) -> bool {
        self.state == ThreadState::Blocked
    }

    /// Checks if the thread has terminated.
    pub fn is_terminated(&self) -> bool {
        self.state == ThreadState::Terminated
    }

    #[cfg(test)]
    pub(crate) fn set_state(&mut self, state: ThreadState) {
        self.state = state;
    }

    /// Polls the thread handler, returning the action to take.
    ///
    /// This method is called by the scheduler to execute one iteration of the thread.
    /// The thread handler receives a context with scheduler access and returns an action
    /// indicating whether to continue, yield, block, or terminate.
    pub(crate) fn poll(&mut self, scheduler: &QxkScheduler) -> ThreadAction {
        if let Some(handler) = &mut self.handler {
            self.state = ThreadState::Running;

            let mut ctx = ThreadContext {
                thread_id: self.id,
                priority: self.priority,
                scheduler,
                iteration: self.iteration,
            };

            self.iteration += 1;
            let action = handler(&mut ctx);

            match action {
                ThreadAction::Terminated => {
                    self.state = ThreadState::Terminated;
                    self.handler = None;
                }
                ThreadAction::Blocked => {
                    self.state = ThreadState::Blocked;
                }
                ThreadAction::Yield | ThreadAction::Continue => {
                    self.state = ThreadState::Ready;
                }
            }

            action
        } else {
            // Handler already consumed (thread terminated)
            self.state = ThreadState::Terminated;
            ThreadAction::Terminated
        }
    }
}

impl fmt::Debug for ExtendedThread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtendedThread")
            .field("id", &self.id)
            .field("priority", &self.priority)
            .field("state", &self.state)
            .field("stack_size", &self.stack_size)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_creation() {
        let config = ThreadConfig::new(
            ThreadId(1),
            ThreadPriority(5),
            thread_handler(|_ctx| ThreadAction::Terminated),
        ).with_stack_size(8192);

        let thread = ExtendedThread::new(config);
        assert_eq!(thread.id(), ThreadId(1));
        assert_eq!(thread.priority(), ThreadPriority(5));
        assert_eq!(thread.state(), ThreadState::Ready);
        assert!(thread.is_ready());
    }

    #[test]
    fn thread_state_transitions() {
        let mut thread = ExtendedThread::new(ThreadConfig::new(
            ThreadId(2),
            ThreadPriority(3),
            thread_handler(|_ctx| ThreadAction::Terminated),
        ));

        assert_eq!(thread.state(), ThreadState::Ready);

        thread.set_state(ThreadState::Blocked);
        assert!(thread.is_blocked());

        thread.set_state(ThreadState::Ready);
        assert!(thread.is_ready());
    }

    #[test]
    fn thread_poll_lifecycle() {
        let scheduler = QxkScheduler::new(None);

        let mut thread = ExtendedThread::new(ThreadConfig::new(
            ThreadId(3),
            ThreadPriority(4),
            thread_handler(move |ctx| {
                if ctx.iteration() < 3 {
                    ThreadAction::Continue
                } else {
                    ThreadAction::Terminated
                }
            }),
        ));

        // Poll 1: Continue
        let action = thread.poll(&scheduler);
        assert_eq!(action, ThreadAction::Continue);
        assert_eq!(thread.state(), ThreadState::Ready);

        // Poll 2: Continue
        let action = thread.poll(&scheduler);
        assert_eq!(action, ThreadAction::Continue);
        assert_eq!(thread.state(), ThreadState::Ready);

        // Poll 3: Continue
        let action = thread.poll(&scheduler);
        assert_eq!(action, ThreadAction::Continue);
        assert_eq!(thread.state(), ThreadState::Ready);

        // Poll 4: Terminated
        let action = thread.poll(&scheduler);
        assert_eq!(action, ThreadAction::Terminated);
        assert_eq!(thread.state(), ThreadState::Terminated);
        assert!(thread.is_terminated());

        // Further polls return Terminated
        let action = thread.poll(&scheduler);
        assert_eq!(action, ThreadAction::Terminated);
    }
}
